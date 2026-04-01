//! CLACKのWindowとwryのWindowHandle間の変換ユーティリティ

use clack_extensions::gui::Window;
use std::num::{NonZeroIsize, NonZeroU32};
use std::ptr::NonNull;
use wxp::raw_window_handle::{
    AppKitWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle, XcbWindowHandle,
};

/// CLACKのWindowをwryのWindowHandleに変換
///
/// # Safety
/// この関数は生のウィンドウハンドルを扱うため、安全性に注意が必要です。
/// CLACKから取得したウィンドウハンドルが有効であることを前提としています。
///
/// # Examples
/// ```no_run
/// use clack_extensions::gui::Window;
/// use wxp_clack::window::clack_to_wry_window_handle;
///
/// fn set_parent(window: Window) -> Result<(), Box<dyn std::error::Error>> {
///     let parent_handle = clack_to_wry_window_handle(&window)?;
///     // parent_handleをWebViewの親として使用
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
