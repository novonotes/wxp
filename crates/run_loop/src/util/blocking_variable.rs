use std::sync::{Arc, Condvar, Mutex};

/// A one-shot cross-thread rendezvous slot.
///
/// One side blocks in [`get_blocking`](Self::get_blocking) until another side
/// calls [`set`](Self::set). This backs `RunLoopSender::send_and_wait`: the
/// caller parks here while the run loop thread produces the value. The caller is
/// responsible for not blocking *on its own* run loop thread (that would
/// deadlock); `send_and_wait` handles that check.
pub struct BlockingVariable<T: Send> {
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
    pub fn new() -> Self {
        Self {
            state: Arc::new((Mutex::new(None), Condvar::new())),
        }
    }

    pub fn set(&self, v: T) {
        let mut lock = self.state.0.lock().unwrap();
        lock.replace(v);
        self.state.1.notify_all();
    }

    pub fn get_blocking(&self) -> T {
        let mut lock = self.state.0.lock().unwrap();
        while lock.is_none() {
            lock = self.state.1.wait(lock).unwrap();
        }
        lock.take().unwrap()
    }
}
