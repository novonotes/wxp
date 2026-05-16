use log::warn;
use std::{
    fmt::Display,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    pin::pin,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Wake, Waker},
    thread::{self, ThreadId},
    time::Duration,
};

use futures::{Future, task::ArcWake};

use crate::{
    Handle, JoinHandle, RunLoopSender, Task,
    main_thread::MainThreadFacilitator,
    platform::{PlatformRunLoop, PollSession},
    task::AbortableTask,
    util::FutureCompleter,
};

// Lets a `!Send`/`!Sync` value (e.g. the platform run loop, which is `Rc`-based)
// live in a `static`. Rust normally forbids this; the type is sound here *only*
// because the contained value is read via the run-loop-thread checks in
// `RunLoop::current`/`try_current`, and is installed/cleared only by
// `initialize`/`shutdown`, which are serialized under `INIT_MUTEX`. So the
// interior is never touched off the run loop thread or concurrently.
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
static RUN_LOOP_THREAD_ID: Mutex<Option<ThreadId>> = Mutex::new(None);

// init/deinit are reference-counted (CLAP/VST3 style): a host may load the same
// plugin DLL into several instances that each call init/deinit, but the loop
// must be created once and torn down only when the last instance leaves.
// `INIT_MUTEX` serializes those transitions; `BLOCK_ON_ACTIVE` detects the
// unsupported re-entrant `block_on`.
static INIT_COUNT: AtomicUsize = AtomicUsize::new(0);
static INIT_MUTEX: Mutex<()> = Mutex::new(());
static BLOCK_ON_ACTIVE: AtomicBool = AtomicBool::new(false);

struct BlockOnActiveGuard;

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
            self.sender.send(|| {});
        }
    }
}

struct RunLoopInner {
    platform_run_loop: Rc<PlatformRunLoop>,
    active_tasks: Mutex<Vec<std::sync::Weak<dyn AbortableTask>>>,
    has_shutdown: AtomicBool,
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

/// An abstraction over the platform-specific RunLoop.
///
/// ## Run loop thread
///
/// The thread that called `RunLoop::init()`. At any given moment only a single run loop thread
/// may exist in the entire process (MUST). `RunLoop::current()` is only usable on this thread.
///
/// - Ordinary applications: should call `RunLoop::init()` on the main thread (SHOULD)
/// - Test environments: any thread may be designated as the run loop thread (MAY)
/// - Thread switching: can be changed by calling `deinit()` then `init()` on another thread (MAY)
pub struct RunLoop {
    inner: Arc<RunLoopInner>,
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
    /// Call during application/DLL initialization (equivalent to CLAP `init` or VST3 `InitDll`).
    /// Safe to call multiple times (defensive implementation).
    pub fn init() -> Result<()> {
        let _guard = INIT_MUTEX.lock().unwrap();

        let count = INIT_COUNT.fetch_add(1, Ordering::SeqCst);
        if count == 0 {
            // Only initialize on the first call
            Self::initialize()?;
        }

        if !Self::is_run_loop_thread() {
            return Err(Error::NotRunLoopThread);
        }

        Ok(())
    }

    /// Forcibly rebinds the run loop to the current thread.
    ///
    /// Mainly a test-suite escape hatch: tests are serialized but a previous
    /// test may have left the loop bound to a now-dead thread. Rather than
    /// failing, tear the old loop down and rebuild it here, preserving the
    /// existing init count so reference counting stays balanced.
    pub fn ensure_run_loop_on_current_thread() -> Result<()> {
        let guard = INIT_MUTEX.lock().unwrap();
        let count = INIT_COUNT.load(Ordering::SeqCst);

        if count == 0 {
            // Nothing initialized yet — the normal path is sufficient.
            drop(guard);
            return Self::init();
        }

        if Self::is_run_loop_thread() {
            // Already where we want to be; nothing to rebuild.
            return Ok(());
        }

        // Bound to a different (likely dead) thread: rebuild in place and
        // restore the original count so callers' deinit pairing still holds.
        INIT_COUNT.store(0, Ordering::SeqCst);
        Self::shutdown();

        Self::initialize()?;
        INIT_COUNT.store(count, Ordering::SeqCst);
        debug_assert!(Self::is_run_loop_thread());
        Ok(())
    }

    /// Call during application/DLL teardown (equivalent to CLAP `deinit` or VST3 `ExitDll`).
    /// Must be called the same number of times as `init()`.
    pub fn deinit() {
        let _guard = INIT_MUTEX.lock().unwrap();

        let count = INIT_COUNT.fetch_sub(1, Ordering::SeqCst);
        if count == 1 {
            // Perform actual cleanup on the last call
            Self::shutdown();
        }
    }

    /// Internal only: performs the actual initialization.
    fn initialize() -> Result<()> {
        // Record the current thread as the run loop thread
        {
            let mut thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();
            *thread_id = Some(thread::current().id());
        }

        // Create the RunLoop instance
        let inner = Arc::new(RunLoopInner {
            platform_run_loop: Rc::new(PlatformRunLoop::new()),
            active_tasks: Mutex::new(Vec::new()),
            has_shutdown: AtomicBool::new(false),
        });

        RUN_LOOP_INSTANCE
            .set(inner)
            .map_err(|_| Error::AlreadyInitialized)?;

        // Set up MainThreadFacilitator (works even without Flutter plugin)
        MainThreadFacilitator::set_for_current_thread();

        Ok(())
    }

    /// Internal only: performs the actual cleanup.
    fn shutdown() {
        if let Some(instance) = RUN_LOOP_INSTANCE.get() {
            // Record that shutdown is complete
            instance.has_shutdown.store(true, Ordering::SeqCst);

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

        // Reset MainThreadFacilitator
        MainThreadFacilitator::reset();
    }

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
        F: FnOnce() + 'static,
    {
        let platform_run_loop = &self.inner.platform_run_loop;
        let handle = platform_run_loop.schedule(in_time, callback);
        let inner_clone = self.inner.clone();
        Handle::new(move || {
            inner_clone.platform_run_loop.unschedule(handle);
        })
    }

    /// Returns a Future that completes after the specified duration.
    pub async fn delay(&self, duration: Duration) {
        let (future, completer) = FutureCompleter::<()>::new();
        self.schedule(duration, move || {
            completer.complete(());
        })
        .detach();
        future.await
    }

    /// Returns a sender that posts callbacks onto the run loop thread.
    ///
    /// Callable from any thread — this is the cross-thread entry point. On the
    /// run loop thread itself a concrete platform sender is cheap; from other
    /// threads we hand back the indirect main-thread sender, which routes
    /// through the facilitator without needing the (`!Send`) `RunLoop` here.
    pub fn sender() -> RunLoopSender {
        if Self::is_run_loop_thread() {
            RunLoop::current().new_sender()
        } else {
            MainThreadFacilitator::is_main_thread()
                .map(|_| RunLoopSender::new_for_run_loop_thread())
                .unwrap()
        }
    }

    /// Returns a sender object that allows other threads to execute callbacks on this run loop.
    /// Unlike `RunLoop`, the sender implements `Send` and `Sync`.
    pub(crate) fn new_sender(&self) -> RunLoopSender {
        RunLoopSender::new(self.inner.platform_run_loop.new_sender())
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

    /// Sets the current thread as the run loop thread.
    ///
    /// [deprecated]
    /// Retained only in case it is needed for integrating with older Flutter versions.
    /// In general, the run loop thread should be designated via `RunLoop::init()` instead.
    ///
    /// Call from the run loop thread after `RunLoop::init()` when not using the
    /// irondash_engine_context plugin. This allows `RunLoop::sender_for_run_loop_thread()`
    /// to work without a Flutter plugin.
    #[deprecated(note = "Use RunLoop::init() instead")]
    pub fn set_run_loop_thread() {
        {
            let mut thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();
            *thread_id = Some(thread::current().id());
        }

        // Set up MainThreadFacilitator (works even without Flutter plugin)
        use crate::main_thread::MainThreadFacilitator;
        MainThreadFacilitator::set_for_current_thread();
    }

    /// Spawns a Future using this run loop as the executor.
    pub fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> JoinHandle<T> {
        // Check for shutdown
        if self.inner.has_shutdown.load(Ordering::SeqCst) {
            panic!("Cannot spawn task on shut down RunLoop");
        }

        let task = Arc::new(Task::new(self.new_sender(), future));

        // Track only a `Weak` so a finished/dropped task can free itself; this
        // list exists purely so `deinit`/`shutdown` can abort stragglers. The
        // list would otherwise grow forever, so compact it once it gets large
        // rather than on every spawn.
        {
            let mut tasks = self.inner.active_tasks.lock().unwrap();
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

    /// Synchronously blocks the current thread until the given Future completes.
    ///
    /// This method continues to drive the RunLoop while waiting, so other tasks submitted
    /// via `spawn` can also make progress. This means a Future that depends on another task
    /// completing on the run loop will not deadlock. In contrast, external executors such as
    /// `pollster::block_on` do not drive the run loop and would deadlock in the same situation.
    ///
    /// Unlike `spawn`, this method polls the provided future directly in place without spawning it.
    /// Therefore, unlike `RunLoop::spawn`, it can accept futures that contain non-`'static` borrows.
    ///
    /// When the `flutter` feature is enabled, the normal polling path (for `run`) may fall back to
    /// the platform default event queue as needed, but `block_on` always processes only RunLoop
    /// specific sources.
    ///
    /// Nested `block_on` calls will panic.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use novonotes_run_loop::RunLoop;
    ///
    /// RunLoop::init().unwrap();
    /// let result = RunLoop::current().block_on(async {
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

        if self.inner.has_shutdown.load(Ordering::SeqCst) {
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

            self.inner.platform_run_loop.poll_once(&mut poll_session);
        }
    }

    /// Returns the RunLoop for the current thread.
    /// Must be called from the run loop thread; panics otherwise.
    pub fn current() -> Self {
        // Verify we are on the run loop thread
        let current_thread = thread::current().id();
        let thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();

        if let Some(run_loop_thread_id) = *thread_id {
            if current_thread != run_loop_thread_id {
                panic!("RunLoop::current() can only be called from the run loop thread");
            }
        } else {
            panic!("RunLoop not initialized. Call RunLoop::init() first");
        }

        // Retrieve the instance
        let instance = RUN_LOOP_INSTANCE
            .get()
            .expect("RunLoop not initialized. Call RunLoop::init() first");

        // Check for shutdown
        if instance.has_shutdown.load(Ordering::SeqCst) {
            panic!("RunLoop has been shut down");
        }

        RunLoop {
            inner: instance.clone(),
        }
    }

    /// Fallible variant of [`RunLoop::current()`].
    ///
    /// Returns an error if the RunLoop is not initialized on the current thread.
    pub fn try_current() -> Result<Self> {
        // Verify we are on the run loop thread
        let current_thread = thread::current().id();
        let thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();

        if let Some(run_loop_thread_id) = *thread_id {
            if current_thread != run_loop_thread_id {
                return Err(Error::NotInitialized);
            }
        } else {
            return Err(Error::NotInitialized);
        }

        // Retrieve the instance
        let instance = RUN_LOOP_INSTANCE.get().ok_or(Error::NotInitialized)?;

        // Check for shutdown
        if instance.has_shutdown.load(Ordering::SeqCst) {
            return Err(Error::NotInitialized);
        }

        Ok(RunLoop {
            inner: instance.clone(),
        })
    }

    /// Runs the run loop until stopped.
    ///
    /// Use this in standalone applications that drive their own run loop.
    /// In plugin environments the host already drives the loop, so this is normally not needed.
    ///
    /// `RunLoop::init()` must have completed before calling this.
    pub fn run(&self) {
        self.inner.platform_run_loop.run()
    }

    /// Stops the run loop.
    pub fn stop(&self) {
        self.inner.platform_run_loop.stop()
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    pub fn run_app(&self) {
        self.inner.platform_run_loop.run_app();
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    pub fn stop_app(&self) {
        self.inner.platform_run_loop.stop_app();
    }
}

/// Spawns a Future using the current thread's RunLoop as the executor.
/// The RunLoop must have been initialized with `RunLoop::init()` beforehand.
pub fn spawn<T: 'static>(future: impl Future<Output = T> + 'static) -> JoinHandle<T> {
    RunLoop::current().spawn(future)
}

#[cfg(test)]
#[allow(clippy::bool_assert_comparison)]
mod tests {
    use crate::{RunLoop, test_helper, util::Capsule};
    use serial_test::serial;
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    #[test]
    #[serial]
    fn test_run() {
        RunLoop::init().unwrap();
        let rl = Rc::new(RunLoop::current());
        let rlc = rl.clone();
        let next_called = Rc::new(RefCell::new(false));
        let next_called_clone = next_called.clone();
        let start = Instant::now();
        rl.schedule(Duration::from_millis(50), move || {
            next_called_clone.replace(true);
            rlc.stop();
        })
        .detach();
        assert_eq!(*next_called.borrow(), false);
        rl.run();
        assert_eq!(*next_called.borrow(), true);
        assert!(start.elapsed() >= Duration::from_millis(50));
        RunLoop::deinit();
    }

    #[test]
    #[serial]
    fn test_sender() {
        RunLoop::init().unwrap();
        let run_loop = Rc::new(RunLoop::current());
        let rl = Arc::new(Mutex::new(Capsule::new(run_loop.clone())));
        let sender = run_loop.new_sender();
        let stop_called = Arc::new(Mutex::new(false));
        let stop_called_clone = stop_called.clone();
        // Confirm that a thread can be spawned while the run loop is already running
        // run_loop.schedule(Duration::from_secs(1000), || {}).detach();
        run_loop
            .schedule(Duration::from_secs(0), || {
                thread::spawn(move || {
                    sender.send(move || {
                        let rl = rl.lock().unwrap();
                        let rl = rl.get_ref().unwrap();
                        *stop_called_clone.lock().unwrap() = true;
                        rl.stop();
                    });
                });
            })
            .detach();
        assert_eq!(*stop_called.lock().unwrap(), false);
        run_loop.run();
        assert_eq!(*stop_called.lock().unwrap(), true);
        RunLoop::deinit();
    }

    #[test]
    #[serial]
    fn test_sender_in_background_thread() {
        test_helper::run_async(async {
            let (tx, rx) = futures::channel::oneshot::channel();

            let handle = thread::spawn(move || {
                let sender = RunLoop::sender();
                sender.send(move || {
                    assert!(RunLoop::is_run_loop_thread());
                    tx.send(()).unwrap();
                });
            });

            // Wait until the callback executes
            rx.await.unwrap();

            handle.join().unwrap();
        });
    }

    #[test]
    #[serial]
    fn test_async() {
        RunLoop::init().unwrap();
        let run_loop = Rc::new(RunLoop::current());
        let run_loop_clone = run_loop.clone();
        run_loop.spawn(async move {
            RunLoop::current().delay(Duration::from_millis(50)).await;
            run_loop_clone.stop();
        });
        let start = Instant::now();
        run_loop.run();
        assert!(start.elapsed() >= Duration::from_millis(50));
        RunLoop::deinit();
    }

    #[test]
    #[serial]
    fn test_init_deinit_reinit() {
        // First init
        RunLoop::init().unwrap();
        assert!(RunLoop::is_run_loop_thread());

        // deinit clears the state
        RunLoop::deinit();

        // Can re-init on another thread
        let handle = thread::spawn(|| {
            RunLoop::init().unwrap();
            assert!(RunLoop::is_run_loop_thread());
            RunLoop::deinit();
        });
        handle.join().unwrap();

        // Can re-init on the original thread as well
        RunLoop::init().unwrap();
        assert!(RunLoop::is_run_loop_thread());
        RunLoop::deinit();
    }

    #[test]
    #[serial]
    fn test_deinit_aborts_all_tasks() {
        use std::sync::atomic::{AtomicBool, Ordering};

        RunLoop::init().unwrap();

        // Track whether each task has started
        let task1_started = Arc::new(AtomicBool::new(false));
        let task2_started = Arc::new(AtomicBool::new(false));

        let t1_started = task1_started.clone();
        let t2_started = task2_started.clone();

        // Spawn long-running tasks that wait to be aborted
        let handle1 = RunLoop::current().spawn(async move {
            t1_started.store(true, Ordering::SeqCst);
            loop {
                std::future::pending::<()>().await;
            }
        });

        let handle2 = RunLoop::current().spawn(async move {
            t2_started.store(true, Ordering::SeqCst);
            loop {
                std::future::pending::<()>().await;
            }
        });

        // Run the loop briefly to let the tasks start, then stop
        RunLoop::current()
            .schedule(Duration::from_millis(300), || {
                RunLoop::current().stop();
            })
            .detach();
        RunLoop::current().run();

        // Confirm the tasks started
        assert!(task1_started.load(Ordering::SeqCst));
        assert!(task2_started.load(Ordering::SeqCst));

        // deinit() should abort all tasks
        RunLoop::deinit();

        // Confirm both tasks were aborted
        let result1 = pollster::block_on(handle1);
        let result2 = pollster::block_on(handle2);

        assert!(matches!(result1, Err(crate::JoinError::Aborted)));
        assert!(matches!(result2, Err(crate::JoinError::Aborted)));
    }

    #[test]
    #[serial]
    fn test_block_on_simple() {
        RunLoop::init().unwrap();
        let result = RunLoop::current().block_on(async { 42 });
        assert_eq!(result, 42);
        RunLoop::deinit();
    }

    #[test]
    #[serial]
    fn test_block_on_with_delay() {
        RunLoop::init().unwrap();
        let start = Instant::now();
        let result = RunLoop::current().block_on(async {
            RunLoop::current().delay(Duration::from_millis(50)).await;
            "completed"
        });
        assert_eq!(result, "completed");
        assert!(start.elapsed() >= Duration::from_millis(50));
        RunLoop::deinit();
    }

    #[test]
    #[serial]
    fn test_block_on_drives_spawned_tasks() {
        RunLoop::init().unwrap();

        // Verify that the RunLoop continues to run during block_on so that
        // separately spawned tasks can make progress.
        let handle = RunLoop::current().spawn(async move {
            RunLoop::current().delay(Duration::from_millis(20)).await;
            123
        });

        let result = RunLoop::current().block_on(async move { handle.await.unwrap() });

        assert_eq!(result, 123);
        RunLoop::deinit();
    }

    #[test]
    #[serial]
    fn test_block_on_nested() {
        // Use ensure to avoid interference from a previous test
        RunLoop::ensure_run_loop_on_current_thread().unwrap();

        // Nested block_on is undefined behavior and should be detected
        let result = std::panic::catch_unwind(|| {
            RunLoop::current().block_on(async {
                let inner_result = RunLoop::current().block_on(async { "inner" });
                format!("outer: {}", inner_result)
            });
        });

        RunLoop::deinit();
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_block_on_panic() {
        // Use ensure to avoid interference from a previous test
        RunLoop::ensure_run_loop_on_current_thread().unwrap();

        // Catch the panic so deinit is always called
        let result = std::panic::catch_unwind(|| {
            RunLoop::current().block_on(async {
                panic!("Task panicked");
            });
        });

        RunLoop::deinit();
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_block_on_recovers_after_panic() {
        RunLoop::ensure_run_loop_on_current_thread().unwrap();

        // After exiting block_on via a panic, the nesting flag should be cleared
        // so subsequent calls work correctly.
        let panic_result = std::panic::catch_unwind(|| {
            RunLoop::current().block_on(async {
                panic!("Task panicked");
            });
        });
        assert!(panic_result.is_err());

        let result = RunLoop::current().block_on(async { 7 });
        assert_eq!(result, 7);

        RunLoop::deinit();
    }

    #[test]
    #[serial]
    fn test_block_on_non_static_future() {
        RunLoop::init().unwrap();

        struct Counter {
            value: u32,
        }

        impl Counter {
            async fn increment_and_get(&mut self) -> u32 {
                RunLoop::current().delay(Duration::from_millis(10)).await;
                self.value += 1;
                self.value
            }
        }

        // Confirm that a non-`'static` future borrowing `&mut self` can be passed to block_on.
        let mut counter = Counter { value: 41 };
        let result = RunLoop::current().block_on(counter.increment_and_get());

        assert_eq!(result, 42);
        assert_eq!(counter.value, 42);
        RunLoop::deinit();
    }

    #[test]
    #[serial]
    fn test_block_on_after_deinit_panics() {
        RunLoop::init().unwrap();
        let run_loop = RunLoop::current();

        RunLoop::deinit();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_loop.block_on(async { 1 });
        }));

        assert!(result.is_err());
    }
}
