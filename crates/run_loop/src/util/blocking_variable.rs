use std::sync::{Arc, Condvar, Mutex};

/// A one-shot cross-thread rendezvous slot.
///
/// One side blocks in [`get_blocking`](Self::get_blocking) until another side
/// calls [`set`](Self::set). This backs [`RunLoop::call`](crate::RunLoop::call)
/// for cross-thread calls: the caller parks here while the run loop thread
/// produces the value. `RunLoop::call` runs inline on the run loop thread, so
/// this blocking path is used only when the caller is on another thread.
pub(crate) struct BlockingVariable<T: Send> {
    state: Arc<(Mutex<Option<T>>, Condvar)>,
}

// Derive(Clone) doesn't work with Arc if T is not Clone
impl<T: Send> Clone for BlockingVariable<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<T: Send> BlockingVariable<T> {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new((Mutex::new(None), Condvar::new())),
        }
    }

    pub(crate) fn set(&self, v: T) {
        let mut lock = self.state.0.lock().unwrap();
        lock.replace(v);
        self.state.1.notify_all();
    }

    pub(crate) fn get_blocking(&self) -> T {
        let mut lock = self.state.0.lock().unwrap();
        while lock.is_none() {
            lock = self.state.1.wait(lock).unwrap();
        }
        lock.take().unwrap()
    }
}
