use std::{cell::RefCell, rc::Rc, task::Poll};

use futures::Future;

// Single-threaded one-shot future.
//
// A `CompletableFuture` paired with a `FutureCompleter`: awaiting the future
// parks until `complete` is called with the value. Deliberately `Rc`/`RefCell`
// based (no synchronization) because both ends live on the same run loop
// thread — this is the building block behind things like `RunLoop::delay`,
// where a scheduled callback completes the future.

struct State<T> {
    waker: Option<std::task::Waker>,
    data: Option<T>,
}

pub struct FutureCompleter<T> {
    state: Rc<RefCell<State<T>>>,
}

impl<T> FutureCompleter<T> {
    pub fn new() -> (CompletableFuture<T>, FutureCompleter<T>) {
        let state = Rc::new(RefCell::new(State {
            waker: None,
            data: None,
        }));
        (
            CompletableFuture {
                state: state.clone(),
            },
            FutureCompleter { state },
        )
    }

    pub fn complete(self, data: T) {
        let waker = {
            let mut state = self.state.borrow_mut();
            state.data.replace(data);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

pub struct CompletableFuture<T> {
    state: Rc<RefCell<State<T>>>,
}

impl<T> Future for CompletableFuture<T> {
    type Output = T;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.borrow_mut();
        let data = state.data.take();
        match data {
            Some(data) => Poll::Ready(data),
            None => {
                state.waker.get_or_insert_with(|| cx.waker().clone());
                Poll::Pending
            }
        }
    }
}
