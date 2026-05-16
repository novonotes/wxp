//! A timer that executes repeating callbacks on `novonotes_run_loop`.
//!
//! Use this when you want periodic processing on the GUI thread without relying on host callbacks.
//! Because callbacks do not require `Send`, thread-affine values such as native GUI objects and
//! WebView channels can be captured directly.

use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::{Rc, Weak},
    time::Duration,
};

use novonotes_run_loop::{Handle, RunLoop};

trait TimerCallback: 'static {
    fn execute(&self, run_loop: &RunLoop);
}

struct SyncCallback<F>
where
    F: Fn() + 'static,
{
    callback: F,
}

impl<F> TimerCallback for SyncCallback<F>
where
    F: Fn() + 'static,
{
    fn execute(&self, _run_loop: &RunLoop) {
        (self.callback)();
    }
}

struct AsyncCallback<F, Fut>
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    callback: F,
}

impl<F, Fut> TimerCallback for AsyncCallback<F, Fut>
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn execute(&self, run_loop: &RunLoop) {
        run_loop.spawn((self.callback)());
    }
}

struct TimerInner {
    is_running: Cell<bool>,
    current_handle: RefCell<Option<Handle>>,
    interval: Duration,
    callback: Box<dyn TimerCallback>,
}

/// A repeating timer that must run on the RunLoop thread.
///
/// Creation, starting, stopping, and dropping must all happen on the same run loop thread.
/// Calling `stop()` or dropping the timer cancels the next scheduled tick.
///
/// Because RunLoop timer cadence depends on the OS and runtime environment, this type does not
/// guarantee exact fire counts or high-precision periods for a given interval. It is intended
/// for use cases that tolerate jitter or skipped ticks, such as reflecting GUI state.
pub struct Timer {
    inner: Rc<TimerInner>,
}

impl Timer {
    /// Creates a timer that calls the synchronous `callback` at each `interval`.
    ///
    /// The timer does not fire until [`start`](Self::start) is called. Because the callback runs
    /// on the run loop thread, it does not need to be `Send`, so native objects can be captured
    /// directly.
    pub fn new<F>(interval: Duration, callback: F) -> Self
    where
        F: Fn() + 'static,
    {
        Self {
            inner: Rc::new(TimerInner {
                is_running: Cell::new(false),
                current_handle: RefCell::new(None),
                interval,
                callback: Box::new(SyncCallback { callback }),
            }),
        }
    }

    /// Helper for creating a timer that holds callback-local state.
    ///
    /// Not used in the template itself, but kept as a public API to avoid forcing callers to
    /// write `Rc<RefCell<_>>` boilerplate when adding per-plugin GUI polling or lightweight
    /// debounce state.
    pub fn new_with_state<T, F>(interval: Duration, initial_state: T, callback: F) -> Self
    where
        T: 'static,
        F: Fn(&mut T) + 'static,
    {
        let state = Rc::new(RefCell::new(initial_state));
        let state_for_callback = state.clone();
        Self::new(interval, move || {
            callback(&mut state_for_callback.borrow_mut());
        })
    }

    /// Async variant of the timer that spawns a future at each `interval`.
    ///
    /// Each tick schedules the next interval without waiting for the previous future to complete,
    /// so long-running futures do not stall the tick cadence. Note that a future already spawned
    /// continues running on the run loop even after the timer is dropped.
    pub fn new_async<F, Fut>(interval: Duration, callback: F) -> Self
    where
        F: Fn() -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        Self {
            inner: Rc::new(TimerInner {
                is_running: Cell::new(false),
                current_handle: RefCell::new(None),
                interval,
                callback: Box::new(AsyncCallback { callback }),
            }),
        }
    }

    /// Starts repeating ticks. Calling `start` again while already running is a no-op.
    ///
    /// Calling from outside the run loop thread is a misuse; a debug-mode panic surfaces this
    /// early.
    pub fn start(&self) {
        debug_assert!(
            RunLoop::try_current().is_ok(),
            "Timer must be started on the initialized RunLoop thread"
        );

        // Re-scheduling when already running would double the tick rate; bail out.
        if self.inner.is_running.replace(true) {
            return;
        }

        self.schedule_next();
    }

    /// Stops repeating ticks and cancels the next scheduled tick. The timer can be restarted.
    pub fn stop(&self) {
        self.inner.is_running.set(false);

        if let Some(mut handle) = self.inner.current_handle.borrow_mut().take() {
            handle.cancel();
        }
    }

    /// Returns `true` if ticks are currently scheduled.
    pub fn is_running(&self) -> bool {
        self.inner.is_running.get()
    }

    fn schedule_next(&self) {
        // Passing a `Weak` reference means a queued tick will fail to upgrade and stop naturally
        // if the Timer is dropped before the tick fires. Passing an `Rc` would keep the Timer
        // alive as long as the queue holds a reference, making drop unable to stop it.
        let weak_inner = Rc::downgrade(&self.inner);
        let interval = self.inner.interval;

        let handle = RunLoop::current().schedule(interval, move || {
            run_timer_tick(weak_inner);
        });

        *self.inner.current_handle.borrow_mut() = Some(handle);
    }
}

// One tick: execute the callback, then re-schedule for the next interval
// (fixed-delay: the next tick is queued after callback completion, not at a fixed frequency).
fn run_timer_tick(weak_inner: Weak<TimerInner>) {
    // The Timer has already been dropped; nothing to do.
    let Some(inner) = weak_inner.upgrade() else {
        return;
    };

    // Discard a tick that was stopped after scheduling but before execution.
    // Without this check, one extra fire could occur immediately after stop().
    if !inner.is_running.get() {
        return;
    }

    let run_loop = RunLoop::current();
    inner.callback.execute(&run_loop);

    let interval = inner.interval;
    let weak_inner_for_next = weak_inner.clone();
    let handle = run_loop.schedule(interval, move || {
        run_timer_tick(weak_inner_for_next);
    });

    *inner.current_handle.borrow_mut() = Some(handle);
}

impl Drop for Timer {
    // Ensure the timer stops on drop even if stop() was never called explicitly.
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use novonotes_run_loop::test_helper::run_async;
    use serial_test::serial;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    #[test]
    #[serial]
    fn timer_state_tracks_start_and_stop() {
        run_async(async {
            let timer = Timer::new(Duration::from_millis(100), || {});

            assert!(!timer.is_running());
            timer.start();
            assert!(timer.is_running());
            timer.stop();
            assert!(!timer.is_running());
        });
    }

    #[test]
    #[serial]
    fn stop_prevents_future_ticks_after_observed_fire() {
        run_async(async {
            let counter = Arc::new(AtomicU32::new(0));
            let counter_clone = counter.clone();

            let timer = Timer::new(Duration::from_millis(100), move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });

            timer.start();
            RunLoop::current().delay(Duration::from_millis(150)).await;
            timer.stop();
            let count_at_stop = counter.load(Ordering::SeqCst);
            RunLoop::current().delay(Duration::from_millis(200)).await;

            // The exact count is not part of the Timer contract. Headless CI and
            // GUI hosts may run the underlying RunLoop timers at different cadences.
            assert!(count_at_stop >= 1, "timer did not fire before stop");
            assert_eq!(counter.load(Ordering::SeqCst), count_at_stop);
        });
    }

    #[test]
    #[serial]
    fn async_task_continues_after_timer_drop() {
        run_async(async {
            let task_started = Arc::new(AtomicBool::new(false));
            let task_completed = Arc::new(AtomicBool::new(false));
            let task_started_clone = task_started.clone();
            let task_completed_clone = task_completed.clone();

            {
                let timer = Timer::new_async(Duration::from_millis(20), move || {
                    let started = task_started_clone.clone();
                    let completed = task_completed_clone.clone();
                    async move {
                        started.store(true, Ordering::SeqCst);
                        RunLoop::current().delay(Duration::from_millis(200)).await;
                        completed.store(true, Ordering::SeqCst);
                    }
                });

                timer.start();
                wait_until_true(&task_started).await;
            }

            // Dropping the timer must not cancel a task that has already been spawned, but the
            // completion delay is still driven by the platform run loop. Wait for the observable
            // condition instead of assuming CI delivers the 200ms delay within a fixed margin.
            wait_until_true(&task_completed).await;
        });
    }

    #[test]
    #[serial]
    #[should_panic(expected = "Timer must be started on the initialized RunLoop thread")]
    #[cfg(debug_assertions)]
    fn timer_panics_without_run_loop_in_debug() {
        let timer = Timer::new(Duration::from_millis(100), || {});
        timer.start();
    }

    async fn wait_until_true(flag: &AtomicBool) {
        // Observe the event instead of assuming a fixed timer cadence. CFRunLoop
        // timers can run slower in headless CI or host-specific GUI environments.
        for _ in 0..50 {
            if flag.load(Ordering::SeqCst) {
                return;
            }
            RunLoop::current().delay(Duration::from_millis(20)).await;
        }

        panic!("condition was not observed before timeout");
    }
}
