use novonotes_run_loop::{Error, RunLoop};
use serial_test::serial;
use std::thread;

#[test]
#[serial]
fn failed_init_on_non_run_loop_thread_does_not_acquire_reference() {
    let guard = RunLoop::init().unwrap();

    let failed_on_background_thread =
        thread::spawn(|| matches!(RunLoop::init(), Err(Error::NotRunLoopThread)))
            .join()
            .unwrap();
    assert!(failed_on_background_thread);

    drop(guard);

    thread::spawn(|| {
        let _guard = RunLoop::init().unwrap();
        assert!(RunLoop::is_run_loop_thread());
    })
    .join()
    .unwrap();
}

#[test]
#[serial]
fn run_loop_guard_releases_exactly_one_successful_acquisition() {
    let first = RunLoop::init().unwrap();
    let second = RunLoop::init().unwrap();

    drop(second);
    assert!(RunLoop::is_initialized());

    drop(first);
    assert!(!RunLoop::is_initialized());
}
