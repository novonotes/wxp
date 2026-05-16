# run_loop_timer

A small timer crate for running repeating callbacks on `novonotes_run_loop`.

Callbacks run on the run loop thread and do not require `Send`, making it easy to periodically
update objects that can only be accessed from a specific thread, such as native GUI widgets or
WebView channels.

```rust
use run_loop_timer::Timer;
use std::time::Duration;

let timer = Timer::new(Duration::from_millis(100), || {
    // Runs on the run loop thread
});
timer.start();
timer.stop();
```

Async callbacks are also supported.

```rust
let timer = Timer::new_async(Duration::from_millis(100), || async {
    // Spawned via RunLoop::current().spawn(...)
});
timer.start();
```

## Prerequisites

- Create, start, stop, and drop the timer on a thread where `RunLoop::init()` has been called.
- When dropped, the next scheduled tick is cancelled.
- An async task that has already been spawned continues running even after the timer is stopped.
- Timing precision depends on the platform and run loop load.
