use novonotes_run_loop::{JoinError, RunLoop, spawn};
use serial_test::serial;
use std::time::Duration;

#[test]
#[serial]
fn test_task_normal_completion() {
    RunLoop::init().unwrap();
    use std::sync::{Arc, Mutex};

    let result = Arc::new(Mutex::new(None));
    let result_clone = result.clone();

    // Launch a task using the spawn function
    spawn(async move {
        *result_clone.lock().unwrap() = Some(42);
    });

    // Run the RunLoop to process the task
    let run_loop = RunLoop::current();
    let mut handle = run_loop.schedule(Duration::from_millis(50), move || {
        RunLoop::current().stop();
    });
    handle.detach();

    run_loop.run();

    // Verify the result
    let res = result.lock().unwrap().take();
    assert_eq!(res, Some(42));

    RunLoop::deinit();
}

#[test]
#[serial]
fn test_task_abort() {
    RunLoop::init().unwrap();
    use futures::Future;
    use std::task::{Context, Poll};

    let run_loop = RunLoop::current();

    let handle = run_loop.spawn(async {
        // Long-running task
        RunLoop::current().delay(Duration::from_secs(10)).await;
        42
    });

    // Abort immediately
    handle.abort();

    // Confirm the task was aborted
    let mut handle = Box::pin(handle);
    let waker = futures::task::noop_waker();
    let mut cx = Context::from_waker(&waker);

    match handle.as_mut().poll(&mut cx) {
        Poll::Ready(result) => {
            assert!(result.is_err());
            assert!(result.unwrap_err().is_aborted());
        }
        Poll::Pending => panic!("An aborted task should complete immediately"),
    }

    RunLoop::deinit();
}

#[test]
#[serial]
fn test_task_panic() {
    RunLoop::init().unwrap();
    use std::sync::{Arc, Mutex};

    let run_loop = RunLoop::current();
    let captured_panic = Arc::new(Mutex::new(None));
    let captured_panic_clone = captured_panic.clone();

    let handle = run_loop.spawn(async {
        panic!("panic inside task");
    });

    // Spawn another task to verify the panic result
    run_loop.spawn(async move {
        let result = handle.await;
        *captured_panic_clone.lock().unwrap() = Some(result);
        RunLoop::current().stop();
    });

    run_loop.run();

    // Verify the result
    let result = captured_panic.lock().unwrap().take().unwrap();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_panic());

    // Verify the panic message
    if let JoinError::Panic(payload) = err {
        if let Some(msg) = payload.downcast_ref::<&str>() {
            assert_eq!(*msg, "panic inside task");
        }
    }

    RunLoop::deinit();
}

#[test]
#[serial]
fn test_multiple_tasks_mixed_results() {
    RunLoop::init().unwrap();
    use std::sync::{Arc, Mutex};

    let run_loop = RunLoop::current();
    let results = Arc::new(Mutex::new(vec![]));

    // Task that completes normally
    let handle1 = run_loop.spawn(async { 1 });

    // Task that gets aborted
    let handle2 = run_loop.spawn(async {
        RunLoop::current().delay(Duration::from_secs(10)).await;
        2
    });
    handle2.abort();

    // Task that panics
    let handle3 = run_loop.spawn(async {
        panic!("intentional panic");
    });

    // Task that collects all results
    let results_clone = results.clone();
    run_loop.spawn(async move {
        let r1 = handle1.await;
        let r2 = handle2.await;
        let r3 = handle3.await;

        let mut res = results_clone.lock().unwrap();
        res.push(("handle1", r1));
        res.push(("handle2", r2));
        res.push(("handle3", r3));

        RunLoop::current().stop();
    });

    run_loop.run();

    // Verify all results
    let res = results.lock().unwrap();
    assert_eq!(res.len(), 3);

    // handle1: completed normally
    assert!(res[0].1.is_ok());
    assert_eq!(res[0].1.as_ref().unwrap(), &1);

    // handle2: cancelled
    assert!(res[1].1.is_err());
    assert!(res[1].1.as_ref().unwrap_err().is_aborted());

    // handle3: panicked
    assert!(res[2].1.is_err());
    assert!(res[2].1.as_ref().unwrap_err().is_panic());

    RunLoop::deinit();
}
