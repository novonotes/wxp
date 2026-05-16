# novonotes_run_loop

A platform-independent run loop interface for Rust.
A fork of [irondash_run_loop](https://github.com/irondash/irondash/tree/main/run_loop) with enhanced safety and error handling for DLL environments.

> 日本語: [README_JA.md](./README_JA.md)

## Purpose

Designed for async task management in audio application/plugin development.
Provides access to the host application's main thread run loop without blocking it,
enabling plugins to schedule and launch tasks on the main thread.

## Features

- **Multi-platform**: Unified API over native run loops on iOS/macOS, Android, Linux, and Windows
- **Async task management**: Supports Rust's standard `async`/`await` patterns
- **Cross-thread communication**: Safe message-passing mechanism
- **DLL / audio plugin support**: Handles use cases where the library runs inside a DLL linked into a third-party application

## Basic Usage

### Initialization

```rust
use novonotes_run_loop::{RunLoop, JoinError};

// Call during application/DLL initialization
RunLoop::init().expect("Failed to initialize RunLoop");

// Get the RunLoop for the current thread
let run_loop = RunLoop::current();

// Call during application/DLL teardown
RunLoop::deinit();
```

### Scheduling Tasks

```rust
use std::time::Duration;

let run_loop = RunLoop::current();

// Schedule execution after 10 seconds
let handle = run_loop.schedule(Duration::from_secs(10), || {
    println!("10 seconds have passed");
});

// Dropping the handle cancels the timer.
// Use detach() to prevent cancellation.
handle.detach();
```

### Running Async Tasks

```rust
// Spawn a task and await its result
let handle = run_loop.spawn(async {
    // Async work
    RunLoop::current().delay(Duration::from_secs(1)).await;
    42
});

// Retrieve the result with error handling
match handle.await {
    Ok(value) => println!("Result: {}", value),  // Should print `Result: 42`
    Err(JoinError::Aborted) => println!("Task was aborted"),
    Err(JoinError::Panic(_)) => println!("Task panicked"),
}
```

### Cross-Thread Communication

`RunLoop::init()` marks the current thread as the run loop thread.
Use `RunLoop::sender()` to send callbacks from other threads to the run loop thread.

```rust
use std::thread;

fn main() {
    assert!(RunLoop::is_run_loop_thread());

    // Send a callback from another thread to the run loop thread
    thread::spawn(move || {
        let sender = RunLoop::sender();
        // Sent callbacks are executed asynchronously on the run loop thread.
        sender.send(|| {
            assert!(RunLoop::is_run_loop_thread());
            println!("Executing on run loop thread");
        });
    });
}
```


## Platform Implementations

| Platform  | Underlying Technology | Notes                                                    |
| --------- | --------------------- | -------------------------------------------------------- |
| iOS/macOS | CFRunLoop             | Core Foundation based, uses a custom RunLoopMode         |
| Android   | ALooper               | NDK ALooper, timer implemented with timerfd              |
| Linux     | GMainContext          | GLib/GTK integration, timer via g_timeout_source         |
| Windows   | Win32 Message Loop    | Message processing via a hidden window                   |

## Execution Model

`init()` acquires a reference to the native loop infrastructure on the current thread (creating one if none exists). In standalone applications, you must call `run()` yourself to drive the loop. In plugin environments the host already drives the loop, so `run()` is unnecessary. Callbacks and timer registrations behave identically regardless of who drives the loop.

| Pattern               | `run()` | Who drives the loop       |
|-----------------------|---------|---------------------------|
| Standalone app        | call    | `run()` itself            |
| Plugin (CLAP/VST3 …)  | skip    | Host (DAW) existing loop  |

## Differences from irondash_run_loop

1. **DLL safety**: Removed thread-local storage. Revised initialization/teardown. Avoids name collisions for Win32 Window Class names and CFRunLoop RunLoopMode names across multiple DLLs.
2. **Explicit `abort()` method**: Allows controlled task cancellation.
3. **Panic recovery**: Catches and reports panics that occur inside tasks.

## Testing

run_loop provides helpers and a test harness for testing code that uses it.
For usage details, see the [Testing Guide](docs/testing.md).

## Project Status

The current status is **alpha**.
NovoNotes uses it in production, but the public API is still stabilizing and may include breaking changes.

## Installation

- The Rust crate is not published to crates.io. Use it with a pinned `git` + `rev`.

## License

MIT License (same as the original project)

## Upstream

This repository is a fork of the `run_loop` crate from [`irondash`](https://github.com/irondash/irondash).
