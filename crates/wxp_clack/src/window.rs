//! Conversion utilities between CLACK's Window and wry's WindowHandle

use clack_extensions::gui::Window;
use std::num::{NonZeroIsize, NonZeroU32};
use std::ptr::NonNull;
use wxp::raw_window_handle::{
    AppKitWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle, XcbWindowHandle,
};

/// Converts a CLACK Window to a wry WindowHandle
///
/// # Safety
/// This function handles raw window handles and requires care regarding safety.
/// It assumes that the window handle obtained from CLACK is valid.
///
/// # Examples
/// ```no_run
/// use clack_extensions::gui::Window;
/// use wxp_clack::window::clack_to_wry_window_handle;
///
/// fn set_parent(window: Window) -> Result<(), Box<dyn std::error::Error>> {
///     let parent_handle = clack_to_wry_window_handle(&window)?;
///     // Use parent_handle as the parent for WebView
///     Ok(())
/// }
/// ```
pub fn clack_to_wry_window_handle<'a>(window: &'a Window) -> Result<WindowHandle<'a>, String> {
    unsafe {
        let raw_handle = if cfg!(target_os = "macos") {
            let nsview = window
                .as_cocoa_nsview()
                .ok_or("Failed to get NSView from CLACK window")?;
            RawWindowHandle::AppKit(AppKitWindowHandle::new(
                NonNull::new(nsview).ok_or("NSView pointer is null")?,
            ))
        } else if cfg!(target_os = "windows") {
            let hwnd = window
                .as_win32_hwnd()
                .ok_or("Failed to get Win32 HWND from CLACK window")?;
            RawWindowHandle::Win32(Win32WindowHandle::new(
                NonZeroIsize::new(hwnd as isize).ok_or("HWND is zero")?,
            ))
        } else {
            // Linux/X11
            let x11_handle = window
                .as_x11_handle()
                .ok_or("Failed to get X11 handle from CLACK window")?;
            RawWindowHandle::Xcb(XcbWindowHandle::new(
                NonZeroU32::new(x11_handle as u32).ok_or("X11 handle is zero")?,
            ))
        };

        Ok(WindowHandle::borrow_raw(raw_handle))
    }
}
