/// RAII guard for a scheduled/registered operation.
///
/// Holding the handle keeps the operation pending; dropping it cancels the
/// operation (runs `on_cancel`). This makes "stop on scope exit" the default and
/// safe choice — callers must opt out explicitly via [`detach`](Self::detach)
/// to let the operation outlive the handle.
pub struct Handle {
    // `None` once the cancel action has been consumed by `cancel`/`detach`,
    // which also makes both operations idempotent.
    on_cancel: Option<Box<dyn FnOnce()>>,
}

impl Handle {
    pub fn new<F>(on_cancel: F) -> Self
    where
        F: FnOnce() + 'static,
    {
        Self {
            on_cancel: Some(Box::new(on_cancel)),
        }
    }

    /// Cancels now instead of waiting for drop. Safe to call more than once.
    pub fn cancel(&mut self) {
        if let Some(on_cancel) = self.on_cancel.take() {
            on_cancel();
        }
    }

    /// Lets the operation keep running after the handle is gone.
    ///
    /// Use this for fire-and-forget work; without it, dropping the handle would
    /// cancel the very operation that was just scheduled.
    pub fn detach(&mut self) {
        self.on_cancel.take();
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.cancel();
    }
}
