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

// Type-erased abort hook so the run loop can cancel a task without knowing its
// output type `T`. Only the `JoinHandle`/`Drop` paths call it.
pub(crate) trait AbortableTask: Send + Sync {
    #[allow(dead_code)] // only used in Drop impl
    fn abort(&self);
}

/// Errors that can occur while a Task is running.
#[derive(Debug)]
pub enum JoinError {
    /// The task was cancelled via `abort()`.
    Aborted,
    /// A panic occurred inside the task.
    Panic(Box<dyn Any + Send>),
}

impl JoinError {
    /// Returns `true` if the error was caused by a cancellation.
    pub fn is_aborted(&self) -> bool {
        matches!(self, Self::Aborted)
    }

    /// Returns `true` if the error was caused by a panic.
    pub fn is_panic(&self) -> bool {
        matches!(self, Self::Panic(_))
    }
}

/// A spawned future plus the slot where its result waits to be joined.
///
/// The future runs to completion via repeated `poll`s driven by its own
/// `ArcWake`; the outcome is parked in `value` until a `JoinHandle` collects it,
/// with `waker` bridging "result is ready" back to whoever awaits the handle.
pub struct Task<T> {
    sender: RunLoopSender,
    future: UnsafeCell<Option<LocalBoxFuture<'static, T>>>,
    value: RefCell<Option<Result<T, JoinError>>>,
    waker: RefCell<Option<std::task::Waker>>,
    aborted: AtomicBool,
}

// SAFETY: a Task is spawned and polled only on its run loop thread (the waker
// re-posts the poll back there rather than polling inline), so the `!Send`
// interior (`UnsafeCell`/`RefCell`/the future) is never *accessed*
// concurrently. The impls exist solely because `ArcWake` requires `Send + Sync`
// to build a `Waker`. Note this does not by itself guarantee the final `Arc`
// (or the future inside it) is dropped on the run loop thread — callers that
// move a `JoinHandle` across threads are responsible for that.
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
                    // Catch panics and treat them as an error
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
        // Drop the Future to release resources
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
        // A wake can fire from any thread, but the future is `!Send` and must be
        // polled on its run loop thread — so bounce the actual poll back there
        // via the sender rather than polling inline.
        sender.send(move || {
            // Guard against redundant wakes: skip the poll if the task already
            // produced a value or was aborted, then notify the joiner if the
            // result is now available.
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

    /// Aborts the task: drops its future and makes future polls of this handle
    /// resolve to `Err(JoinError::Aborted)`.
    ///
    /// # Warning
    ///
    /// Cancellation happens by dropping the future at its current await point,
    /// not by unwinding it — code between await points never gets to run its
    /// cleanup. Concretely:
    /// - in-flight I/O may be left half-written,
    /// - guards held across an await may not run their intended teardown,
    /// - external handles/connections owned by the future are just dropped.
    ///
    /// Prefer a cooperative cancellation signal when any of that matters.
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
