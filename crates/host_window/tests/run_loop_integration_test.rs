// Uses a custom `main` (harness = false) instead of `#[test]`: native windows
// and the run loop must live on the process main thread, but Rust's default
// test harness runs each test on a spawned thread, which AppKit/X11/Win32
// reject. This also verifies host_window and the run loop cooperate end to end.
use host_window::create_window;
use log::error;
use novonotes_run_loop::{RunLoop, RunLoopLocal};
use std::time::Duration;

fn main() {
    println!("Running wxp GUI tests on main thread...");

    let guard = RunLoop::init().unwrap();

    // Run tests
    let mut failed = false;

    // Isolate the test so a panic is reported as a failure and still lets the
    // run loop be torn down cleanly below, rather than aborting the process.
    print!("Testing window creation... ");
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        test_simple_window(guard.local())
    })) {
        Ok(_) => println!("✓"),
        Err(e) => {
            println!("✗");
            error!("Error: {:?}", e);
            failed = true;
        }
    }

    if failed {
        error!("\nSome tests failed!");
        std::process::exit(1);
    } else {
        println!("\nAll tests passed!");
    }
}

fn test_simple_window(run_loop: &RunLoopLocal) {
    let window_handle = create_window("Test Window", 400.0, 300.0);
    window_handle.show();

    // Run the loop briefly so the window actually reaches the screen, then stop
    // it from within so the test terminates instead of blocking forever.
    // `detach` lets the scheduled task outlive its handle.
    let mut handle = run_loop.schedule(Duration::from_secs(1), move |run_loop| {
        println!("Window test completed");
        run_loop.stop_app();
    });
    handle.detach();

    run_loop.run_app();

    window_handle.destroy();
}
