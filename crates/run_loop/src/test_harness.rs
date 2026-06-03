// Custom test harness — helper for GUI integration tests
use crate::{RunLoop, RunLoopLocal};
use log::{error, info};

/// Harness that runs multiple GUI tests sequentially.
/// Some GUI-related operations must run on the main thread and are not
/// straightforward to test with the standard harness. Use this harness for those cases.
///
/// # Example
/// ```ignore
/// run_gui_tests(vec![
///     ("test1", test_function1),
///     ("test2", test_function2),
/// ]);
/// ```
/// When using this harness, the standard harness must be disabled.
///
/// Example Cargo.toml entry:
/// ```ignore
/// [[test]]
/// name = "wxp_webview_test"
/// path = "tests/wxp_webview_test.rs"
/// harness = false
/// ```
///
pub fn run_gui_tests<F>(tests: Vec<(&str, F)>)
where
    F: FnOnce(&RunLoopLocal) -> Result<(), String>,
{
    info!("Running GUI tests on main thread...");

    // Initialize RunLoop
    let guard = match RunLoop::init() {
        Ok(guard) => guard,
        Err(e) => {
            error!("Failed to initialize RunLoop: {:?}", e);
            std::process::exit(1);
        }
    };

    let mut failed = false;

    for (name, test_fn) in tests {
        print!("Testing {}... ", name);
        match test_fn(guard.local()) {
            Ok(_) => println!("✓"),
            Err(e) => {
                println!("✗");
                error!("Error: {}", e);
                failed = true;
            }
        }
    }

    if failed {
        error!("Some tests failed!");
        std::process::exit(1);
    } else {
        info!("All tests passed!");
    }
}
