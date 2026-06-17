# run_loop_test_utils

Test-only helpers for code that uses `novonotes_run_loop`.

## run_async

Use this when testing async code. It handles RunLoop initialization, execution, and teardown.

```rust
use run_loop_test_utils::run_async;
use serial_test::serial;

#[test]
#[serial]
fn test_example() {
    run_async(async {
        42
    });
}
```

Tests using `run_async` should be serialized because `novonotes_run_loop` supports one active run loop per process.

## run_gui_tests

Use this for GUI integration tests that must run on the run loop thread. Test binaries that call this usually disable the standard Rust harness.

```toml
[[test]]
name = "gui_test"
path = "tests/gui_test.rs"
harness = false
```

```rust
use novonotes_run_loop::RunLoopLocal;
use run_loop_test_utils::run_gui_tests;

fn main() {
    run_gui_tests(vec![("test_name", test_function)]);
}

fn test_function(run_loop: &RunLoopLocal) -> Result<(), String> {
    run_loop.schedule(std::time::Duration::ZERO, move |run_loop| {
        run_loop.stop_app();
    })
    .detach();

    run_loop.run_app();
    Ok(())
}
```
