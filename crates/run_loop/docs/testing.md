# run_loop Testing Guide

There are two kinds of test infrastructure for run_loop: the standard async test helper and the GUI test harness.

## test_helper (for ordinary async tests)

Use this when testing async code. It automatically handles RunLoop initialization, execution, and teardown.

### Usage

```rust
use novonotes_run_loop::test_helper as test;
use serial_test::serial;

#[test]
#[serial]  // Required for serialization when running multiple tests.
fn test_example() {
    test::run_async(async {
        // Write async test code here
        RunLoop::current().delay(Duration::from_millis(10)).await;
        42  // Any type can be returned
    });
}
```

- **Always attach `#[serial]`**: RunLoop does not support concurrent execution by multiple threads.
- **Return type is flexible**: You can return `Result<T, E>` or any other type.
- **Panics are handled**: Panics inside the test are caught and reported as test failures.

## test_harness (for GUI integration tests)

Tests that require GUI operations on macOS/iOS must run on the main thread. In those cases, disable the standard test harness and use the dedicated one instead.

### Setup

The standard Rust harness does not run tests on the main thread, so it must be disabled.

```toml
# Cargo.toml
[[test]]
name = "gui_test"
path = "tests/gui_test.rs"
harness = false  # Disable the standard harness
```

### Usage

```rust
use novonotes_run_loop::test_harness::run_gui_tests;

fn main() {
    run_gui_tests(vec![
        ("test_name", test_function),
        // Add more tests as needed
    ]);
}

fn test_function() -> Result<(), String> {
    RunLoop::current().schedule(Duration::ZERO, move || {
        // Some GUI test code
        assert_eq!(1 + 1, 2);

        RunLoop::current().stop_app();
    })
    .detach();

    // run_app blocks until stop_app is called.
    RunLoop::current().run_app();
    Ok(())
}
```
