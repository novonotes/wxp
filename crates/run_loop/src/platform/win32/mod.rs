mod adapter;
mod sys;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use self::adapter::WindowAdapter;
use self::sys::windows::*;

pub(crate) type HandleType = usize;
pub(crate) const INVALID_HANDLE: HandleType = 0;

pub(crate) struct PlatformRunLoop {
    state: Box<State>,
}

impl PlatformRunLoop {
    pub(crate) fn new() -> Self {
        let res = Self {
            state: Box::new(State::new()),
        };
        res.state.initialize();
        res
    }

    pub(crate) fn unschedule(&self, handle: HandleType) {
        self.state.unschedule(handle);
    }

    #[must_use]
    pub(crate) fn schedule<F>(&self, in_time: Duration, callback: F) -> HandleType
    where
        F: FnOnce() + 'static,
    {
        self.state.schedule(in_time, callback)
    }

    pub(crate) fn shutdown(&self) {
        self.state.shutdown();
    }

    pub(crate) fn run(&self) {
        self.state.run();
    }

    // Windows has no separate "application" object like AppKit; driving the
    // message loop is all there is, so `run_app`/`stop_app` just alias run/stop.
    pub(crate) fn run_app(&self) {
        self.run();
    }

    pub(crate) fn stop(&self) {
        self.state.stop();
    }

    pub(crate) fn stop_app(&self) {
        self.stop();
    }

    pub(crate) fn poll_once(&self, poll_session: &mut PollSession) {
        self.state.poll_once(poll_session);
    }

    pub(crate) fn new_sender(&self) -> PlatformRunLoopSender {
        self.state.new_sender()
    }
}

struct Timer {
    scheduled: Instant,
    callback: Box<dyn FnOnce()>,
}

type SenderCallback = Box<dyn FnOnce() + Send>;

// Private stop message. Posting one (instead of just setting a flag) guarantees
// the blocking `GetMessageW` wakes so the loop can observe the stop request.
const WM_RUNLOOP_STOP: u32 = WM_USER + 1;

struct State {
    next_handle: Cell<HandleType>,
    hwnd: Cell<HWND>,
    timers: RefCell<HashMap<HandleType, Timer>>,
    is_shutdown: Cell<bool>,

    // Callbacks sent from other threads
    sender_callbacks: Arc<Mutex<Vec<SenderCallback>>>,

    // Indicate that stop has been called
    stopping: Cell<bool>,
}

pub(crate) struct PollSession {
    /// Polling state for `RunLoop::block_on`.
    ///
    /// For the first few milliseconds, poll non-blocking aggressively.
    /// After that, block-wait on messages for this RunLoop's dedicated HWND.
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

impl State {
    fn new() -> Self {
        Self {
            next_handle: Cell::new(INVALID_HANDLE + 1),
            hwnd: Cell::new(0),
            timers: RefCell::new(HashMap::new()),
            is_shutdown: Cell::new(false),
            sender_callbacks: Arc::new(Mutex::new(Vec::new())),
            stopping: Cell::new(false),
        }
    }

    fn initialize(&self) {
        // The loop is driven by an invisible message-only window (parented to
        // HWND_MESSAGE inside `create_window`): it never displays, it just gives
        // us an HWND to post timer/sender/stop messages to.
        self.hwnd.set(self.create_window(
            "Irondash RunLoop Window",
            0, // WINDOW_STYLE
            0, // WINDOW_EX_STYLE
        ));
    }

    fn wake_up_at(&self, time: Instant) {
        if self.is_shutdown.get() {
            return;
        }

        let wait_time = time.saturating_duration_since(Instant::now());
        unsafe {
            SetTimer(self.hwnd.get(), 1, wait_time.as_millis() as u32, None);
        }
    }

    fn on_timer(&self) {
        let next_time = self.process_timers();
        self.wake_up_at(next_time);
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
        if self.is_shutdown.get() {
            return INVALID_HANDLE;
        }

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
        if self.is_shutdown.get() {
            return;
        }

        self.timers.borrow_mut().remove(&handle);
        self.wake_up_at(self.next_timer());
    }

    fn process_timers(&self) -> Instant {
        loop {
            let now = Instant::now();
            let pending: Vec<HandleType> = self
                .timers
                .borrow()
                .iter()
                .filter(|v| v.1.scheduled <= now)
                .map(|v| *v.0)
                .collect();
            if pending.is_empty() {
                break;
            }
            for handle in pending {
                let timer = self.timers.borrow_mut().remove(&handle);
                if let Some(timer) = timer {
                    (timer.callback)();
                }
            }
        }

        self.next_timer()
    }

    fn process_callbacks(&self) {
        let callbacks: Vec<SenderCallback> = {
            let mut callbacks = self.sender_callbacks.lock().unwrap();
            callbacks.drain(0..).collect()
        };
        for c in callbacks {
            c()
        }
    }

    fn new_sender(&self) -> PlatformRunLoopSender {
        PlatformRunLoopSender {
            hwnd: self.hwnd.get(),
            callbacks: Arc::downgrade(&self.sender_callbacks),
        }
    }

    fn run(&self) {
        self.stopping.set(false);
        unsafe {
            let mut message = MSG {
                hwnd: 0,
                message: 0,
                wParam: 0,
                lParam: 0,
                time: 0,
                pt: POINT { x: 0, y: 0 },
            };
            while !self.stopping.get() && GetMessageW(&mut message as *mut _, 0, 0, 0) != 0 {
                TranslateMessage(&message as *const _);
                DispatchMessageW(&message as *const _);
            }
        }
    }

    fn poll_once(&self, poll_session: &mut PollSession) {
        unsafe {
            // Without MWMO_INPUTAVAILABLE the wait can
            // be racy as it will ignore messages posted between
            // PeekMessageW and MsgWaitForMultipleObjectsEx.
            MsgWaitForMultipleObjectsEx(
                0,
                std::ptr::null_mut(),
                7,
                QS_POSTMESSAGE | QS_TIMER,
                MWMO_INPUTAVAILABLE,
            );
            let mut message = MSG::default();
            loop {
                let res = PeekMessageW(
                    &mut message as *mut _,
                    self.hwnd.get(),
                    0,
                    0,
                    PM_REMOVE | PM_NOYIELD,
                ) != 0;

                if res {
                    TranslateMessage(&message as *const _);
                    DispatchMessageW(&message as *const _);
                } else {
                    if !poll_session.timed_out {
                        poll_session.timed_out =
                            poll_session.start.elapsed() >= Duration::from_millis(6);
                    }
                    break;
                }
            }
        }
    }

    fn stop(&self) {
        unsafe { PostMessageW(self.hwnd.get(), WM_RUNLOOP_STOP, 0, 0) };
    }

    fn shutdown(&self) {
        if self.is_shutdown.replace(true) {
            return;
        }

        self.stopping.set(true);
        let timers = std::mem::take(&mut *self.timers.borrow_mut());
        let callbacks = self
            .sender_callbacks
            .lock()
            .map(|mut callbacks| std::mem::take(&mut *callbacks))
            .unwrap_or_default();
        unsafe {
            KillTimer(self.hwnd.get(), 1);
            DestroyWindow(self.hwnd.get());
        }
        self.hwnd.set(0);
        drop((timers, callbacks));
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl WindowAdapter for State {
    fn wnd_proc(&self, hwnd: HWND, msg: u32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
        match msg {
            WM_TIMER => {
                self.on_timer();
            }
            WM_USER => {
                self.process_callbacks();
            }
            WM_RUNLOOP_STOP => {
                self.stopping.set(true);
            }
            _ => {}
        }
        unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) }
    }
}

#[derive(Clone)]
pub(crate) struct PlatformRunLoopSender {
    hwnd: HWND,
    callbacks: std::sync::Weak<Mutex<Vec<SenderCallback>>>,
}

#[allow(unused_variables)]
impl PlatformRunLoopSender {
    pub(crate) fn send<F>(&self, callback: F) -> bool
    where
        F: FnOnce() + 'static + Send,
    {
        if let Some(callbacks) = self.callbacks.upgrade() {
            {
                let mut callbacks = callbacks.lock().unwrap();
                callbacks.push(Box::new(callback));
            }
            unsafe {
                PostMessageW(self.hwnd, WM_USER, 0, 0);
            }
            true
        } else {
            false
        }
    }
}

pub(crate) type PlatformThreadId = u32;

pub(crate) fn get_system_thread_id() -> PlatformThreadId {
    unsafe { GetCurrentThreadId() }
}
