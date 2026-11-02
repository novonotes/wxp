use serial_test::serial;

#[test]
#[serial]
fn run_async_returns_future_output() {
    let value = run_loop_test_utils::run_async(async { 42 });
    assert_eq!(value, 42);
}
