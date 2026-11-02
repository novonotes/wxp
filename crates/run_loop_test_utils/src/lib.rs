//! Test utilities for code that depends on `novonotes_run_loop`.

use std::sync::{Arc, Mutex};

use log::{error, info};
use novonotes_run_loop::{JoinError, RunLoop, RunLoopLocal};

/// Runs GUI tests sequentially on a single initialized run loop.
///
/// Use this for integration tests that must run on the run loop thread and
/// cannot use Rust's standard test harness scheduling directly. Test binaries
/// that call this usually set `harness = false`.
pub fn run_gui_tests<F>(tests: Vec<(&str, F)>)
where
    F: FnOnce(&RunLoopLocal) -> Result<(), String>,
{
    info!("Running GUI tests on run loop thread...");

    let guard = match RunLoop::init() {
        Ok(guard) => guard,
        Err(error) => {
            error!("Failed to initialize RunLoop: {error:?}");
            std::process::exit(1);
        }
    };

    let mut failed = false;

    for (name, test_fn) in tests {
        print!("Testing {name}... ");
        match test_fn(guard.local()) {
            Ok(()) => println!("ok"),
            Err(error) => {
                println!("failed");
                error!("Error: {error}");
                failed = true;
            }
        }
    }

    if failed {
        error!("Some tests failed");
        std::process::exit(1);
    }
}

/// Runs an async test body on a temporary run loop.
///
/// Tests using this helper should be serialized because `novonotes_run_loop`
/// allows only one active run loop per process.
pub fn run_async<F, T>(test_fn: F) -> T
where
    F: std::future::Future<Output = T> + 'static,
    T: Send + 'static,
{
    let guard = RunLoop::init().unwrap();
    let run_loop = guard.local();
    let result = Arc::new(Mutex::new(None));
    let result_clone = result.clone();

    let handle = run_loop.spawn(test_fn);

    run_loop.spawn(async move {
        let result = match handle.await {
            Ok(test_result) => Ok(test_result),
            Err(JoinError::Panic(payload)) => Err(format!("Task panicked: {payload:?}")),
            Err(error) => Err(format!("Unexpected task error: {error:?}")),
        };
        *result_clone.lock().unwrap() = Some(result);
        RunLoop::call(|local| local.stop()).unwrap();
    });

    run_loop.run();

    let result = Arc::try_unwrap(result)
        .map_err(|_| "Failed to unwrap Arc")
        .unwrap()
        .into_inner()
        .map_err(|_| "Failed to unwrap Mutex")
        .unwrap();

    match result {
        Some(Ok(value)) => value,
        Some(Err(message)) => panic!("{message}"),
        None => panic!("Task did not complete"),
    }
}
