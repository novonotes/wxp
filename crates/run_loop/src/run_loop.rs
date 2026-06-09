use log::warn;
use std::{
    fmt::Display,
    marker::PhantomData,
    mem::ManuallyDrop,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    pin::pin,
    rc::Rc,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    thread::{self, ThreadId},
    time::Duration,
};

use futures::{Future, future::poll_fn, task::ArcWake};

use crate::{
    Handle, JoinHandle, RunLoopSender, Task,
    platform::{PlatformRunLoop, PollSession},
    task::AbortableTask,
    util::{BlockingVariable, FutureCompleter},
};

// Lets a `!Send`/`!Sync` value (e.g. the platform run loop, which is `Rc`-based)
// live in a `static`. Rust normally forbids this; the type is sound here *only*
// because the contained value is read via run-loop-thread-only entry points
// (`RunLoopGuard::local`, post/call callbacks, and internal local helpers), and
// is installed/cleared only by `initialize`/`shutdown`, which are serialized
// under `INIT_MUTEX`. So the interior is never touched off the run loop thread
// or concurrently.
struct RunLoopThreadOnly<T> {
    inner: std::cell::UnsafeCell<Option<T>>,
}

// SAFETY: these impls are a deliberate lie to the type system. They are sound
// only under the invariant above (run-loop-thread reads; init/shutdown under
// `INIT_MUTEX`). Do not add callers that bypass both.
unsafe impl<T> Send for RunLoopThreadOnly<T> {}
unsafe impl<T> Sync for RunLoopThreadOnly<T> {}

impl<T> RunLoopThreadOnly<T> {
    const fn new() -> Self {
        Self {
            inner: std::cell::UnsafeCell::new(None),
        }
    }

    fn set(&self, value: T) -> std::result::Result<(), T> {
        unsafe {
            let inner = &mut *self.inner.get();
            if inner.is_some() {
                Err(value)
            } else {
                *inner = Some(value);
                Ok(())
            }
        }
    }

    fn get(&self) -> Option<&T> {
        unsafe {
            let inner = &*self.inner.get();
            inner.as_ref()
        }
    }

    fn clear(&self) {
        unsafe {
            let inner = &mut *self.inner.get();
            *inner = None;
        }
    }
}

// There is at most one run loop per process. `RUN_LOOP_THREAD_ID` is the
// cross-thread source of truth for "which thread owns it" and gates access to
// the thread-only instance above.
static RUN_LOOP_INSTANCE: RunLoopThreadOnly<Arc<RunLoopInner>> = RunLoopThreadOnly::new();
static RUN_LOOP_SENDER: Mutex<Option<RunLoopSender>> = Mutex::new(None);
static RUN_LOOP_THREAD_ID: Mutex<Option<ThreadId>> = Mutex::new(None);

// init/deinit are reference-counted (CLAP/VST3 style): a host may load the same
// plugin DLL into several instances that each call init/deinit, but the loop
// must be created once and torn down only when the last instance leaves.
// `INIT_MUTEX` serializes those transitions; `BLOCK_ON_ACTIVE` detects the
// unsupported re-entrant `block_on`.
static INIT_COUNT: AtomicUsize = AtomicUsize::new(0);
static INIT_MUTEX: Mutex<()> = Mutex::new(());
static BLOCK_ON_ACTIVE: AtomicBool = AtomicBool::new(false);
static PENDING_CALLS: Mutex<Vec<Weak<dyn PendingCall>>> = Mutex::new(Vec::new());

struct BlockOnActiveGuard;

trait PendingCall: Send + Sync {
    fn cancel(&self);
}

struct CallCompletion<R: Send + 'static> {
    var: BlockingVariable<Result<R>>,
    completed: AtomicBool,
}

impl<R: Send + 'static> CallCompletion<R> {
    fn new(var: BlockingVariable<Result<R>>) -> Self {
        Self {
            var,
            completed: AtomicBool::new(false),
        }
    }

    fn complete(&self, result: Result<R>) {
        if !self.completed.swap(true, Ordering::AcqRel) {
            self.var.set(result);
        }
    }
}

impl<R: Send + 'static> PendingCall for CallCompletion<R> {
    fn cancel(&self) {
        if !self.completed.swap(true, Ordering::AcqRel) {
            self.var.set(Err(Error::NotInitialized));
        }
    }
}

impl<R: Send + 'static> Drop for CallCompletion<R> {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl BlockOnActiveGuard {
    fn enter() -> Self {
        let was_active = BLOCK_ON_ACTIVE.swap(true, Ordering::AcqRel);
        assert!(
            !was_active,
            "Nested RunLoop::block_on is undefined behavior."
        );
        Self
    }
}

impl Drop for BlockOnActiveGuard {
    fn drop(&mut self) {
        BLOCK_ON_ACTIVE.store(false, Ordering::Release);
    }
}

struct BlockOnWaker {
    sender: RunLoopSender,
    queued: AtomicBool,
}

impl BlockOnWaker {
    fn new(sender: RunLoopSender) -> Self {
        Self {
            sender,
            queued: AtomicBool::new(true),
        }
    }

    fn take_queued(&self) -> bool {
        self.queued.swap(false, Ordering::AcqRel)
    }
}

impl Wake for BlockOnWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        // `block_on` yields control back to the platform poll loop while waiting,
        // so when the future is woken we need to trigger another run loop iteration.
        // We re-enqueue a single empty callback to avoid excessive wake spam.
        if !self.queued.swap(true, Ordering::AcqRel) {
            let _ = self.sender.send(|| {});
        }
    }
}

struct RunLoopInner {
    platform_run_loop: Rc<PlatformRunLoop>,
    active_tasks: Mutex<Vec<std::sync::Weak<dyn AbortableTask>>>,
    has_shutdown: AtomicBool,
    shutdown_token: Arc<AtomicBool>,
}

impl Drop for RunLoopInner {
    fn drop(&mut self) {
        // Normally has_shutdown is set by shutdown(), but in abnormal exit scenarios
        // (e.g. DLL unloaded without calling deinit()) we set it here so other code
        // can stop cleanly.
        self.has_shutdown.store(true, Ordering::SeqCst);

        // Clean up active tasks
        if let Ok(tasks) = self.active_tasks.lock() {
            // Tasks are already held as Weak, so no forced abort is required.
            // Log a warning if any are still alive.
            let active_count = tasks.iter().filter(|t| t.upgrade().is_some()).count();
            if active_count > 0 {
                warn!(
                    "Warning: RunLoop dropped with {} active tasks",
                    active_count
                );
            }
        }

        // Platform-specific cleanup is handled automatically by each platform's Drop impl.
    }
}

/// Process-wide facade for the native run loop.
///
/// `RunLoop` owns no per-instance state. After [`init`](Self::init) succeeds,
/// callbacks can be posted to the run loop thread from any thread using
/// [`post`](Self::post) or [`call`](Self::call).
///
/// Operations that may capture `!Send` GUI/native state are isolated behind
/// [`RunLoopLocal`]. It is borrowed from [`RunLoopGuard`] on the thread that
/// initialized the run loop, or passed to callbacks that run on the run loop
/// thread.
///
/// This crate uses a static singleton instead of thread-local storage so DLL
/// unload does not depend on host-controlled TLS destructor ordering.
pub struct RunLoop {
    inner: Arc<RunLoopInner>,
}

/// RAII ownership of one run loop initialization reference.
///
/// Dropping the guard releases the reference acquired by [`RunLoop::init`].
///
/// The guard may be released from a different host thread during plugin object
/// teardown. [`RunLoopLocal`] remains run-loop-thread-only; only the lifetime
/// reference itself is sendable.
pub struct RunLoopGuard {
    local: ManuallyDrop<RunLoopLocal>,
}

impl Drop for RunLoopGuard {
    fn drop(&mut self) {
        if RunLoop::is_run_loop_thread() {
            RunLoop::release_guard_on_run_loop_thread();
            unsafe { ManuallyDrop::drop(&mut self.local) };
        } else {
            // Audio plugin hosts may destroy wrapper objects on a different
            // thread from the one that created them. In that case, move the
            // run-loop-thread-local capability into a shutdown/drop barrier so
            // !Send tasks, callbacks, and platform state are released at an
            // event-loop boundary. This follows the same safety shape as JUCE's
            // MessageManagerLock: the message thread reaches a known safe point
            // instead of another thread touching UI/run-loop state directly.
            let local = unsafe { ManuallyDrop::take(&mut self.local) };
            RunLoop::release_guard_from_other_thread(RunLoopLocalForRunLoopDrop::new(local));
        }
    }
}

impl RunLoopGuard {
    /// Returns the run-loop-thread-local capability for this guard.
    ///
    /// The returned [`RunLoopLocal`] can run operations that are only valid on the
    /// run loop thread, including callbacks and futures that capture `!Send`
    /// state.
    pub fn local(&self) -> &RunLoopLocal {
        assert!(
            RunLoop::is_run_loop_thread(),
            "RunLoopGuard::local() is only valid on the run loop thread"
        );
        &self.local
    }
}

unsafe impl Send for RunLoopGuard {}

struct RunLoopLocalForRunLoopDrop {
    local: ManuallyDrop<RunLoopLocal>,
    dropped: bool,
}

impl RunLoopLocalForRunLoopDrop {
    fn new(local: RunLoopLocal) -> Self {
        Self {
            local: ManuallyDrop::new(local),
            dropped: false,
        }
    }

    fn drop_on_run_loop_thread(mut self) {
        assert!(
            RunLoop::is_run_loop_thread(),
            "RunLoopLocal must be dropped on the run loop thread"
        );
        unsafe { ManuallyDrop::drop(&mut self.local) };
        self.dropped = true;
    }
}

impl Drop for RunLoopLocalForRunLoopDrop {
    fn drop(&mut self) {
        if self.dropped {
            return;
        }

        if RunLoop::is_run_loop_thread() {
            unsafe { ManuallyDrop::drop(&mut self.local) };
            self.dropped = true;
        } else {
            // If the host has already stopped pumping the run loop, the posted
            // barrier may be rejected or abandoned. Dropping this local on the
            // host's destructor thread could release native run-loop state on
            // the wrong thread, so leaking is the safer failure mode for audio
            // plugins.
            warn!("leaking RunLoopLocal because it could not be dropped on the run loop thread");
        }
    }
}

// SAFETY: this wrapper is Send only to move ownership from an arbitrary host
// destructor thread back to the recorded run-loop thread. `RunLoopLocal` remains
// unusable off-thread; the wrapper either drops it on the run-loop thread or
// deliberately leaks it.
unsafe impl Send for RunLoopLocalForRunLoopDrop {}

/// Capability for operations that must be created and driven on the run loop thread.
///
/// `RunLoopLocal` is intentionally `!Send + !Sync`. It is borrowed from
/// [`RunLoopGuard`] or passed to callbacks executed by [`RunLoop::post`] and
/// [`RunLoop::call`].
pub struct RunLoopLocal {
    run_loop: RunLoop,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl RunLoopLocal {
    fn new(run_loop: RunLoop) -> Self {
        Self {
            run_loop,
            _not_send_sync: PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Error {
    /// The engine-context plugin is not loaded.
    /// Accessing the main-thread sender requires the irondash_engine_context Flutter plugin.
    #[cfg(feature = "flutter")]
    EngineContextPluginError(irondash_engine_context::Error),

    /// RunLoop is already initialized.
    AlreadyInitialized,

    /// RunLoop is not initialized. Call RunLoop::init() first.
    NotInitialized,

    /// Called from a thread that is not the run loop thread.
    NotRunLoopThread,

    #[cfg(test)]
    RunLoopThreadNotSet,
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(feature = "flutter")]
impl From<irondash_engine_context::Error> for Error {
    fn from(err: irondash_engine_context::Error) -> Self {
        Error::EngineContextPluginError(err)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "flutter")]
            Error::EngineContextPluginError(e) => e.fmt(f),
            Error::AlreadyInitialized => write!(f, "RunLoop was already initialized"),
            Error::NotInitialized => {
                write!(f, "RunLoop is not initialized. Call RunLoop::init() first")
            }
            Error::NotRunLoopThread => {
                write!(
                    f,
                    "RunLoop::init() must be called from the run loop thread. \
                          If this is a test, use serial_test::serial to run the test in serial."
                )
            }
            #[cfg(test)]
            Error::RunLoopThreadNotSet => write!(
                f,
                "main thread was not set. call RunLoop::set_main_thread() from main thread"
            ),
        }
    }
}

impl std::error::Error for Error {}

impl RunLoop {
    /// Initializes the process-wide run loop on the current thread.
    ///
    /// This is not the same lifecycle hook as CLAP `clap_entry.init`. In audio
    /// plugins, call it from the host main/UI thread that will receive GUI
    /// callbacks, not from a scanning thread, audio thread, or DSO
    /// initialization hook.
    ///
    /// Each successful call returns a [`RunLoopGuard`]. The run loop is
    /// reference-counted and shuts down when the last guard is dropped.
    ///
    /// Returns an error if the run loop is already initialized on another thread.
    pub fn init() -> Result<RunLoopGuard> {
        let _guard = INIT_MUTEX.lock().unwrap();

        let count = INIT_COUNT.load(Ordering::SeqCst);
        if count == 0 {
            Self::initialize()?;
            INIT_COUNT.store(1, Ordering::SeqCst);
            return Ok(RunLoopGuard {
                local: ManuallyDrop::new(RunLoopLocal::new(Self::current_local()?)),
            });
        }

        if !Self::is_run_loop_thread() {
            return Err(Error::NotRunLoopThread);
        }

        INIT_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(RunLoopGuard {
            local: ManuallyDrop::new(RunLoopLocal::new(Self::current_local()?)),
        })
    }

    /// Returns whether the process-wide run loop is currently initialized.
    ///
    /// This may be called from any thread. The result is only a snapshot:
    /// another thread may drop the final [`RunLoopGuard`] immediately after this
    /// returns, so callers must still handle errors from [`post`](Self::post)
    /// and [`call`](Self::call).
    pub fn is_initialized() -> bool {
        INIT_COUNT.load(Ordering::SeqCst) > 0
    }

    #[cfg(test)]
    /// Forcibly rebinds the run loop to the current thread.
    ///
    /// Mainly a test-suite escape hatch: tests are serialized but a previous
    /// test may have left the loop bound to a now-dead thread. Rather than
    /// failing, tear the old loop down and rebuild it here, preserving the
    /// existing init count so reference counting stays balanced.
    pub fn ensure_run_loop_on_current_thread() -> Result<RunLoopGuard> {
        let guard = INIT_MUTEX.lock().unwrap();
        let count = INIT_COUNT.load(Ordering::SeqCst);

        if count == 0 {
            // Nothing initialized yet — the normal path is sufficient.
            drop(guard);
            return Self::init();
        }

        if Self::is_run_loop_thread() {
            // Already where we want to be; nothing to rebuild.
            INIT_COUNT.fetch_add(1, Ordering::SeqCst);
            return Ok(RunLoopGuard {
                local: ManuallyDrop::new(RunLoopLocal::new(Self::current_local()?)),
            });
        }

        // Bound to a different (likely dead) thread: rebuild in place and
        // restore the original count so existing guards stay balanced.
        INIT_COUNT.store(0, Ordering::SeqCst);
        Self::shutdown();

        Self::initialize()?;
        INIT_COUNT.store(count, Ordering::SeqCst);
        debug_assert!(Self::is_run_loop_thread());
        INIT_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(RunLoopGuard {
            local: ManuallyDrop::new(RunLoopLocal::new(Self::current_local()?)),
        })
    }

    fn release_guard_on_run_loop_thread() {
        let _guard = INIT_MUTEX.lock().unwrap();

        let count = INIT_COUNT.fetch_sub(1, Ordering::SeqCst);
        if count == 1 {
            Self::shutdown();
        }
    }

    fn release_guard_from_other_thread(local: RunLoopLocalForRunLoopDrop) {
        let _guard = INIT_MUTEX.lock().unwrap();

        let count = INIT_COUNT.fetch_sub(1, Ordering::SeqCst);
        if count == 1 {
            Self::shutdown_from_other_thread(local);
        } else {
            Self::drop_local_from_other_thread(local);
        }
    }

    /// Internal only: performs the actual initialization.
    fn initialize() -> Result<()> {
        // Create the RunLoop instance.
        #[allow(clippy::arc_with_non_send_sync)]
        let inner = Arc::new(RunLoopInner {
            platform_run_loop: Rc::new(PlatformRunLoop::new()),
            active_tasks: Mutex::new(Vec::new()),
            has_shutdown: AtomicBool::new(false),
            shutdown_token: Arc::new(AtomicBool::new(false)),
        });

        let sender = RunLoopSender::new(
            inner.platform_run_loop.new_sender(),
            Arc::downgrade(&inner.shutdown_token),
        );

        RUN_LOOP_INSTANCE
            .set(inner.clone())
            .map_err(|_| Error::AlreadyInitialized)?;

        {
            let mut run_loop_sender = RUN_LOOP_SENDER.lock().unwrap();
            *run_loop_sender = Some(sender);
        }

        // Only publish the owning thread after the instance is installed, so a
        // failed initialize does not leave a partially observable run loop.
        {
            let mut thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();
            *thread_id = Some(thread::current().id());
        }

        Ok(())
    }

    /// Internal only: performs the actual cleanup.
    fn shutdown() {
        if let Some(instance) = RUN_LOOP_INSTANCE.get() {
            // Record that shutdown is complete
            instance.has_shutdown.store(true, Ordering::SeqCst);
            instance.shutdown_token.store(true, Ordering::SeqCst);

            // Wake cross-thread RunLoop::call waiters whose callbacks may still be
            // queued in a platform loop that is about to stop being pumped.
            let pending_calls = std::mem::take(&mut *PENDING_CALLS.lock().unwrap());
            for pending_call in pending_calls {
                if let Some(pending_call) = pending_call.upgrade() {
                    pending_call.cancel();
                }
            }

            // Abort all active tasks.
            // Catch any panics during abort to prevent crashes.
            // In audio plugins, not crashing the DAW host is the top priority.
            // This is a safety net; ideally no panic should occur here.
            if let Ok(tasks) = instance.active_tasks.lock() {
                for weak_task in tasks.iter() {
                    if let Some(task) = weak_task.upgrade() {
                        if let Err(e) =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                task.abort();
                            }))
                        {
                            log::error!(
                                "panic during task abort in shutdown (ignored to prevent crash): {:?}",
                                e
                            );
                        }
                    }
                }
            }

            // Clear the active task list
            if let Ok(mut tasks) = instance.active_tasks.lock() {
                tasks.clear();
            }

            // Platform-specific cleanup is handled automatically by each platform's Drop impl.
        }

        // Clear the run loop thread ID so a new thread can be set by the next init()
        {
            let mut thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();
            *thread_id = None;
        }

        // Clear the RunLoop instance so a new one can be created by the next init()
        RUN_LOOP_INSTANCE.clear();

        // Drop the cross-thread sender after the run loop instance is no longer
        // observable.
        {
            let mut run_loop_sender = RUN_LOOP_SENDER.lock().unwrap();
            *run_loop_sender = None;
        }
    }

    fn shutdown_from_other_thread(local: RunLoopLocalForRunLoopDrop) {
        let Some(instance) = RUN_LOOP_INSTANCE.get() else {
            return;
        };
        let Some(sender) = RUN_LOOP_SENDER.lock().unwrap().take() else {
            Self::cancel_pending_calls();
            return;
        };

        // Stop every normal cloned sender before posting the shutdown barrier.
        // Otherwise a host could enqueue plugin-DLL callbacks after the barrier,
        // then unload the DLL as soon as this release returns.
        instance.has_shutdown.store(true, Ordering::SeqCst);
        instance.shutdown_token.store(true, Ordering::SeqCst);
        Self::cancel_pending_calls();

        let completed = Arc::new((Mutex::new(false), Condvar::new()));
        let completed_for_barrier = completed.clone();
        let sent = sender.send_shutdown_barrier(move || {
            // Drop the guard's local capability before shutdown clears the
            // recorded run-loop thread. The process-wide instance remains alive
            // until `shutdown` clears it below, so platform cleanup still runs
            // on this same event-loop turn.
            local.drop_on_run_loop_thread();
            Self::shutdown();
            let (lock, condvar) = &*completed_for_barrier;
            *lock.lock().unwrap() = true;
            condvar.notify_one();
        });

        if !sent {
            warn!(
                "failed to post RunLoop shutdown barrier; leaking platform state to avoid cross-thread cleanup"
            );
            return;
        }

        let (lock, condvar) = &*completed;
        let mut done = lock.lock().unwrap();
        while !*done {
            // This intentionally has the same blocking trade-off as JUCE's
            // MessageManagerLock: if the host blocks the run-loop thread, this
            // destructor can also block. Returning before the barrier runs is
            // more dangerous for plugins because the host may unload the DLL
            // while queued callbacks or tasks still reference plugin code.
            done = condvar.wait(done).unwrap();
        }
    }

    fn drop_local_from_other_thread(local: RunLoopLocalForRunLoopDrop) {
        let Some(sender) = RUN_LOOP_SENDER.lock().unwrap().as_ref().cloned() else {
            return;
        };

        let completed = Arc::new((Mutex::new(false), Condvar::new()));
        let completed_for_barrier = completed.clone();
        let sent = sender.send_shutdown_barrier(move || {
            local.drop_on_run_loop_thread();
            let (lock, condvar) = &*completed_for_barrier;
            *lock.lock().unwrap() = true;
            condvar.notify_one();
        });

        if !sent {
            warn!(
                "failed to post RunLoop local drop barrier; leaking local state to avoid cross-thread cleanup"
            );
            return;
        }

        // This intentionally has the same blocking trade-off as JUCE's
        // MessageManagerLock: the destructor waits until the run-loop thread
        // reaches the queued barrier. Returning earlier would allow the host to
        // unload the plugin DLL while run-loop-local callbacks still own plugin
        // code or data.
        let (lock, condvar) = &*completed;
        let mut done = lock.lock().unwrap();
        while !*done {
            done = condvar.wait(done).unwrap();
        }
    }

    fn cancel_pending_calls() {
        let pending_calls = std::mem::take(&mut *PENDING_CALLS.lock().unwrap());
        for pending_call in pending_calls {
            if let Some(pending_call) = pending_call.upgrade() {
                pending_call.cancel();
            }
        }
    }

    /// Posts a callback to be run later on the run loop thread.
    ///
    /// The callback is always queued, even when called from the run loop thread.
    /// Use [`call`](Self::call) when the callback must complete before returning.
    ///
    /// Returns `Ok(())` once the callback has been handed to the run loop queue.
    /// This does not mean the callback has already run or that it will run to
    /// completion: the run loop may shut down before queued work is processed.
    ///
    /// Returns an error if there is no initialized run loop to post to.
    ///
    /// The callback must be `Send + 'static` because it may be transferred from
    /// another thread to the run loop thread. Once inside the callback, use the
    /// provided [`RunLoopLocal`] to schedule timers or spawn `!Send` futures.
    pub fn post<F>(callback: F) -> Result<()>
    where
        F: FnOnce(&RunLoopLocal) + Send + 'static,
    {
        let sender = Self::sender()?;
        if !sender.send(move || {
            if let Ok(run_loop) = Self::current_local() {
                callback(&RunLoopLocal::new(run_loop));
            }
        }) {
            return Err(Error::NotInitialized);
        }
        Ok(())
    }

    /// Returns a Future that completes after the specified duration.
    ///
    /// Use [`RunLoopLocal::delay`] when already running inside a run-loop
    /// callback. This associated function is for async code that does not own a
    /// [`RunLoopLocal`]; it posts only the timer registration to the run loop and
    /// wakes the awaiting task from the timer callback.
    ///
    /// Returns an error if there is no initialized run loop to post to.
    pub async fn delay(duration: Duration) -> Result<()> {
        let completed = Arc::new(AtomicBool::new(false));
        let mut scheduled = false;

        poll_fn(move |cx| {
            if completed.load(Ordering::Acquire) {
                return Poll::Ready(Ok(()));
            }

            if !scheduled {
                scheduled = true;
                let completed = completed.clone();
                let waker = cx.waker().clone();
                if let Err(error) = Self::post(move |run_loop| {
                    run_loop
                        .schedule(duration, move |_| {
                            completed.store(true, Ordering::Release);
                            waker.wake();
                        })
                        .detach();
                }) {
                    return Poll::Ready(Err(error));
                }
            }

            Poll::Pending
        })
        .await
    }

    /// Runs a callback on the run loop thread and returns its result.
    ///
    /// If called from the run loop thread, the callback runs immediately.
    /// Otherwise, it is posted to the run loop thread and this function blocks
    /// until the callback completes.
    ///
    /// Returns an error if there is no initialized run loop to call into, or if
    /// the run loop shuts down before the queued callback starts. Errors or
    /// values produced by the callback itself should be represented in `R` (for
    /// example by returning a `Result` from the callback).
    ///
    /// Be careful when calling this while holding locks: unrelated run loop
    /// callbacks may need the same locks before this callback can run.
    pub fn call<F, R>(callback: F) -> Result<R>
    where
        F: FnOnce(&RunLoopLocal) -> R + Send + 'static,
        R: Send + 'static,
    {
        if Self::is_run_loop_thread() {
            let run_loop = Self::current_local()?;
            return Ok(callback(&RunLoopLocal::new(run_loop)));
        }

        let sender = Self::sender()?;
        let var = BlockingVariable::<Result<R>>::new();
        let completion = Arc::new(CallCompletion::new(var.clone()));
        let pending_call: Arc<dyn PendingCall> = completion.clone();
        {
            let mut pending_calls = PENDING_CALLS.lock().unwrap();
            pending_calls.retain(|pending_call| pending_call.upgrade().is_some());
            pending_calls.push(Arc::downgrade(&pending_call));
        }
        if !sender.send(move || {
            let result =
                Self::current_local().map(|run_loop| callback(&RunLoopLocal::new(run_loop)));
            completion.complete(result);
        }) {
            return Err(Error::NotInitialized);
        }
        var.get_blocking()
    }

    #[doc(hidden)]
    pub fn sender() -> Result<RunLoopSender> {
        RUN_LOOP_SENDER
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or(Error::NotInitialized)
    }

    /// Returns whether the current thread is the run loop thread.
    pub fn is_run_loop_thread() -> bool {
        let thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();
        if let Some(run_loop_thread_id) = *thread_id {
            let current_id = thread::current().id();
            current_id == run_loop_thread_id
        } else {
            false
        }
    }

    fn current_local() -> Result<Self> {
        let current_thread = thread::current().id();
        let thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();

        if let Some(run_loop_thread_id) = *thread_id {
            if current_thread != run_loop_thread_id {
                return Err(Error::NotRunLoopThread);
            }
        } else {
            return Err(Error::NotInitialized);
        }

        let instance = RUN_LOOP_INSTANCE.get().ok_or(Error::NotInitialized)?;

        if instance.has_shutdown.load(Ordering::SeqCst) {
            return Err(Error::NotInitialized);
        }

        Ok(RunLoop {
            inner: instance.clone(),
        })
    }
}

impl RunLoopLocal {
    /// Schedules `callback` to be executed after `in_time`.
    ///
    /// Returns a [`Handle`] that must be kept alive until the callback executes.
    /// Dropping the handle early cancels the callback.
    ///
    /// * Call [`Handle::detach()`] to ensure execution even after the handle is dropped.
    /// * Call [`Handle::cancel()`] to cancel without dropping the handle.
    #[must_use]
    pub fn schedule<F>(&self, in_time: Duration, callback: F) -> Handle
    where
        F: FnOnce(&RunLoopLocal) + 'static,
    {
        let inner_for_callback = self.run_loop.inner.clone();
        let handle = self
            .run_loop
            .inner
            .platform_run_loop
            .schedule(in_time, move || {
                callback(&RunLoopLocal::new(RunLoop {
                    inner: inner_for_callback,
                }));
            });
        let inner_clone = self.run_loop.inner.clone();
        Handle::new(move || {
            inner_clone.platform_run_loop.unschedule(handle);
        })
    }

    /// Returns a Future that completes after the specified duration.
    pub async fn delay(&self, duration: Duration) {
        let (future, completer) = FutureCompleter::<()>::new();
        self.schedule(duration, move |_| {
            completer.complete(());
        })
        .detach();
        future.await
    }

    /// Returns a sender object that allows other threads to execute callbacks on this run loop.
    /// Unlike `RunLoop`, the sender implements `Send` and `Sync`.
    pub(crate) fn new_sender(&self) -> RunLoopSender {
        RunLoopSender::new(
            self.run_loop.inner.platform_run_loop.new_sender(),
            Arc::downgrade(&self.run_loop.inner.shutdown_token),
        )
    }

    /// Spawns a Future using this run loop as the executor.
    pub fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> JoinHandle<T> {
        // Check for shutdown
        if self.run_loop.inner.has_shutdown.load(Ordering::SeqCst) {
            panic!("Cannot spawn task on shut down RunLoop");
        }

        let task = Arc::new(Task::new(self.new_sender(), future));

        // Track only a `Weak` so a finished/dropped task can free itself; this
        // list exists purely so `deinit`/`shutdown` can abort stragglers. The
        // list would otherwise grow forever, so compact it once it gets large
        // rather than on every spawn.
        {
            let mut tasks = self.run_loop.inner.active_tasks.lock().unwrap();
            tasks.push(Arc::downgrade(&(task.clone() as Arc<dyn AbortableTask>)));

            if tasks.len() > 100 {
                tasks.retain(|weak| weak.upgrade().is_some());
            }
        }

        // Kick off the first poll by faking a wake; subsequent polls are driven
        // by the task's own waker.
        ArcWake::wake_by_ref(&task);
        JoinHandle::new(task)
    }

    /// Blocks the run loop thread until the given Future completes.
    ///
    /// This method continues to drive the RunLoop while waiting, so other tasks submitted
    /// via `spawn` can also make progress. This means a Future that depends on another task
    /// completing on the run loop will not deadlock. In contrast, external executors such as
    /// `pollster::block_on` do not drive the run loop and would deadlock in the same situation.
    ///
    /// Unlike `spawn`, this method polls the provided future directly in place without spawning it.
    /// Therefore, unlike [`spawn`](Self::spawn), it can accept futures that
    /// contain non-`'static` borrows.
    ///
    /// Nested `block_on` calls will panic.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use novonotes_run_loop::RunLoop;
    ///
    /// let guard = RunLoop::init().unwrap();
    /// let result = guard.local().block_on(async {
    ///     // Async work
    ///     42
    /// });
    /// assert_eq!(result, 42);
    /// ```
    pub fn block_on<F, T>(&self, future: F) -> T
    where
        F: Future<Output = T>,
    {
        let _block_on_guard = BlockOnActiveGuard::enter();

        if self.run_loop.inner.has_shutdown.load(Ordering::SeqCst) {
            panic!("Cannot block on shut down RunLoop");
        }

        let block_on_waker = Arc::new(BlockOnWaker::new(self.new_sender()));
        let waker = Waker::from(block_on_waker.clone());
        let mut context = Context::from_waker(&waker);
        let mut future = pin!(future);

        // Rather than using stop(), re-poll the target future each time it is woken
        // while continuing to drive RunLoop-specific sources.
        let mut poll_session = PollSession::new();

        loop {
            if block_on_waker.take_queued() {
                match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(&mut context))) {
                    Ok(std::task::Poll::Ready(value)) => return value,
                    Ok(std::task::Poll::Pending) => {}
                    Err(panic_payload) => resume_unwind(panic_payload),
                }
            }

            self.run_loop
                .inner
                .platform_run_loop
                .poll_once(&mut poll_session);
        }
    }

    /// Runs the run loop until stopped.
    ///
    /// Use this in standalone applications that drive their own run loop.
    /// In plugin environments the host already drives the loop, so this is normally not needed.
    ///
    /// `RunLoop::init()` must have completed before calling this.
    pub fn run(&self) {
        self.run_loop.inner.platform_run_loop.run()
    }

    /// Stops the run loop.
    pub fn stop(&self) {
        self.run_loop.inner.platform_run_loop.stop()
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    pub fn run_app(&self) {
        self.run_loop.inner.platform_run_loop.run_app();
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    pub fn stop_app(&self) {
        self.run_loop.inner.platform_run_loop.stop_app();
    }
}

#[cfg(test)]
#[allow(clippy::bool_assert_comparison)]
mod tests {
    use crate::{Error, RunLoop, RunLoopLocal, run_loop::PENDING_CALLS};
    use serial_test::serial;
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    #[test]
    #[serial]
    fn test_run() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();
        let next_called = Rc::new(RefCell::new(false));
        let next_called_clone = next_called.clone();
        let start = Instant::now();
        run_loop
            .schedule(Duration::from_millis(50), move |run_loop| {
                next_called_clone.replace(true);
                run_loop.stop();
            })
            .detach();
        assert_eq!(*next_called.borrow(), false);
        run_loop.run();
        assert_eq!(*next_called.borrow(), true);
        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    #[serial]
    fn test_post_from_background_thread() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();
        let stop_called = Arc::new(Mutex::new(false));
        let stop_called_clone = stop_called.clone();

        thread::spawn(move || {
            RunLoop::post(move |run_loop| {
                assert!(RunLoop::is_run_loop_thread());
                *stop_called_clone.lock().unwrap() = true;
                run_loop.stop();
            })
            .unwrap();
        })
        .join()
        .unwrap();

        assert_eq!(*stop_called.lock().unwrap(), false);
        run_loop.run();
        assert_eq!(*stop_called.lock().unwrap(), true);
    }

    #[test]
    #[serial]
    fn test_call_from_run_loop_thread_runs_inline() {
        let _guard = RunLoop::init().unwrap();
        let value = Arc::new(Mutex::new(0));
        let value_for_call = value.clone();

        RunLoop::call(move |_| {
            *value_for_call.lock().unwrap() = 42;
        })
        .unwrap();

        assert_eq!(*value.lock().unwrap(), 42);
    }

    #[test]
    #[serial]
    fn test_call_from_background_thread_waits_for_result() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();

        let handle = thread::spawn(|| RunLoop::call(|_| 42).unwrap());
        run_loop
            .schedule(Duration::from_millis(10), |run_loop| run_loop.stop())
            .detach();
        run_loop.run();

        assert_eq!(handle.join().unwrap(), 42);
    }

    #[test]
    #[serial]
    fn test_call_from_background_thread_unblocks_on_shutdown() {
        let guard = RunLoop::init().unwrap();

        let handle = thread::spawn(|| RunLoop::call(|_| 42));

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let has_pending_call = PENDING_CALLS
                .lock()
                .unwrap()
                .iter()
                .any(|pending_call| pending_call.upgrade().is_some());
            if has_pending_call {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "background RunLoop::call did not register as pending"
            );
            thread::sleep(Duration::from_millis(1));
        }

        drop(guard);

        assert!(matches!(handle.join().unwrap(), Err(Error::NotInitialized)));
    }

    #[test]
    #[serial]
    fn test_async() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();
        let start = Instant::now();
        run_loop
            .schedule(Duration::from_millis(50), |run_loop| run_loop.stop())
            .detach();
        run_loop.run();
        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    #[serial]
    fn test_init_deinit_reinit() {
        let guard = RunLoop::init().unwrap();
        assert!(RunLoop::is_initialized());
        drop(guard);
        assert!(!RunLoop::is_initialized());

        // Can re-init on another thread
        let handle = thread::spawn(|| {
            let _guard = RunLoop::init().unwrap();
            assert!(RunLoop::is_run_loop_thread());
        });
        handle.join().unwrap();

        // Can re-init on the original thread as well
        let _guard = RunLoop::init().unwrap();
        assert!(RunLoop::is_run_loop_thread());
    }

    #[test]
    #[serial]
    fn test_deinit_aborts_all_tasks() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();
        let task1_started = Arc::new(AtomicBool::new(false));
        let task2_started = Arc::new(AtomicBool::new(false));

        let t1_started = task1_started.clone();
        let t2_started = task2_started.clone();

        let handle1 = run_loop.spawn(async move {
            t1_started.store(true, Ordering::SeqCst);
            loop {
                std::future::pending::<()>().await;
            }
        });

        let handle2 = run_loop.spawn(async move {
            t2_started.store(true, Ordering::SeqCst);
            loop {
                std::future::pending::<()>().await;
            }
        });

        run_loop
            .schedule(Duration::from_millis(300), |run_loop| run_loop.stop())
            .detach();
        run_loop.run();

        assert!(task1_started.load(Ordering::SeqCst));
        assert!(task2_started.load(Ordering::SeqCst));

        drop(guard);

        let result1 = pollster::block_on(handle1);
        let result2 = pollster::block_on(handle2);

        assert!(matches!(result1, Err(crate::JoinError::Aborted)));
        assert!(matches!(result2, Err(crate::JoinError::Aborted)));
    }

    #[test]
    #[serial]
    fn test_drop_guard_from_background_thread_uses_run_loop_shutdown_barrier() {
        struct DropThreadRecorder {
            dropped_on_run_loop: Arc<AtomicBool>,
        }

        impl Drop for DropThreadRecorder {
            fn drop(&mut self) {
                self.dropped_on_run_loop
                    .store(RunLoop::is_run_loop_thread(), Ordering::SeqCst);
            }
        }

        let guard = RunLoop::init().unwrap();
        let run_loop = {
            let local = guard.local();
            RunLoopLocal::new(RunLoop {
                inner: local.run_loop.inner.clone(),
            })
        };
        let dropped_on_run_loop = Arc::new(AtomicBool::new(false));
        let dropped_on_run_loop_for_task = dropped_on_run_loop.clone();
        let _handle = run_loop.spawn(async move {
            let _recorder = DropThreadRecorder {
                dropped_on_run_loop: dropped_on_run_loop_for_task,
            };
            std::future::pending::<()>().await;
        });

        let release_done = Arc::new(AtomicBool::new(false));
        let release_done_for_thread = release_done.clone();
        let release_thread = thread::spawn(move || {
            drop(guard);
            release_done_for_thread.store(true, Ordering::SeqCst);
        });

        run_loop.block_on(futures::future::poll_fn(|cx| {
            if release_done.load(Ordering::SeqCst) {
                std::task::Poll::Ready(())
            } else {
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        }));
        release_thread.join().unwrap();

        assert!(dropped_on_run_loop.load(Ordering::SeqCst));
        assert!(!RunLoop::is_initialized());
        assert!(matches!(RunLoop::post(|_| {}), Err(Error::NotInitialized)));
    }

    #[test]
    #[serial]
    fn test_drop_non_last_guard_from_background_thread_keeps_run_loop_alive() {
        let guard_to_drop = RunLoop::init().unwrap();
        let guard_to_keep = RunLoop::init().unwrap();
        let run_loop = {
            let local = guard_to_keep.local();
            RunLoopLocal::new(RunLoop {
                inner: local.run_loop.inner.clone(),
            })
        };

        let release_done = Arc::new(AtomicBool::new(false));
        let release_done_for_thread = release_done.clone();
        let release_thread = thread::spawn(move || {
            drop(guard_to_drop);
            release_done_for_thread.store(true, Ordering::SeqCst);
        });

        run_loop.block_on(futures::future::poll_fn(|cx| {
            if release_done.load(Ordering::SeqCst) {
                std::task::Poll::Ready(())
            } else {
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        }));
        release_thread.join().unwrap();

        assert!(RunLoop::is_initialized());
        RunLoop::post(|_| {}).unwrap();

        drop(guard_to_keep);
    }

    #[test]
    #[serial]
    fn test_block_on_simple() {
        let guard = RunLoop::init().unwrap();
        let result = guard.local().block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    #[serial]
    fn test_block_on_with_delay() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();
        let start = Instant::now();
        let result = run_loop.block_on(async {
            run_loop.delay(Duration::from_millis(50)).await;
            "completed"
        });
        assert_eq!(result, "completed");
        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    #[serial]
    fn test_block_on_drives_spawned_tasks() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();
        let completed = Arc::new(AtomicBool::new(false));
        let completed_for_task = completed.clone();

        let handle = run_loop.spawn(async move {
            completed_for_task.store(true, Ordering::SeqCst);
            123
        });

        let result = run_loop.block_on(async move { handle.await.unwrap() });
        assert_eq!(result, 123);
        assert!(completed.load(Ordering::SeqCst));
    }

    #[test]
    #[serial]
    fn test_block_on_nested() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_loop.block_on(async {
                let inner_result = run_loop.block_on(async { "inner" });
                format!("outer: {}", inner_result)
            });
        }));

        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_block_on_panic() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_loop.block_on(async {
                panic!("Task panicked");
            });
        }));

        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_block_on_recovers_after_panic() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();

        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_loop.block_on(async {
                panic!("Task panicked");
            });
        }));
        assert!(panic_result.is_err());

        let result = run_loop.block_on(async { 7 });
        assert_eq!(result, 7);
    }

    #[test]
    #[serial]
    fn test_block_on_non_static_future() {
        let guard = RunLoop::init().unwrap();
        let run_loop = guard.local();

        struct Counter {
            value: u32,
        }

        impl Counter {
            async fn increment_and_get(&mut self) -> u32 {
                self.value += 1;
                self.value
            }
        }

        let mut counter = Counter { value: 41 };
        let result = run_loop.block_on(counter.increment_and_get());

        assert_eq!(result, 42);
        assert_eq!(counter.value, 42);
    }

    #[test]
    #[serial]
    fn test_block_on_after_deinit_panics() {
        let guard = RunLoop::init().unwrap();
        let run_loop = RunLoopLocal::new(RunLoop {
            inner: guard.local().run_loop.inner.clone(),
        });

        drop(guard);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_loop.block_on(async { 1 });
        }));

        assert!(result.is_err());
    }
}
