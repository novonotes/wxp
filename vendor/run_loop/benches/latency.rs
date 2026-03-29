use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use novonotes_run_loop::{RunLoop, RunLoopSender};
use std::sync::{Arc, Barrier, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

fn bench_schedule_latency(c: &mut Criterion) {
    RunLoop::init().unwrap();
    let run_loop = RunLoop::current();

    c.bench_function("schedule_latency", |b| {
        b.iter(|| {
            let latency = Arc::new(Mutex::new(None));
            let latency_clone = latency.clone();
            let start = Instant::now();

            let mut handle = run_loop.schedule(Duration::from_millis(0), move || {
                *latency_clone.lock().unwrap() = Some(start.elapsed());
                RunLoop::current().stop();
            });
            handle.detach();

            run_loop.run();
            let result = latency.lock().unwrap().take().unwrap();
            black_box(result);
        });
    });

    RunLoop::deinit();
}

fn bench_spawn_latency(c: &mut Criterion) {
    RunLoop::init().unwrap();
    let run_loop = RunLoop::current();

    c.bench_function("spawn_latency", |b| {
        b.iter(|| {
            let latency = Arc::new(Mutex::new(None));
            let latency_clone = latency.clone();
            let start = Instant::now();

            run_loop.spawn(async move {
                *latency_clone.lock().unwrap() = Some(start.elapsed());
                RunLoop::current().stop();
            });

            run_loop.run();
            let result = latency.lock().unwrap().take().unwrap();
            black_box(result);
        });
    });

    RunLoop::deinit();
}

fn bench_sender_same_thread_latency(c: &mut Criterion) {
    RunLoop::init().unwrap();
    let run_loop = RunLoop::current();
    let sender = RunLoop::sender();

    c.bench_function("sender_same_thread_latency", |b| {
        b.iter(|| {
            let latency = Arc::new(Mutex::new(None));
            let latency_clone = latency.clone();
            let start = Instant::now();

            sender.send(move || {
                *latency_clone.lock().unwrap() = Some(start.elapsed());
                RunLoop::current().stop();
            });

            run_loop.run();
            let result = latency.lock().unwrap().take().unwrap();
            black_box(result);
        });
    });

    RunLoop::deinit();
}

fn bench_sender_cross_thread_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("sender_cross_thread_latency");

    for thread_count in &[1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_threads", thread_count)),
            thread_count,
            |b, &thread_count| {
                // メインスレッドのrunloop初期化
                RunLoop::init().unwrap();
                let main_run_loop = RunLoop::current();

                // 各スレッドからの送信を測定
                b.iter(|| {
                    let barrier = Arc::new(Barrier::new(thread_count + 1));
                    let completed = Arc::new(Mutex::new(0));
                    let latencies = Arc::new(Mutex::new(vec![]));

                    let mut handles = vec![];

                    for _ in 0..thread_count {
                        let barrier = barrier.clone();
                        let completed = completed.clone();
                        let latencies = latencies.clone();

                        let handle = thread::spawn(move || {
                            barrier.wait();
                            let start = Instant::now();
                            let sender = RunLoop::sender();

                            sender.send(move || {
                                let latency = start.elapsed();
                                latencies.lock().unwrap().push(latency);
                                let mut count = completed.lock().unwrap();
                                *count += 1;
                                if *count == thread_count {
                                    RunLoop::current().stop();
                                }
                            });
                        });

                        handles.push(handle);
                    }

                    // 全スレッドを同時にスタート
                    barrier.wait();

                    // メインスレッドでイベントを処理
                    main_run_loop.run();

                    for handle in handles {
                        handle.join().unwrap();
                    }

                    let result = latencies.lock().unwrap().clone();
                    black_box(result);
                });

                RunLoop::deinit();
            },
        );
    }

    group.finish();
}

fn bench_sender_send_and_wait_latency(c: &mut Criterion) {
    // 別スレッドでRunLoopを実行
    let (ready_tx, ready_rx) = mpsc::channel();
    let (sender_tx, sender_rx) = mpsc::channel::<RunLoopSender>();
    let running = Arc::new(Mutex::new(true));
    let running_clone = running.clone();

    let worker = thread::spawn(move || {
        RunLoop::init().unwrap();
        let run_loop = RunLoop::current();
        let sender = RunLoop::sender();
        sender_tx.send(sender).unwrap();

        // 定期的に停止フラグをチェック
        let running_check = running_clone.clone();
        run_loop.spawn(async move {
            ready_tx.send(()).unwrap();
            while *running_check.lock().unwrap() {
                RunLoop::current().delay(Duration::from_millis(1)).await;
            }
            RunLoop::current().stop();
        });

        run_loop.run();
        RunLoop::deinit();
    });

    let sender = sender_rx.recv().unwrap();
    ready_rx.recv().unwrap();

    c.bench_function("sender_send_and_wait_latency", |b| {
        b.iter(|| {
            let start = Instant::now();
            let latency = Arc::new(Mutex::new(Duration::default()));
            let latency_clone = latency.clone();

            sender.send_and_wait(move || {
                *latency_clone.lock().unwrap() = start.elapsed();
            });

            let result = *latency.lock().unwrap();
            black_box(result);
        });
    });

    *running.lock().unwrap() = false;
    worker.join().unwrap();
}

fn bench_spawn_multiple_tasks(c: &mut Criterion) {
    let mut group = c.benchmark_group("spawn_multiple_tasks");

    for task_count in &[10, 50, 100, 500] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_tasks", task_count)),
            task_count,
            |b, &task_count| {
                RunLoop::init().unwrap();
                let run_loop = RunLoop::current();

                b.iter(|| {
                    let latency = Arc::new(Mutex::new(None));
                    let completed = Arc::new(Mutex::new(0));
                    let start = Instant::now();

                    for _i in 0..task_count {
                        let latency = latency.clone();
                        let completed = completed.clone();
                        run_loop.spawn(async move {
                            let mut count = completed.lock().unwrap();
                            *count += 1;
                            if *count == task_count {
                                *latency.lock().unwrap() = Some(start.elapsed());
                                RunLoop::current().stop();
                            }
                        });
                    }

                    run_loop.run();
                    let result = latency.lock().unwrap().take().unwrap();
                    black_box(result);
                });

                RunLoop::deinit();
            },
        );
    }

    group.finish();
}

fn bench_schedule_with_delay(c: &mut Criterion) {
    let mut group = c.benchmark_group("schedule_with_delay");

    for delay_ms in &[0, 1, 5, 10, 50] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}ms", delay_ms)),
            delay_ms,
            |b, &delay_ms| {
                RunLoop::init().unwrap();
                let run_loop = RunLoop::current();

                b.iter(|| {
                    let latency = Arc::new(Mutex::new(None));
                    let latency_clone = latency.clone();
                    let expected_delay = Duration::from_millis(delay_ms);
                    let start = Instant::now();

                    let mut handle = run_loop.schedule(expected_delay, move || {
                        *latency_clone.lock().unwrap() = Some(start.elapsed());
                        RunLoop::current().stop();
                    });
                    handle.detach();

                    // タイムアウト用のタイマーも設定
                    let mut timeout_handle =
                        run_loop.schedule(Duration::from_millis(delay_ms + 100), || {
                            RunLoop::current().stop();
                        });
                    timeout_handle.detach();

                    run_loop.run();

                    let result = latency
                        .lock()
                        .unwrap()
                        .take()
                        .unwrap_or_else(|| Duration::from_millis(delay_ms + 100));
                    black_box(result);
                });

                RunLoop::deinit();
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_schedule_latency,
    bench_spawn_latency,
    bench_sender_same_thread_latency,
    bench_sender_cross_thread_latency,
    bench_sender_send_and_wait_latency,
    bench_spawn_multiple_tasks,
    bench_schedule_with_delay,
);

criterion_main!(benches);
