use novonotes_run_loop::test_helper as test;
use serial_test::serial;

// Example tests using the helper function
#[test]
#[serial]
fn test_success() -> Result<(), String> {
    test::run_async(async {
        // A test that succeeds
        Ok(())
    })
}

#[test]
#[serial]
fn test_async_wait() -> Result<(), String> {
    test::run_async(async { Ok(()) })
}

#[test]
#[serial]
fn test_error_propagation() {
    let result: Result<(), String> = test::run_async(async {
        // Verify that errors propagate correctly
        Err("Expected error".to_string())
    });
    assert!(result.is_err());
}

// This test intentionally fails and is therefore ignored.
#[ignore]
#[test]
#[serial]
#[should_panic(expected = "test panic")]
fn test_panic_handling() {
    test::run_async(async {
        panic!("test panic");
    });
}

#[test]
#[serial]
fn test_generic_return_value() {
    let result: i32 = test::run_async(async { 42 });
    assert_eq!(result, 42);
}

#[test]
#[serial]
fn test_tuple_return() {
    let (a, b): (String, i32) = test::run_async(async { ("hello".to_string(), 123) });
    assert_eq!(a, "hello");
    assert_eq!(b, 123);
}
