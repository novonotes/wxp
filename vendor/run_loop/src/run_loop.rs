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

// ランループスレッドからのみアクセスされる静的変数用のラッパー
// Sendだが!Syncな型を静的変数に格納できるようにする
struct RunLoopThreadOnly<T> {
    inner: std::cell::UnsafeCell<Option<T>>,
}

// MainThreadOnlyは!Sendな型も格納できるが、
// ランループスレッドからのみアクセスされることを前提としている
unsafe impl<T> Send for RunLoopThreadOnly<T> {}
unsafe impl<T> Sync for RunLoopThreadOnly<T> {}
// 警告: この実装は危険！ランループスレッドからのみアクセスすることを保証する必要がある

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

// グローバルシングルトン実装
static RUN_LOOP_INSTANCE: RunLoopThreadOnly<Arc<RunLoopInner>> = RunLoopThreadOnly::new();
static RUN_LOOP_THREAD_ID: Mutex<Option<ThreadId>> = Mutex::new(None);

// CLAPパターンに従った初期化カウント
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
        // `block_on` は待機中に platform の poll ループへ制御を返しているため、
        // future が wake されたら RunLoop をもう一度回すきっかけが必要になる。
        // ここでは空コールバックを 1 件だけ再投入し、過剰な wake スパムを避ける。
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
        // 通常は shutdown() で has_shutdown が設定されるが、
        // 異常終了時（deinit() が呼ばれずに DLL がアンロードされる等）にも
        // has_shutdown を true にすることで、他のコードが適切に動作を停止できるようにする
        self.has_shutdown.store(true, Ordering::SeqCst);

        // アクティブタスクのクリーンアップ
        if let Ok(tasks) = self.active_tasks.lock() {
            // タスクは既にWeakなので、強制的なabortは不要
            // ただしログを出力
            let active_count = tasks.iter().filter(|t| t.upgrade().is_some()).count();
            if active_count > 0 {
                warn!(
                    "Warning: RunLoop dropped with {} active tasks",
                    active_count
                );
            }
        }

        // プラットフォーム固有のクリーンアップは各プラットフォームの Drop 実装で自動的に行われる
    }
}

/// プラットフォーム固有の RunLoop を抽象化した型
///
/// ランループスレッドとは
///
/// RunLoop::init() を呼び出したスレッドのこと。任意の時点で存在できるランループスレッドは
/// システム全体で単一のスレッドのみ（MUST）。このスレッドでのみ RunLoop::current() が使用可能。
///
/// - 通常のアプリケーション: メインスレッドで RunLoop::init() を呼び出すべき（SHOULD）
/// - テスト環境: 任意のスレッドをランループスレッドに指定可能（MAY）
/// - スレッド切り替え: deinit() 後に別スレッドで init() することで切り替え可能（MAY）
pub struct RunLoop {
    inner: Arc<RunLoopInner>,
}

#[derive(Debug, Clone)]
pub enum Error {
    /// エンジンコンテキストプラグインが読み込まれていない。
    /// メインスレッドsenderへのアクセスにはirondash_engine_context Flutterプラグインが必要。
    #[cfg(feature = "flutter")]
    EngineContextPluginError(irondash_engine_context::Error),

    /// RunLoopは既に初期化されている
    AlreadyInitialized,

    /// RunLoopが初期化されていない。先にRunLoop::init()を呼び出すこと
    NotInitialized,

    /// RunLoopスレッド以外から呼び出された
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

// thread_local削除 - グローバルシングルトンに置き換え

impl RunLoop {
    /// アプリケーション/DLLの初期化時に呼び出す（CLAPのinit、VST3のInitDll相当）
    /// 複数回呼ばれても安全（防御的実装）
    pub fn init() -> Result<()> {
        let _guard = INIT_MUTEX.lock().unwrap();

        let count = INIT_COUNT.fetch_add(1, Ordering::SeqCst);
        if count == 0 {
            // 初回のみ実際の初期化
            Self::initialize()?;
        }

        if !Self::is_run_loop_thread() {
            return Err(Error::NotRunLoopThread);
        }

        Ok(())
    }

    /// 現在のスレッドをランループスレッドとして保証する
    pub fn ensure_run_loop_on_current_thread() -> Result<()> {
        let guard = INIT_MUTEX.lock().unwrap();
        let count = INIT_COUNT.load(Ordering::SeqCst);

        if count == 0 {
            drop(guard);
            return Self::init();
        }

        if Self::is_run_loop_thread() {
            return Ok(());
        }

        INIT_COUNT.store(0, Ordering::SeqCst);
        Self::shutdown();

        Self::initialize()?;
        INIT_COUNT.store(count, Ordering::SeqCst);
        debug_assert!(Self::is_run_loop_thread());
        Ok(())
    }

    /// アプリケーション/DLLの終了時に呼び出す（CLAPのdeinit、VST3のExitDll相当）
    /// init()と同じ回数だけ呼ばれる必要がある
    pub fn deinit() {
        let _guard = INIT_MUTEX.lock().unwrap();

        let count = INIT_COUNT.fetch_sub(1, Ordering::SeqCst);
        if count == 1 {
            // 最後の呼び出しで実際のクリーンアップ
            Self::shutdown();
        }
    }

    /// 内部使用のみ：実際の初期化処理
    fn initialize() -> Result<()> {
        // 現在のスレッドをランループスレッドとして記録
        {
            let mut thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();
            *thread_id = Some(thread::current().id());
        }

        // RunLoopインスタンスを作成
        let inner = Arc::new(RunLoopInner {
            platform_run_loop: Rc::new(PlatformRunLoop::new()),
            active_tasks: Mutex::new(Vec::new()),
            has_shutdown: AtomicBool::new(false),
        });

        RUN_LOOP_INSTANCE
            .set(inner)
            .map_err(|_| Error::AlreadyInitialized)?;

        // MainThreadFacilitatorを設定（Flutter pluginがない環境でも動作するように）
        MainThreadFacilitator::set_for_current_thread();

        Ok(())
    }

    /// 内部使用のみ：実際のクリーンアップ処理
    fn shutdown() {
        if let Some(instance) = RUN_LOOP_INSTANCE.get() {
            // シャットダウン完了を記録
            instance.has_shutdown.store(true, Ordering::SeqCst);

            // アクティブタスクをすべてabort
            // abort 中の panic をキャッチしてクラッシュを防ぐ。
            // オーディオプラグインでは DAW をクラッシュさせないことが最優先。
            // これは保険であり、本来は panic が起きない設計にすべき。
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

            // アクティブタスクのリストをクリア
            if let Ok(mut tasks) = instance.active_tasks.lock() {
                tasks.clear();
            }

            // プラットフォーム固有のクリーンアップは各プラットフォームの Drop 実装で自動的に行われる
        }

        // ランループスレッドIDをクリア（次のinit()で新しいスレッドを設定可能にする）
        {
            let mut thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();
            *thread_id = None;
        }

        // RunLoopインスタンスもクリア（次のinit()で新しいインスタンスを作成可能にする）
        RUN_LOOP_INSTANCE.clear();

        // MainThreadFacilitatorをリセット
        MainThreadFacilitator::reset();
    }

    /// 指定された遅延後にコールバックを実行するようスケジュール。
    ///
    /// コールバックが実行されるまで保持する必要がある[`Handle`]を返す。
    /// ハンドルが早期にドロップされると、コールバックはキャンセルされる。
    ///
    /// * [`Handle::detach()`]を呼ぶとハンドルをドロップしても実行が保証される
    /// * [`Handle::cancel()`]でハンドルをドロップせずにキャンセル可能
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

    /// 指定された時間後に完了するFutureを返す。
    pub async fn delay(&self, duration: Duration) {
        let (future, completer) = FutureCompleter::<()>::new();
        self.schedule(duration, move || {
            completer.complete(());
        })
        .detach();
        future.await
    }

    /// ランループスレッドへコールバックを送信できるsenderオブジェクトを返す。
    pub fn sender() -> RunLoopSender {
        if Self::is_run_loop_thread() {
            RunLoop::current().new_sender()
        } else {
            MainThreadFacilitator::is_main_thread()
                .map(|_| RunLoopSender::new_for_run_loop_thread())
                .unwrap()
        }
    }

    /// 他のスレッドからこのランループでコールバックを実行するための
    /// senderオブジェクトを返す。
    /// senderは`RunLoop`と異なり`Send`と`Sync`を実装している。
    pub(crate) fn new_sender(&self) -> RunLoopSender {
        RunLoopSender::new(self.inner.platform_run_loop.new_sender())
    }

    /// 現在のスレッドがランループスレッドかどうかを返す
    pub fn is_run_loop_thread() -> bool {
        let thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();
        if let Some(run_loop_thread_id) = *thread_id {
            let current_id = thread::current().id();
            current_id == run_loop_thread_id
        } else {
            false
        }
    }

    /// 現在のスレッドをランループスレッドとして設定
    ///
    /// [deprecated]
    /// もしかしたら古い Flutter と統合する際に必要かもしれないので残してあるだけ。
    /// 基本的にこのメソッドではなく、RunLoop::init() によって RunLoopスレッドを指定するべき。
    ///
    /// irondash_engine_contextプラグインを使用しない場合、
    /// RunLoop::init()の後にランループスレッドで呼び出す。
    /// これによりFlutterプラグインなしでRunLoop::sender_for_run_loop_thread()が動作する。
    #[deprecated(note = "Use RunLoop::init() instead")]
    pub fn set_run_loop_thread() {
        {
            let mut thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();
            *thread_id = Some(thread::current().id());
        }

        // MainThreadFacilitatorを設定（Flutter pluginがない環境でも動作するように）
        use crate::main_thread::MainThreadFacilitator;
        MainThreadFacilitator::set_for_current_thread();
    }

    /// このランループをエグゼキュータとしてFutureをスポーンする。
    pub fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> JoinHandle<T> {
        // シャットダウンチェック
        if self.inner.has_shutdown.load(Ordering::SeqCst) {
            panic!("Cannot spawn task on shut down RunLoop");
        }

        let task = Arc::new(Task::new(self.new_sender(), future));

        // タスクを追跡リストに追加
        {
            let mut tasks = self.inner.active_tasks.lock().unwrap();
            tasks.push(Arc::downgrade(&(task.clone() as Arc<dyn AbortableTask>)));

            // デッドタスクを定期的にクリーンアップ
            if tasks.len() > 100 {
                tasks.retain(|weak| weak.upgrade().is_some());
            }
        }

        ArcWake::wake_by_ref(&task);
        JoinHandle::new(task)
    }

    /// 指定されたFutureが完了するまで現在のスレッドを同期的に待機する。
    ///
    /// このメソッドは待機中も RunLoop を駆動し続けるため、`spawn` で投入された他タスクも実行される。
    /// そのため、待機対象 Future が RunLoop 上の別タスク完了に依存していても進行できる。
    /// 一方で `pollster::block_on` など外部 executor は RunLoop を駆動しないため、
    /// 同じ依存関係ではデッドロックしうる。
    ///
    /// `spawn` は使わず、呼び出し元の future をその場で直接 poll する。
    /// そのため `RunLoop::spawn` と異なり、non-`'static` な借用を含む future も扱える。
    ///
    /// `flutter` feature 有効時、通常のポーリング (`run` 用) は必要に応じてプラットフォーム既定の
    /// イベントキューへフォールバックする場合があるが、`block_on` では常に RunLoop 固有ソースのみを処理する。
    ///
    /// ネストした `block_on` はパニックする。
    ///
    /// # 使用例
    ///
    /// ```no_run
    /// use novonotes_run_loop::RunLoop;
    ///
    /// RunLoop::init().unwrap();
    /// let result = RunLoop::current().block_on(async {
    ///     // 非同期処理
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

        // stop() は使わず、待機対象 future が wake されるたびに再 poll しつつ
        // RunLoop 固有ソースを回し続ける。
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

    /// 現在のスレッドのRunLoopを返す
    /// 必ずランループスレッドから呼び出すべき。
    /// それ以外のスレッドの場合はパニックする。
    pub fn current() -> Self {
        // ランループスレッドチェック
        let current_thread = thread::current().id();
        let thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();

        if let Some(run_loop_thread_id) = *thread_id {
            if current_thread != run_loop_thread_id {
                panic!("RunLoop::current() can only be called from the run loop thread");
            }
        } else {
            panic!("RunLoop not initialized. Call RunLoop::init() first");
        }

        // インスタンス取得
        let instance = RUN_LOOP_INSTANCE
            .get()
            .expect("RunLoop not initialized. Call RunLoop::init() first");

        // シャットダウンチェック
        if instance.has_shutdown.load(Ordering::SeqCst) {
            panic!("RunLoop has been shut down");
        }

        RunLoop {
            inner: instance.clone(),
        }
    }

    /// 現在のスレッドのRunLoopを取得する失敗可能なメソッド。
    ///
    /// 現在のスレッドで RunLoop が初期化されていない場合はエラーを返す。
    pub fn try_current() -> Result<Self> {
        // ランループスレッドチェック
        let current_thread = thread::current().id();
        let thread_id = RUN_LOOP_THREAD_ID.lock().unwrap();

        if let Some(run_loop_thread_id) = *thread_id {
            if current_thread != run_loop_thread_id {
                return Err(Error::NotInitialized);
            }
        } else {
            return Err(Error::NotInitialized);
        }

        // インスタンス取得
        let instance = RUN_LOOP_INSTANCE.get().ok_or(Error::NotInitialized)?;

        // シャットダウンチェック
        if instance.has_shutdown.load(Ordering::SeqCst) {
            return Err(Error::NotInitialized);
        }

        Ok(RunLoop {
            inner: instance.clone(),
        })
    }

    /// 停止されるまでランループを実行する。
    pub fn run(&self) {
        self.inner.platform_run_loop.run()
    }

    /// ランループを停止する。
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

/// 現在のスレッドのランループをエグゼキュータとしてFutureをスポーンする。
/// 事前にRunLoop::init()で初期化されている必要がある。
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
        // ランループが既に実行中のときにスレッドをスポーンすることを確認
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

            // コールバックが実行されるまで待機
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
        // 初回のinit
        RunLoop::init().unwrap();
        assert!(RunLoop::is_run_loop_thread());

        // deinitで状態をクリア
        RunLoop::deinit();

        // 別のスレッドで再度init可能
        let handle = thread::spawn(|| {
            RunLoop::init().unwrap();
            assert!(RunLoop::is_run_loop_thread());
            RunLoop::deinit();
        });
        handle.join().unwrap();

        // 元のスレッドでも再度init可能
        RunLoop::init().unwrap();
        assert!(RunLoop::is_run_loop_thread());
        RunLoop::deinit();
    }

    #[test]
    #[serial]
    fn test_deinit_aborts_all_tasks() {
        use std::sync::atomic::{AtomicBool, Ordering};

        // 初期化
        RunLoop::init().unwrap();

        // タスクが実行されたかを追跡
        let task1_started = Arc::new(AtomicBool::new(false));
        let task2_started = Arc::new(AtomicBool::new(false));

        let t1_started = task1_started.clone();
        let t2_started = task2_started.clone();

        // 長時間実行されるタスクをスポーン
        let handle1 = RunLoop::current().spawn(async move {
            t1_started.store(true, Ordering::SeqCst);
            // 無限ループでabort待ち
            loop {
                std::future::pending::<()>().await;
            }
        });

        let handle2 = RunLoop::current().spawn(async move {
            t2_started.store(true, Ordering::SeqCst);
            // 無限ループでabort待ち
            loop {
                std::future::pending::<()>().await;
            }
        });

        // RunLoopを少し実行してタスクを開始させる
        RunLoop::current()
            .schedule(Duration::from_millis(300), || {
                // タスクが開始されるのを待ってから停止
                RunLoop::current().stop();
            })
            .detach();
        RunLoop::current().run();

        // タスクが開始されていたことを確認
        assert!(task1_started.load(Ordering::SeqCst));
        assert!(task2_started.load(Ordering::SeqCst));

        // deinit()を呼ぶ - 全てのタスクがabortされるはず
        RunLoop::deinit();

        // pollster::block_on でタスクがアボートされたことを確認
        let result1 = pollster::block_on(handle1);
        let result2 = pollster::block_on(handle2);

        // 両方のタスクがAbortedエラーを返すはず
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

        // `block_on` 待機中も RunLoop が回り続け、別途 spawn されたタスクが進行できることを確認。
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
        // 前のテストの影響を排除するため ensure を使用
        RunLoop::ensure_run_loop_on_current_thread().unwrap();

        // ネストした block_on は未定義動作として debug_assert で検出される
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
        // 前のテストの影響を排除するため ensure を使用
        RunLoop::ensure_run_loop_on_current_thread().unwrap();

        // catch_unwind でパニックをキャッチして deinit を確実に呼ぶ
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

        // panic で `block_on` を抜けた後もネスト判定フラグが解除され、次回呼び出しが正常動作することを確認。
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

        // `&mut self` を借用した non-`'static` future を `block_on` に渡せることを確認。
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
