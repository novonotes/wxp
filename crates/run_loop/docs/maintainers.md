# run_loop Maintainer & Design Notes

This document is intended for **maintainers and porters**. For user-facing documentation, see
[README.md](../README.md) and the [lib.rs crate docs](../src/lib.rs).

> Japanese version: [maintainers_JA.md](./maintainers_JA.md)

---

## Relationship with upstream

This crate is forked from [irondash_run_loop](https://github.com/irondash/irondash/tree/main/run_loop),
but **ongoing sync with upstream is intentionally not performed**.

irondash targets multi-platform Flutter plugin development, whereas this crate is evolved
independently to optimize for audio plugin (CLAP/VST3) specific requirements such as DLL safety
and preventing panic propagation into the DAW host.

---

## Key differences from irondash

| Change | Reason |
|---|---|
| Removed thread-local storage; replaced with a global singleton | Avoids TLS destructor issues on DLL unload (see below) |
| Reference-counted `init()` / `deinit()` | Tolerates repeated initialization calls from plugin integration code (see below) |
| Unique Win32 Window Class name and CFRunLoop RunLoopMode name | Prevents name collisions when multiple DLLs are loaded in the same process |
| Added `abort()` method | Enables controlled task cancellation |
| Panic inside a task is caught with `catch_unwind` | Prevents taking down the DAW host (see below) |
| Added `block_on()` | Allows synchronously awaiting a Future on the CLAP GUI thread (see below) |

---

## Singleton constraint

There is a hard constraint that **only one run loop thread may exist in the process at any time**.

`CFRunLoop` on Darwin and `GMainContext` on Linux are APIs that assume a thread-local concept of
"the current thread's run loop". It would be possible to design a system where multiple threads each
have their own run loop, but for audio plugins the mapping of "GUI thread = run loop thread" is
sufficient, and adding that complexity brings no benefit.

In test environments any thread can be designated the run loop thread, so always serialize tests
with `#[serial_test::serial]`.

---

## `init` / `deinit` design

A reference-counting scheme (`INIT_COUNT: AtomicUsize`) is used.

`RunLoop::init()` is not the same lifecycle hook as CLAP `clap_entry.init`. CLAP entry
initialization is DSO initialization, should be fast, and may be called from a scanning or worker
thread. `RunLoop::init()` instead pins the current thread as the run loop thread, so plugin
integrations should call it from the host main/UI thread that will receive GUI callbacks.

The reference count exists because plugin integration code may still attempt repeated setup/teardown
around GUI lifecycles or host reloads. Actual initialization runs when `INIT_COUNT` transitions
from 0→1; cleanup runs when it transitions from 1→0.

Misuse patterns:
- Calling `deinit()` more times than `init()` → The count underflows, risking a reference to a
  cleaned-up instance on the next `init()` (hard to detect due to `fetch_sub` wrap-around).
- DLL unloaded without calling `deinit()` → `RunLoopInner::drop` runs a best-effort fallback, but
  it is best-effort only. It is critical that the shutdown path does not panic.

---

## Why TLS is avoided

Thread-local storage makes it difficult to control the order and timing of destructors on DLL
unload. On Windows in particular, the ordering of `DLL_THREAD_DETACH` / `DLL_PROCESS_DETACH` is
host-dependent, and destructors that access other TLS values are a known source of crashes.

Additionally, in scenarios where the same thread is shared between a different DLL or the host
code, values held in TLS may end up referencing code or data from the already-unloaded DLL.

For these reasons `RUN_LOOP_INSTANCE` and `RUN_LOOP_THREAD_ID` are held as `static` global
variables (guarded by `Mutex`).

---

## Intent of `block_on`

External executors such as `pollster::block_on` **do not drive the run loop**.
Therefore, if you want to wait for a task submitted via `spawn` to complete, using an external
executor will deadlock.

`RunLoop::block_on` continuously calls the platform-specific poll (`platform_run_loop.poll_once`)
while polling the Future. This allows the Future being awaited to make progress even when it
depends on another task completing on the run loop.

Nested `block_on` calls are detected by the `BLOCK_ON_ACTIVE` flag and will panic (to prevent
deadlocks from re-entrancy).

---

## Responsibilities of the platform backend

Each platform backend (`src/platform/`) implements the following:

- `PlatformRunLoop` — creation, teardown, and polling of the run loop (`poll_once`)
- `PlatformRunLoopSender` — enqueuing callbacks from other threads
- `PollSession` — polling state management inside `block_on`

When adding a new platform, refer to the `cfg` branches in `src/platform/mod.rs` and the
`PollSession` implementation.

---

## Change guidelines

- **Do not panic in the shutdown path**: It will take down the DAW host. Wrap with `catch_unwind`
  or ensure the implementation cannot panic.
- **Do not carelessly change the main-thread detection logic**: `RUN_LOOP_THREAD_ID` is acquired
  and compared in multiple places; changing it will break the consistency of `sender()`,
  `current()`, and `is_run_loop_thread()`.
- **Keep Win32 Window Class names and CFRunLoop RunLoopMode names unique**: Maintain the
  crate-specific prefix to prevent name collisions with `irondash` and other libraries.
