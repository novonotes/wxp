#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::HostWindowHandle;
#[cfg(target_os = "macos")]
pub use macos::HostWindowHandle;
#[cfg(target_os = "windows")]
pub use windows::HostWindowHandle;

use std::sync::Arc;

/// Native host window content size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostWindowSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Default)]
pub struct HostWindowCallbacks {
    pub on_close: Option<Arc<dyn Fn() + Send + Sync>>,
    pub on_resize: Option<Arc<dyn Fn(HostWindowSize) -> HostWindowSize + Send + Sync>>,
}

/// Creates a standalone native window to host a WebView during local development.
///
/// In production a plugin receives its window from the DAW; this crate fakes that
/// host so wxp can be run and tested as an ordinary application. The returned
/// [`HostWindowHandle`] exposes a `raw-window-handle` that can be passed straight
/// into wxp's child-WebView builder, mirroring how a real host hands one over.
pub fn create_window(title: &str, width: f64, height: f64) -> HostWindowHandle {
    create_window_with_callbacks(title, width, height, HostWindowCallbacks::default())
}

pub fn create_window_with_callbacks(
    title: &str,
    width: f64,
    height: f64,
    callbacks: HostWindowCallbacks,
) -> HostWindowHandle {
    #[cfg(target_os = "macos")]
    return macos::create_window(title, width, height, callbacks);

    #[cfg(target_os = "windows")]
    return windows::create_window(title, width, height, callbacks);

    #[cfg(target_os = "linux")]
    return linux::create_window(title, width, height, callbacks);
}
