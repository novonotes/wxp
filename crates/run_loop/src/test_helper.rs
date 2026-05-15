use crate::RunLoop;
use std::sync::{Arc, Mutex};

/// Helper function for running RunLoop-based async tests.
/// Tests that depend on RunLoop must be serialized (e.g., with `#[serial]` from serial_test)
/// because RunLoop does not support designating multiple threads as the run loop thread
/// simultaneously.
///
/// # Example
///
/// ```ignore
/// #[test]
/// #[serial]
/// fn test_something() {
///     run_loop::test_helper::run_async(async {
///         // Run async work on the run loop
///         Ok::<(), String>(())
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

    // Spawn the async function
    let handle = run_loop.spawn(test_fn);

    // Wait for completion, then stop the RunLoop
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

    // Extract and return the result
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
