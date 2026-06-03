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
//! - [`RunLoop::init()`] marks the current thread as the run loop thread and returns a guard that releases that initialization reference on drop.
//! - Use [`RunLoop::post()`] or [`RunLoop::call()`] from other threads.
//! - Use [`RunLoopGuard::local()`] for thread-affine operations that may capture `!Send` state.
//! - Tests have a singleton constraint and must be serialized with `#[serial_test::serial]`.

#![allow(clippy::new_without_default)]

mod handle;
mod run_loop;
mod run_loop_sender;
mod task;
#[doc(hidden)]
pub mod test_harness;
#[doc(hidden)]
pub mod test_helper;
mod thread_id;

pub use handle::Handle;
pub use run_loop::{Error, Result, RunLoop, RunLoopGuard, RunLoopLocal};
#[doc(hidden)]
pub use run_loop_sender::RunLoopSender;
pub(crate) use task::Task;
pub use task::{JoinError, JoinHandle};
pub(crate) use thread_id::{SystemThreadId, get_system_thread_id};

pub(crate) mod platform;
pub(crate) mod util;
