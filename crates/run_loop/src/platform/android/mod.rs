mod sys;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::c_int,
    mem::ManuallyDrop,
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(crate) type HandleType = usize;
pub(crate) const INVALID_HANDLE: HandleType = 0;

use sys::{libc::*, ndk_sys::*};

use self::sys::libc;

/// Android run loop driven by the thread's `ALooper`.
///
/// Two file descriptors are registered with the looper so it wakes for our work
/// without us owning the looper's poll loop:
/// - `pipes`: a self-pipe; cross-thread senders write a byte to wake the looper
///   and have queued callbacks drained.
/// - `state.timer_fd`: a `timerfd` armed for the next scheduled callback.
///
/// `state_ptr` is a leaked `Weak<State>` handed to the C callbacks and reclaimed
/// in `Drop`. A `Weak` (not `Rc`) is used so a pending looper callback can never
/// keep `State` alive past this object's lifetime.
pub(crate) struct PlatformRunLoop {
    looper: *mut ALooper,
    pipes: [c_int; 2],
    state: Rc<State>,
    state_ptr: *const State,
    running: Cell<bool>,
    is_shutdown: Cell<bool>,
}

struct Timer {
    scheduled: Instant,
    callback: Box<dyn FnOnce()>,
}

struct State {
    timer_fd: c_int,
    callbacks: Arc<Mutex<Callbacks>>,
    next_handle: Cell<HandleType>,
    timers: RefCell<HashMap<HandleType, Timer>>,
}

type SenderCallback = Box<dyn FnOnce() + Send>;

pub(crate) struct PollSession {
    /// Polling state for `RunLoop::block_on`.
    ///
    /// For the first few milliseconds, poll non-blocking aggressively.
    /// After that, switch to blocking wait on the same looper.
    start: Instant,
    timed_out: bool,
}

impl PollSession {
    pub(crate) fn new() -> Self {
        Self {
            start: Instant::now(),
            timed_out: false,
        }
    }
}

struct Callbacks {
    fd: c_int,
    callbacks: Vec<SenderCallback>,
    is_shutdown: bool,
}

#[allow(unused_variables)]
impl PlatformRunLoop {
    pub(crate) fn new() -> Self {
        let looper = unsafe {
            let mut looper = ALooper_forThread();
            if looper.is_null() {
                looper = ALooper_prepare(0);
            }
            ALooper_acquire(looper);
            looper
        };
        let mut pipes: [c_int; 2] = [0, 2];
        unsafe { pipe(pipes.as_mut_ptr()) };

        let timer_fd = unsafe { timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK) };

        let state = Rc::new(State {
            timer_fd,
            callbacks: Arc::new(Mutex::new(Callbacks {
                fd: pipes[1],
                callbacks: Vec::new(),
                is_shutdown: false,
            })),
            next_handle: Cell::new(INVALID_HANDLE + 1),
            timers: RefCell::new(HashMap::new()),
        });

        let state_ptr = Weak::into_raw(Rc::downgrade(&state));

        unsafe {
            ALooper_addFd(
                looper,
                pipes[0],
                0,
                ALOOPER_EVENT_INPUT as c_int,
                Some(Self::looper_cb),
                state_ptr as *mut _,
            );
            ALooper_addFd(
                looper,
                timer_fd,
                0,
                ALOOPER_EVENT_INPUT as c_int,
                Some(Self::looper_timer_cb),
                state_ptr as *mut _,
            );
        }

        Self {
            looper,
            pipes,
            state,
            state_ptr,
            running: Cell::new(false),
            is_shutdown: Cell::new(false),
        }
    }

    unsafe extern "C" fn looper_cb(
        fd: ::std::ffi::c_int,
        events: ::std::ffi::c_int,
        data: *mut ::std::ffi::c_void,
    ) -> ::std::ffi::c_int {
        // Drain the wake byte(s) so the fd does not stay signaled.
        let mut buf = [0u8; 8];
        read(fd, buf.as_mut_ptr() as *mut _, buf.len());

        // `data` is the leaked `Weak<State>`. Reborrow it via `ManuallyDrop` so
        // this callback does NOT consume the single weak ref (that ref is owned
        // by `PlatformRunLoop` and freed in its `Drop`). `upgrade` failing means
        // the run loop is gone — just ignore the event.
        let state = data as *const State;
        let state = ManuallyDrop::new(Weak::from_raw(state));
        if let Some(state) = state.upgrade() {
            state.process_callbacks();
        }
        1
    }

    unsafe extern "C" fn looper_timer_cb(
        fd: ::std::ffi::c_int,
        events: ::std::ffi::c_int,
        data: *mut ::std::ffi::c_void,
    ) -> ::std::ffi::c_int {
        // Same borrow-without-consuming dance as `looper_cb`; here the wake
        // source is the timerfd firing for due timers.
        let mut buf = [0u8; 8];
        read(fd, buf.as_mut_ptr() as *mut _, buf.len());

        let state = data as *const State;
        let state = ManuallyDrop::new(Weak::from_raw(state));
        if let Some(state) = state.upgrade() {
            state.process_timers();
        }
        1
    }

    pub(crate) fn poll_once(&self, poll_session: &mut PollSession) {
        let timeout_ms = if poll_session.timed_out { -1 } else { 0 };
        unsafe {
            ALooper_pollOnce(
                timeout_ms,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if !poll_session.timed_out {
            poll_session.timed_out = poll_session.start.elapsed() >= Duration::from_millis(6);
        }
    }

    pub(crate) fn unschedule(&self, handle: HandleType) {
        if self.is_shutdown.get() {
            return;
        }
        self.state.unschedule(handle);
    }

    #[must_use]
    pub(crate) fn schedule<F>(&self, in_time: Duration, callback: F) -> HandleType
    where
        F: FnOnce() + 'static,
    {
        if self.is_shutdown.get() {
            return INVALID_HANDLE;
        }
        self.state.schedule(in_time, callback)
    }

    pub(crate) fn new_sender(&self) -> PlatformRunLoopSender {
        PlatformRunLoopSender {
            callbacks: Arc::downgrade(&self.state.callbacks),
        }
    }

    pub(crate) fn run(&self) {
        self.running.set(true);
        while self.running.get() {
            let res = unsafe {
                ALooper_pollOnce(
                    -1,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if res == ALOOPER_POLL_TIMEOUT || res == ALOOPER_POLL_ERROR {
                self.running.set(false);
            }
        }
    }

    pub(crate) fn stop(&self) {
        self.running.set(false);
        unsafe { ALooper_wake(self.looper) };
    }

    pub(crate) fn shutdown(&self) {
        if self.is_shutdown.replace(true) {
            return;
        }

        self.running.set(false);
        unsafe {
            ALooper_removeFd(self.looper, self.pipes[0]);
            ALooper_removeFd(self.looper, self.state.timer_fd);
            ALooper_wake(self.looper);
        }
        self.state.cancel_all();
    }
}

impl State {
    fn process_callbacks(&self) {
        let callbacks: Vec<SenderCallback> = {
            let mut callbacks = self.callbacks.lock().unwrap();
            callbacks.callbacks.drain(0..).collect()
        };
        for c in callbacks {
            c()
        }
    }

    fn get_pending_timers(&self) -> Vec<HandleType> {
        let now = Instant::now();
        self.timers
            .borrow()
            .iter()
            .filter(|v| v.1.scheduled <= now)
            .map(|v| *v.0)
            .collect()
    }

    fn process_pending_timers(&self, pending: Vec<HandleType>) {
        for handle in pending {
            let timer = self.timers.borrow_mut().remove(&handle);
            if let Some(timer) = timer {
                (timer.callback)();
            }
        }
        self.wake_up_at(self.next_timer());
    }

    fn process_timers(&self) {
        loop {
            let pending: Vec<HandleType> = self.get_pending_timers();
            if pending.is_empty() {
                break;
            }
            self.process_pending_timers(pending);
        }
    }

    fn wake_up_at(&self, time: Instant) {
        let wait_time = time.saturating_duration_since(Instant::now());
        let spec = itimerspec {
            it_interval: timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            it_value: timespec {
                tv_sec: wait_time.as_secs().try_into().unwrap(),
                tv_nsec: wait_time.subsec_nanos().try_into().unwrap(),
            },
        };
        unsafe {
            timerfd_settime(self.timer_fd, 0, &spec as *const _, std::ptr::null_mut());
        }
    }

    fn next_timer(&self) -> Instant {
        let min = self.timers.borrow().values().map(|x| x.scheduled).min();
        min.unwrap_or_else(|| Instant::now() + Duration::from_secs(60 * 60))
    }

    fn next_handle(&self) -> HandleType {
        let r = self.next_handle.get();
        self.next_handle.replace(r + 1);
        r
    }

    pub(crate) fn schedule<F>(&self, in_time: Duration, callback: F) -> HandleType
    where
        F: FnOnce() + 'static,
    {
        let handle = self.next_handle();

        self.timers.borrow_mut().insert(
            handle,
            Timer {
                scheduled: Instant::now() + in_time,
                callback: Box::new(callback),
            },
        );

        self.wake_up_at(self.next_timer());

        handle
    }

    pub(crate) fn unschedule(&self, handle: HandleType) {
        self.timers.borrow_mut().remove(&handle);
        self.wake_up_at(self.next_timer());
    }

    fn cancel_all(&self) {
        let timers = std::mem::take(&mut *self.timers.borrow_mut());
        let spec = itimerspec {
            it_interval: timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            it_value: timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
        };
        unsafe {
            timerfd_settime(self.timer_fd, 0, &spec as *const _, std::ptr::null_mut());
        }

        let callbacks = self
            .callbacks
            .lock()
            .map(|mut callbacks| {
                callbacks.is_shutdown = true;
                std::mem::take(&mut callbacks.callbacks)
            })
            .unwrap_or_default();
        drop((timers, callbacks));
    }
}

impl Drop for PlatformRunLoop {
    fn drop(&mut self) {
        self.shutdown();
        unsafe {
            ALooper_release(self.looper);
            Weak::from_raw(self.state_ptr);
            close(self.pipes[0]);
            close(self.pipes[1]);
            close(self.state.timer_fd);
        }
    }
}

#[derive(Clone)]
pub(crate) struct PlatformRunLoopSender {
    callbacks: std::sync::Weak<Mutex<Callbacks>>,
}

#[allow(unused_variables)]
impl PlatformRunLoopSender {
    pub(crate) fn send<F>(&self, callback: F) -> bool
    where
        F: FnOnce() + 'static + Send,
    {
        if let Some(callbacks) = self.callbacks.upgrade() {
            let mut callbacks = callbacks.lock().unwrap();
            if callbacks.is_shutdown {
                return false;
            }
            callbacks.callbacks.push(Box::new(callback));
            // Write to the self-pipe to wake the looper; the byte value is
            // irrelevant, the readable fd is the signal that drains the queue.
            let buf = [0u8; 8];
            unsafe {
                write(callbacks.fd, buf.as_ptr() as *const _, buf.len());
            }
            true
        } else {
            false
        }
    }
}

pub(crate) type PlatformThreadId = usize;

pub(crate) fn get_system_thread_id() -> PlatformThreadId {
    unsafe { libc::pthread_self() }
}
