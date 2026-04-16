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

/// Creates a window for the plugin environment
pub fn create_window(title: &str, width: f64, height: f64) -> HostWindowHandle {
    #[cfg(target_os = "macos")]
    return macos::create_window(title, width, height);

    #[cfg(target_os = "windows")]
    return windows::create_window(title, width, height);

    #[cfg(target_os = "linux")]
    return linux::create_window(title, width, height);
}
