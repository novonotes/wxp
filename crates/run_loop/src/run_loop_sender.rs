use std::fmt::Debug;

use crate::{
    RunLoop, SystemThreadId, get_system_thread_id, main_thread::MainThreadFacilitator,
    platform::PlatformRunLoopSender, util::BlockingVariable,
};

/// A `Send + Clone` handle for posting callbacks onto a run loop from any thread.
///
/// This is the only sanctioned way to reach the run loop thread from background
/// work: it lets `!Send` thread-affine state (native windows, WebView channels)
/// stay on its owning thread while other threads merely enqueue work for it.
#[derive(Clone)]
pub struct RunLoopSender {
    inner: RunLoopSenderInner,
}

#[derive(Clone)]
enum RunLoopSenderInner {
    /// Targets a specific run loop captured at creation (`thread_id` is kept so
    /// `is_same_thread` can short-circuit when already on that thread).
    PlatformSender {
        thread_id: SystemThreadId,
        platform_sender: PlatformRunLoopSender,
    },
    /// Targets "the main thread" indirectly via `MainThreadFacilitator`. Used
    /// when the concrete run loop is owned elsewhere (e.g. a Flutter engine).
    MainThreadSender,
}

impl Debug for RunLoopSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            RunLoopSenderInner::PlatformSender {
                thread_id,
                platform_sender: _,
            } => f
                .debug_struct("RunLoopSender")
                .field("thread_id", &thread_id)
                .finish(),
            RunLoopSenderInner::MainThreadSender => f
                .debug_struct("RunLoopSender")
                .field("thread_id", &"main")
                .finish(),
        }
    }
}

impl RunLoopSender {
    pub(crate) fn new(platform_sender: PlatformRunLoopSender) -> Self {
        Self {
            inner: RunLoopSenderInner::PlatformSender {
                thread_id: get_system_thread_id(),
                platform_sender,
            },
        }
    }

    /// Creates sender for run loop thread (Normally main thread). This should only be called from
    /// background threads. On run loop thread the RunLoop should create regular
    /// sender from current run loop.
    ///
    /// The reason is that the run loop thread sender, when invoking on run loop thread,
    /// may execute the callback synchronously instead of scheduling it (linux),
    /// which is not how regular run loop sender works.
    #[allow(unused)] // not used in tests
    pub(crate) fn new_for_run_loop_thread() -> Self {
        debug_assert!(!RunLoop::is_run_loop_thread());
        Self {
            inner: RunLoopSenderInner::MainThreadSender,
        }
    }

    /// Returns true if sender would send the callback to current thread.
    pub fn is_same_thread(&self) -> bool {
        match self.inner {
            RunLoopSenderInner::PlatformSender {
                thread_id,
                platform_sender: _,
            } => get_system_thread_id() == thread_id,
            // A `MainThreadSender` is only created after the `MainThreadFacilitator`
            // was confirmed initialized, so resolving "same thread" via the run
            // loop thread check here cannot fail.
            RunLoopSenderInner::MainThreadSender => RunLoop::is_run_loop_thread(),
        }
    }

    /// Schedules the callback to be executed on run loop and returns immediately.
    pub fn send<F>(&self, callback: F)
    where
        F: FnOnce() + 'static + Send,
    {
        match &self.inner {
            RunLoopSenderInner::PlatformSender {
                thread_id: _,
                platform_sender,
            } => {
                platform_sender.send(callback);
            }
            RunLoopSenderInner::MainThreadSender => {
                // `unwrap` is sound: a `MainThreadSender` is only constructed
                // after `MainThreadFacilitator::is_main_thread()` succeeded
                // (see `RunLoop::sender`), which means the facilitator was
                // already initialized — via `init()` (Manual) or the Flutter
                // engine context. So `perform_on_main_thread` cannot hit the
                // uninitialized path here.
                MainThreadFacilitator::perform_on_main_thread(callback).unwrap();
            }
        }
    }

    /// Schedules the callback on run loop and blocks until it is invoked.
    /// If current thread is run loop thread the callback will be invoked immediately
    /// (otherwise it would deadlock).
    pub fn send_and_wait<F, R>(&self, callback: F) -> R
    where
        F: FnOnce() -> R + 'static + Send,
        R: Send + 'static,
    {
        if self.is_same_thread() {
            callback()
        } else {
            let var = BlockingVariable::<R>::new();
            let var_clone = var.clone();
            self.send(move || {
                var_clone.set(callback());
            });
            var.get_blocking()
        }
    }
}
