use crate::RunLoop;
use std::sync::{Arc, Mutex};

/// RunLoopベースの非同期テストを実行するためのヘルパー関数
/// RunLoop に依存するテストは、serial_test の #[serial] などで、直列実行する必要があります。
/// RunLoop は同時に複数のスレッドを RunLoop スレッドとして設定することをサポートしていないためです。
///
/// # 使用例
///
/// ```
/// #[test]
/// #[serial]
/// fn test_something() -> Result<(), String> {
///     run_loop::test_helper::run_async(async {
///         RunLoop::current().wait(Duration::from_millis(10)).await;
///         Ok(())
///     });
/// }
/// ```
pub fn run_async<F, T>(test_fn: F) -> T
where
    F: std::future::Future<Output = T> + 'static,
    T: Send + 'static,
{
    RunLoop::init().unwrap();
    let run_loop = RunLoop::current();
    let result = Arc::new(Mutex::new(None));
    let result_clone = result.clone();

    // 非同期関数を実行
    let handle = run_loop.spawn(test_fn);

    // 完了を待ってRunLoopを停止
    run_loop.spawn(async move {
        match handle.await {
            Ok(test_result) => {
                *result_clone.lock().unwrap() = Some(Ok(test_result));
                RunLoop::current().stop();
            }
            Err(crate::JoinError::Panic(msg)) => {
                *result_clone.lock().unwrap() = Some(Err(format!("Task panicked: {:?}", msg)));
                RunLoop::current().stop();
            }
            Err(e) => {
                *result_clone.lock().unwrap() = Some(Err(format!("Unexpected error: {:?}", e)));
                RunLoop::current().stop();
            }
        }
    });

    run_loop.run();

    RunLoop::deinit();

    // 結果を取り出して返す
    let result = Arc::try_unwrap(result)
        .map_err(|_| "Failed to unwrap Arc")
        .unwrap()
        .into_inner()
        .map_err(|_| "Failed to unwrap Mutex")
        .unwrap();

    match result {
        Some(Ok(value)) => value,
        Some(Err(msg)) => panic!("{}", msg),
        None => panic!("Task did not complete"),
    }
}
