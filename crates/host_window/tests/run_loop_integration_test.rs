// Uses a custom `main` (harness = false) instead of `#[test]`: native windows
// and the run loop must live on the process main thread, but Rust's default
// test harness runs each test on a spawned thread, which AppKit/X11/Win32
// reject. This also verifies host_window and the run loop cooperate end to end.
use host_window::create_window;
use log::error;
use novonotes_run_loop::RunLoop;
use std::time::Duration;

fn main() {
    println!("Running wxp GUI tests on main thread...");

    // Initialize RunLoop
    RunLoop::init().unwrap();

    // Run tests
    let mut failed = false;

    // Isolate the test so a panic is reported as a failure and still lets the
    // run loop be torn down cleanly below, rather than aborting the process.
    print!("Testing window creation... ");
    match std::panic::catch_unwind(|| test_simple_window()) {
        Ok(_) => println!("✓"),
        Err(e) => {
            println!("✗");
            error!("Error: {:?}", e);
            failed = true;
        }
    }

    // Clean up RunLoop
    RunLoop::deinit();

    if failed {
        error!("\nSome tests failed!");
        std::process::exit(1);
    } else {
        println!("\nAll tests passed!");
    }
}

fn test_simple_window() {
    let window_handle = create_window("Test Window", 400.0, 300.0);
    window_handle.show();

    // Run the loop briefly so the window actually reaches the screen, then stop
    // it from within so the test terminates instead of blocking forever.
    // `detach` lets the scheduled task outlive its handle.
    let mut handle = RunLoop::current().schedule(Duration::from_secs(1), move || {
        println!("Window test completed");
        RunLoop::current().stop_app();
    });
    handle.detach();

    RunLoop::current().run_app();

    window_handle.destroy();
}
