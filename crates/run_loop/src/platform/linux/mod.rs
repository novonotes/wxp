mod sys;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    os::raw::c_uint,
    rc::Rc,
    sync::{
        Arc, Mutex, Once,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use log::error;
use sys::glib::*;

use self::sys::libc;

type SourceId = c_uint;

pub(crate) type HandleType = usize;
pub(crate) const INVALID_HANDLE: HandleType = 0;

pub(crate) struct PollSession {
    /// Polling state for `RunLoop::block_on`.
    ///
    /// For the first few milliseconds, poll non-blocking aggressively.
    /// After that, switch to blocking wait on the same context.
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

pub(crate) struct PlatformRunLoop {
    context: ContextHolder,
    main_loop: *mut GMainLoop,
    next_handle: Cell<HandleType>,
    timers: Rc<RefCell<HashMap<HandleType, SourceId>>>,
    sender_state: Arc<Mutex<SenderState>>,
    is_shutdown: Cell<bool>,
}

struct SenderState {
    source_ids: Vec<SourceId>,
    is_shutdown: bool,
}

// Attach a Rust closure to a GLib timeout source. GLib only takes a C function
// + opaque pointer, so the closure is boxed, passed as that pointer, called via
// `trampoline`, and freed via `destroy_closure` when GLib drops the source —
// the standard pattern; keep the box/trampoline/destroy trio balanced.
fn context_add_source<F>(context: *mut GMainContext, interval: Duration, func: F) -> SourceId
where
    F: FnMut() -> gboolean + 'static,
{
    unsafe extern "C" fn trampoline<F: FnMut() -> gboolean + 'static>(func: gpointer) -> gboolean {
        let func: &RefCell<F> = unsafe { &*(func as *const RefCell<F>) };
        (*func.borrow_mut())()
    }

    fn into_raw<F: FnMut() -> gboolean + 'static>(func: F) -> gpointer {
        let func: Box<RefCell<F>> = Box::new(RefCell::new(func));
        Box::into_raw(func) as gpointer
    }

    unsafe extern "C" fn destroy_closure<F: FnMut() -> gboolean + 'static>(ptr: gpointer) {
        let _ = unsafe { Box::<RefCell<F>>::from_raw(ptr as *mut _) };
    }

    unsafe {
        let source = g_timeout_source_new(interval.as_millis() as _);
        g_source_set_callback(
            source,
            Some(trampoline::<F>),
            into_raw(func),
            Some(destroy_closure::<F>),
        );
        let id = g_source_attach(source, context);

        g_source_unref(source);
        id
    }
}

fn context_remove_source(context: *mut GMainContext, source_id: SourceId) {
    unsafe {
        let source = g_main_context_find_source_by_id(context, source_id);
        if !source.is_null() {
            g_source_destroy(source);
        }
    }
}

// We have no real "main thread" signal inside a plugin, so we approximate it as
// "the first thread that touched this library". The CAS only succeeds once, so
// `FIRST_THREAD` latches the earliest observer and never moves afterwards.
static FIRST_THREAD: AtomicUsize = AtomicUsize::new(0);
static GTK_INIT: Once = Once::new();

fn remember_first_thread() {
    let thread_id = get_system_thread_id();
    let _ = FIRST_THREAD.compare_exchange(0, thread_id, Ordering::SeqCst, Ordering::SeqCst);
}

fn is_main_thread() -> bool {
    remember_first_thread();
    FIRST_THREAD.load(Ordering::SeqCst) == get_system_thread_id()
}

// Run `remember_first_thread` from an ELF constructor (`.init_array`) so the
// latch happens at library-load time — typically on the host's UI/main thread,
// before any of our other code (or a worker thread) can claim it first.
#[used]
#[cfg_attr(
    any(target_os = "linux", target_os = "android"),
    unsafe(link_section = ".init_array")
)]
static ON_LOAD: extern "C" fn() = {
    #[cfg_attr(
        any(target_os = "linux", target_os = "android"),
        unsafe(link_section = ".text.startup")
    )]
    extern "C" fn on_load() {
        remember_first_thread();
    }
    on_load
};

#[allow(unused_variables)]
impl PlatformRunLoop {
    pub(crate) fn new() -> Self {
        // Only the (approximate) main thread may init GTK; doing it from a
        // worker would either be rejected by GTK or fight the host that already
        // owns the UI. `Once` keeps it to a single attempt process-wide.
        if is_main_thread() {
            GTK_INIT.call_once(|| {
                // Match tao's gtk-rs initialization so embedding alongside a
                // tao/winit-based host stays consistent.
                if let Err(e) = gtk::init() {
                    let message =
                        format!("Failed to initialize GTK on Linux run loop thread: {}", e);
                    error!("{message}");
                    panic!("{message}");
                }
            });
        }

        // Reuse the host's existing GMainContext whenever possible so our
        // callbacks/timers run on the same loop GTK already drives. Preference
        // order: the default context if this thread owns it, else any
        // thread-default the host installed, else the default context on the
        // main thread, and only as a last resort a fresh private context.
        let context = unsafe {
            let default_context = g_main_context_default();
            if g_main_context_is_owner(default_context) == GTRUE {
                ContextHolder::retain(default_context)
            } else {
                let thread_context = g_main_context_get_thread_default();
                if !thread_context.is_null() {
                    ContextHolder::retain(thread_context)
                } else if is_main_thread() {
                    ContextHolder::retain(default_context)
                } else {
                    ContextHolder::adopt(g_main_context_new())
                }
            }
        };
        unsafe { g_main_context_push_thread_default(context.0) };
        let main_loop = unsafe { g_main_loop_new(context.0, GFALSE) };
        Self {
            context,
            next_handle: Cell::new(INVALID_HANDLE + 1),
            timers: Rc::new(RefCell::new(HashMap::new())),
            sender_state: Arc::new(Mutex::new(SenderState {
                source_ids: Vec::new(),
                is_shutdown: false,
            })),
            is_shutdown: Cell::new(false),
            main_loop,
        }
    }

    pub(crate) fn unschedule(&self, handle: HandleType) {
        if self.is_shutdown.get() {
            return;
        }

        let source = self.timers.borrow_mut().remove(&handle);
        if let Some(source) = source {
            context_remove_source(self.context.0, source);
        }
    }

    fn next_handle(&self) -> HandleType {
        let r = self.next_handle.get();
        self.next_handle.replace(r + 1);
        r
    }

    #[must_use]
    pub(crate) fn schedule<F>(&self, in_time: Duration, callback: F) -> HandleType
    where
        F: FnOnce() + 'static,
    {
        if self.is_shutdown.get() {
            return INVALID_HANDLE;
        }

        let callback = Rc::new(RefCell::new(Some(callback)));
        let handle = self.next_handle();

        let timers = self.timers.clone();

        let source_id = context_add_source(self.context.0, in_time, move || {
            timers.borrow_mut().remove(&handle);
            let f = callback
                .borrow_mut()
                .take()
                .expect("Timer callback was called multiple times");
            f();
            G_SOURCE_REMOVE
        });
        self.timers.borrow_mut().insert(handle, source_id);
        handle
    }

    pub(crate) fn run(&self) {
        unsafe { g_main_loop_run(self.main_loop) };
    }

    pub(crate) fn stop(&self) {
        unsafe { g_main_loop_quit(self.main_loop) };
    }

    pub(crate) fn shutdown(&self) {
        if self.is_shutdown.replace(true) {
            return;
        }

        let timer_source_ids: Vec<SourceId> = self
            .timers
            .borrow_mut()
            .drain()
            .map(|(_, source_id)| source_id)
            .collect();
        for source_id in timer_source_ids {
            context_remove_source(self.context.0, source_id);
        }

        let source_ids = {
            let mut sender_state = self.sender_state.lock().unwrap();
            sender_state.is_shutdown = true;
            std::mem::take(&mut sender_state.source_ids)
        };
        for source_id in source_ids {
            context_remove_source(self.context.0, source_id);
        }

        unsafe { g_main_loop_quit(self.main_loop) };
    }

    pub(crate) fn run_app(&self) {
        unsafe { gtk_main() };
    }

    pub(crate) fn stop_app(&self) {
        unsafe { gtk_main_quit() };
    }

    pub(crate) fn poll_once(&self, poll_session: &mut PollSession) {
        if !poll_session.timed_out {
            // For the first 6ms, poll non-blocking aggressively
            unsafe { g_main_context_iteration(self.context.0, GFALSE) };
            poll_session.timed_out = poll_session.start.elapsed() >= Duration::from_millis(6);
        } else {
            // After that, switch to blocking wait on the same context
            unsafe { g_main_context_iteration(self.context.0, GTRUE) };
        }
    }

    pub(crate) fn is_main_thread() -> bool {
        unsafe { g_main_context_is_owner(g_main_context_default()) == GTRUE }
    }

    pub(crate) fn new_sender(self: &Rc<Self>) -> PlatformRunLoopSender {
        PlatformRunLoopSender::new(self.context.clone(), Arc::downgrade(&self.sender_state))
    }
}

impl Drop for PlatformRunLoop {
    fn drop(&mut self) {
        self.shutdown();
        unsafe {
            g_main_context_pop_thread_default(self.context.0);
            g_main_loop_unref(self.main_loop);
        }
    }
}

/// RAII reference-count holder for a `GMainContext`.
///
/// Every holder owns exactly one ref; `Clone`/`Drop` mirror `g_main_context_ref`
/// / `_unref`. This is what lets a `PlatformRunLoopSender` outlive (or be sent
/// across threads from) the `PlatformRunLoop` without the context being freed.
struct ContextHolder(*mut GMainContext);

// SAFETY: GMainContext is documented as safe to ref/unref and to invoke into
// from any thread; this holder only ever does those operations.
unsafe impl Send for ContextHolder {}
unsafe impl Sync for ContextHolder {}

impl ContextHolder {
    /// Takes a new ref on a context owned by someone else (host/GTK).
    unsafe fn retain(context: *mut GMainContext) -> Self {
        Self(unsafe { g_main_context_ref(context) })
    }
    /// Takes ownership of a context we just created (no extra ref needed).
    unsafe fn adopt(context: *mut GMainContext) -> Self {
        Self(context)
    }
}

impl Clone for ContextHolder {
    fn clone(&self) -> Self {
        Self(unsafe { g_main_context_ref(self.0) })
    }
}

impl Drop for ContextHolder {
    fn drop(&mut self) {
        unsafe { g_main_context_unref(self.0) };
    }
}

#[derive(Clone)]
pub(crate) struct PlatformRunLoopSender {
    context: ContextHolder,
    state: std::sync::Weak<Mutex<SenderState>>,
}

impl PlatformRunLoopSender {
    fn new(context: ContextHolder, state: std::sync::Weak<Mutex<SenderState>>) -> Self {
        Self { context, state }
    }

    pub(crate) fn send<F>(&self, callback: F) -> bool
    where
        F: FnOnce() + 'static + Send,
    {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        {
            let state = state.lock().unwrap();
            if state.is_shutdown {
                return false;
            }
        }

        // Track the GLib source itself instead of only checking a shutdown flag.
        // On plugin unload the source must be destroyed synchronously so GLib
        // drops the boxed Rust callback before the DSO can disappear.
        let state_for_callback = Arc::downgrade(&state);
        let source_id_for_callback = Arc::new(AtomicUsize::new(INVALID_HANDLE));
        let source_id_after_attach = source_id_for_callback.clone();
        let callback_finished = Arc::new(AtomicBool::new(false));
        let callback_finished_after_attach = callback_finished.clone();
        let mut callback = Some(callback);
        let source_id = context_add_source(self.context.0, Duration::ZERO, move || {
            callback_finished.store(true, Ordering::Release);

            let Some(state) = state_for_callback.upgrade() else {
                return G_SOURCE_REMOVE;
            };
            let source_id = source_id_for_callback.load(Ordering::Acquire) as SourceId;
            {
                let mut state = state.lock().unwrap();
                state.source_ids.retain(|id| *id != source_id);
                if state.is_shutdown {
                    return G_SOURCE_REMOVE;
                }
            }

            let callback = callback
                .take()
                .expect("Sender callback was called multiple times");
            callback();
            G_SOURCE_REMOVE
        });
        source_id_after_attach.store(source_id as usize, Ordering::Release);

        let mut state = state.lock().unwrap();
        if state.is_shutdown {
            context_remove_source(self.context.0, source_id);
            false
        } else {
            if !callback_finished_after_attach.load(Ordering::Acquire) {
                state.source_ids.push(source_id);
            }
            true
        }
    }
}

pub(crate) type PlatformThreadId = usize;

pub(crate) fn get_system_thread_id() -> PlatformThreadId {
    unsafe { libc::pthread_self() }
}
