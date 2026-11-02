use std::fmt::Debug;

use std::sync::{
    Weak,
    atomic::{AtomicBool, Ordering},
};

use crate::{SystemThreadId, get_system_thread_id, platform::PlatformRunLoopSender};

// Internal cross-thread enqueue handle.
//
// Public callers should use `RunLoop::post` or `RunLoop::call`. Keeping this
// private lets thread-affine state stay on the run loop thread while internal
// tasks and wakers can still enqueue work from other threads.
#[derive(Clone)]
pub(crate) struct RunLoopSender {
    thread_id: SystemThreadId,
    platform_sender: PlatformRunLoopSender,
    shutdown_token: Weak<AtomicBool>,
}

impl Debug for RunLoopSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunLoopSender")
            .field("thread_id", &self.thread_id)
            .finish()
    }
}

impl RunLoopSender {
    pub(crate) fn new(
        platform_sender: PlatformRunLoopSender,
        shutdown_token: Weak<AtomicBool>,
    ) -> Self {
        Self {
            thread_id: get_system_thread_id(),
            platform_sender,
            shutdown_token,
        }
    }

    pub(crate) fn send<F>(&self, callback: F) -> bool
    where
        F: FnOnce() + 'static + Send,
    {
        let Some(shutdown_token) = self.shutdown_token.upgrade() else {
            return false;
        };
        if shutdown_token.load(Ordering::Acquire) {
            return false;
        }
        let sent = self.platform_sender.send(callback);
        sent && !shutdown_token.load(Ordering::Acquire)
    }
}
