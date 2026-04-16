// Custom test harness — starting with a simple window creation test
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
    // Create a simple window
    let window_handle = create_window("Test Window", 400.0, 300.0);

    // Show the window
    window_handle.show();

    // Wait a moment
    let mut handle = RunLoop::current().schedule(Duration::from_secs(1), move || {
        println!("Window test completed");
        RunLoop::current().stop_app();
    });
    handle.detach();

    // In a desktop environment, use run_app
    RunLoop::current().run_app();

    // Destroy the window
    window_handle.destroy();
}
