use std::{
    any::Any,
    cell::{RefCell, UnsafeCell},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
};

use futures::{
    Future, FutureExt,
    future::LocalBoxFuture,
    task::{ArcWake, waker_ref},
};

use crate::RunLoopSender;

// タスクのabort機能を抽象化するトレイト
pub(crate) trait AbortableTask: Send + Sync {
    #[allow(dead_code)] // Drop実装でのみ使用
    fn abort(&self);
}

/// Task 実行中に発生するエラー
#[derive(Debug)]
pub enum JoinError {
    /// タスクが abort() で中断された
    Aborted,
    /// タスク内で panic が発生した
    Panic(Box<dyn Any + Send>),
}

impl JoinError {
    /// エラーがキャンセルによるものかチェック
    pub fn is_aborted(&self) -> bool {
        matches!(self, Self::Aborted)
    }

    /// エラーが panic によるものかチェック
    pub fn is_panic(&self) -> bool {
        matches!(self, Self::Panic(_))
    }
}

pub struct Task<T> {
    sender: RunLoopSender,
    future: UnsafeCell<Option<LocalBoxFuture<'static, T>>>,
    value: RefCell<Option<Result<T, JoinError>>>,
    waker: RefCell<Option<std::task::Waker>>,
    aborted: AtomicBool,
}

// Tasks can only be spawned on run loop thread and will only be executed
// on run loop thread. ArcWake however doesn't know this.
unsafe impl<T> Send for Task<T> {}
unsafe impl<T> Sync for Task<T> {}

impl<T: 'static> Task<T> {
    pub(crate) fn new<F>(sender: RunLoopSender, future: F) -> Self
    where
        F: Future<Output = T> + 'static,
        T: 'static,
    {
        let future = future.boxed_local();
        Self {
            sender,
            future: UnsafeCell::new(Some(future)),
            value: RefCell::new(None),
            waker: RefCell::new(None),
            aborted: AtomicBool::new(false),
        }
    }

    fn poll(self: &std::sync::Arc<Self>) -> Poll<Result<T, JoinError>> {
        if self.aborted.load(Ordering::Acquire) {
            return Poll::Ready(Err(JoinError::Aborted));
        }

        let waker = waker_ref(self).clone();
        let context = &mut core::task::Context::from_waker(&waker);
        unsafe {
            let future_opt = &mut *self.future.get();
            match future_opt {
                Some(future) => {
                    // panic をキャッチしてエラーとして扱う
                    match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
                        Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
                        Ok(Poll::Pending) => Poll::Pending,
                        Err(panic_payload) => Poll::Ready(Err(JoinError::Panic(panic_payload))),
                    }
                }
                None => Poll::Ready(Err(JoinError::Aborted)),
            }
        }
    }

    pub(crate) fn abort(&self) {
        self.aborted.store(true, Ordering::Release);
        // Future を drop してリソースを解放
        unsafe {
            let future_opt = &mut *self.future.get();
            *future_opt = None;
        }
    }
}

impl<T: 'static> AbortableTask for Task<T> {
    fn abort(&self) {
        Task::abort(self);
    }
}

impl<T: 'static> ArcWake for Task<T> {
    fn wake_by_ref(arc_self: &std::sync::Arc<Self>) {
        let arc_self = arc_self.clone();
        let sender = arc_self.sender.clone();
        sender.send(move || {
            if arc_self.value.borrow().is_none() && !arc_self.aborted.load(Ordering::Acquire) {
                if let Poll::Ready(result) = arc_self.poll() {
                    *arc_self.value.borrow_mut() = Some(result);
                }
            }
            if arc_self.value.borrow().is_some() || arc_self.aborted.load(Ordering::Acquire) {
                if let Some(waker) = arc_self.waker.borrow_mut().take() {
                    waker.wake();
                }
            }
        });
    }
}

pub struct JoinHandle<T> {
    task: Arc<Task<T>>,
}

impl<T: 'static> JoinHandle<T> {
    pub(crate) fn new(task: Arc<Task<T>>) -> Self {
        Self { task }
    }

    /// Aborts the task, dropping the future and preventing further execution.
    /// The JoinHandle will return Err(JoinError::Cancelled) when polled after abort.
    ///
    /// # Warning
    ///
    /// This method forcibly terminates the running task immediately. Be aware that:
    /// - File operations may be interrupted, leaving incomplete data
    /// - Locks (Mutex, RwLock) may not be properly released if held during abort
    /// - Resources like file handles or network connections may not be cleaned up
    pub fn abort(&self) {
        self.task.abort();
    }
}

impl<T: 'static> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        if self.task.aborted.load(Ordering::Acquire) {
            return Poll::Ready(Err(JoinError::Aborted));
        }

        let value = self.task.value.borrow_mut().take();
        match value {
            Some(result) => Poll::Ready(result),
            None => {
                self.task
                    .waker
                    .borrow_mut()
                    .get_or_insert_with(|| cx.waker().clone());
                Poll::Pending
            }
        }
    }
}
