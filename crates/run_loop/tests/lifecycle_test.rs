use novonotes_run_loop::{Error, RunLoop};
use serial_test::serial;
use std::thread;

#[test]
#[serial]
fn failed_init_on_non_run_loop_thread_does_not_acquire_reference() {
    RunLoop::init().unwrap();

    let result = thread::spawn(RunLoop::init).join().unwrap();
    assert!(matches!(result, Err(Error::NotRunLoopThread)));

    RunLoop::deinit();

    thread::spawn(|| {
        RunLoop::init().unwrap();
        assert!(RunLoop::is_run_loop_thread());
        RunLoop::deinit();
    })
    .join()
    .unwrap();
}

#[test]
#[serial]
fn run_loop_guard_releases_exactly_one_successful_acquisition() {
    let first = RunLoop::acquire_on_current_thread().unwrap();
    let second = RunLoop::acquire_on_current_thread().unwrap();

    drop(second);
    assert!(RunLoop::try_current().is_ok());

    drop(first);
    assert!(RunLoop::try_current().is_err());
}
