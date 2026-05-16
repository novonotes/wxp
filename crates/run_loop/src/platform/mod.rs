//! Selects the per-OS run loop backend and re-exports it under one name.
//!
//! The rest of the crate programs against `platform::*` and never against a
//! specific OS, so each backend must expose the same surface (`PlatformRunLoop`,
//! `PlatformRunLoopSender`, `PollSession`, thread-id helpers).

pub use self::platform_impl::*;

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[path = "darwin/mod.rs"]
mod platform_impl;

#[cfg(target_os = "windows")]
#[path = "win32/mod.rs"]
mod platform_impl;

#[cfg(target_os = "linux")]
#[path = "linux/mod.rs"]
mod platform_impl;

#[cfg(target_os = "android")]
#[path = "android/mod.rs"]
mod platform_impl;
