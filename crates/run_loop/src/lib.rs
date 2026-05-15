//! A crate that provides a unified API over platform-specific native run loops
//! (CFRunLoop / ALooper / GMainContext / Win32 message loop).
//! A fork of [irondash_run_loop](https://github.com/irondash/irondash) with enhanced safety
//! for DLL and audio plugin environments.
//!
//! For usage examples see the [README](https://github.com/novonotes/wxp/tree/main/crates/run_loop).
//! For design background see [docs/maintainers.md](../docs/maintainers.md).
//!
//! ## Notes
//!
//! - [`RunLoop::current()`] may only be called from the run loop thread. Use [`RunLoop::sender()`] from other threads.
//! - Always pair `init()` with `deinit()` (the implementation uses reference counting internally).
//! - Tests have a singleton constraint and must be serialized with `#[serial_test::serial]` (see [`test_harness`]).

#![allow(clippy::new_without_default)]

mod handle;
mod main_thread;
mod run_loop;
mod run_loop_sender;
mod task;
pub mod test_harness;
pub mod test_helper;
mod thread_id;

pub use handle::*;
pub use run_loop::*;
pub use run_loop_sender::*;
pub use task::*;
pub use thread_id::*;

// Note: These modules are public but there are no API stability guarantees
pub mod platform;
pub mod util;
