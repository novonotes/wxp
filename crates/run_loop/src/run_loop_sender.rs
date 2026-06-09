use std::fmt::Debug;

use std::sync::{
    Weak,
    atomic::{AtomicBool, Ordering},
};

use crate::{SystemThreadId, get_system_thread_id, platform::PlatformRunLoopSender};

/// A `Send + Clone` handle for posting callbacks onto a run loop from any thread.
///
/// This is the only sanctioned way to reach the run loop thread from background
/// work: it lets `!Send` thread-affine state (native windows, WebView channels)
/// stay on its owning thread while other threads merely enqueue work for it.
#[derive(Clone)]
pub struct RunLoopSender {
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

    /// Returns true if this sender targets the current thread.
    pub fn is_same_thread(&self) -> bool {
        get_system_thread_id() == self.thread_id
    }

    /// Schedules the callback to be executed on run loop and returns immediately.
    pub fn send<F>(&self, callback: F) -> bool
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

    pub(crate) fn send_shutdown_barrier<F>(&self, callback: F) -> bool
    where
        F: FnOnce() + 'static + Send,
    {
        // Shutdown marks the token before posting this barrier so every other
        // cloned sender stops accepting plugin/DLL callbacks. The barrier itself
        // is the one callback that must still reach the run-loop thread to abort
        // !Send tasks and clear platform state at an event-loop boundary.
        self.platform_sender.send(callback)
    }
}
