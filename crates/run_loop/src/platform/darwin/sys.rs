//! Minimal raw FFI bindings for the Darwin run loop backend.
//!
//! Names mirror the C symbols verbatim (hence the `non_*` lint allows) so they
//! stay greppable against Apple's headers; keep them 1:1 with the platform API.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(non_upper_case_globals)]

use std::ffi::{c_int, c_void};

// Link the system frameworks/libraries this backend depends on. The empty
// `extern` blocks exist only to emit the link directives.
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "pthread")]
unsafe extern "C" {
    pub(super) fn pthread_threadid_np(thread: *mut c_void, thread_id: *mut u64) -> c_int;
}
