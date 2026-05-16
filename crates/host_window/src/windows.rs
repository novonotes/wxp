use core::num::NonZeroIsize;
use raw_window_handle::{HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use std::ffi::OsStr;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use winapi::shared::minwindef::{HINSTANCE, LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::winuser::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindowVisible,
    RegisterClassW, SW_HIDE, SW_SHOW, ShowWindow, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

static WINDOW_CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);
const WINDOW_CLASS_NAME: &str = "novonotes.host_window.Window";

/// A handle to the development host window, usable as a `raw-window-handle` source.
///
/// The handle may be moved between threads, but the `HWND` it wraps belongs to
/// the thread that created the window (Win32 ties window messages to that
/// thread). The `Send`/`Sync` impls only make the handle transportable.
#[derive(Clone, Copy)]
pub struct HostWindowHandle {
    hwnd: HWND,
}

// SAFETY: only the raw `HWND` value is shared. The dev harness drives the window
// from its owning thread, so cross-thread message handling never occurs.
unsafe impl Send for HostWindowHandle {}
unsafe impl Sync for HostWindowHandle {}

impl HostWindowHandle {
    /// Returns the raw HWND pointer
    pub fn as_raw(&self) -> HWND {
        self.hwnd
    }

    /// Shows the window
    pub fn show(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_SHOW);
        }
    }

    /// Hides the window
    pub fn hide(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// Checks whether the window is visible
    pub fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd) != 0 }
    }

    /// Destroys the window
    pub fn destroy(self) {
        unsafe {
            if !self.hwnd.is_null() {
                DestroyWindow(self.hwnd);
            }
        }
    }
}

impl HasWindowHandle for HostWindowHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        let handle = Win32WindowHandle::new(
            NonZeroIsize::new(self.hwnd as isize)
                .ok_or(raw_window_handle::HandleError::Unavailable)?,
        );
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(handle)) })
    }
}

/// Builds the dev host window.
pub(crate) fn create_window(title: &str, width: f64, height: f64) -> HostWindowHandle {
    unsafe {
        let hinstance = GetModuleHandleW(ptr::null());

        // A window class can only be registered once per process; the harness
        // may create several windows, so guard the registration with an atomic.
        if !WINDOW_CLASS_REGISTERED.swap(true, Ordering::SeqCst) {
            register_window_class(hinstance);
        }

        let hwnd = create_win32_window(title, width as i32, height as i32, hinstance);

        HostWindowHandle { hwnd }
    }
}

/// Registers the window class
unsafe fn register_window_class(hinstance: HINSTANCE) {
    unsafe {
        let class_name: Vec<u16> = OsStr::new(WINDOW_CLASS_NAME)
            .encode_wide()
            .chain(once(0))
            .collect();

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: ptr::null_mut(),
            hCursor: ptr::null_mut(),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };

        let class_atom = RegisterClassW(&wc);
        if class_atom == 0 {
            panic!(
                "failed to register Win32 window class `{}`: error {}",
                WINDOW_CLASS_NAME,
                GetLastError()
            );
        }
    }
}

/// Creates a Win32 window
unsafe fn create_win32_window(title: &str, width: i32, height: i32, hinstance: HINSTANCE) -> HWND {
    unsafe {
        let class_name: Vec<u16> = OsStr::new(WINDOW_CLASS_NAME)
            .encode_wide()
            .chain(once(0))
            .collect();
        let window_name: Vec<u16> = OsStr::new(title).encode_wide().chain(once(0)).collect();

        // Callers pass the desired *client* (content) size, but `CreateWindowExW`
        // takes the outer size. Inflate by the frame so the WebView gets exactly
        // the requested dimensions.
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        winapi::um::winuser::AdjustWindowRect(&mut rect, WS_OVERLAPPEDWINDOW, 0);

        let window_width = rect.right - rect.left;
        let window_height = rect.bottom - rect.top;

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_name.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            100, // x
            100, // y
            window_width,
            window_height,
            ptr::null_mut(),
            ptr::null_mut(),
            hinstance,
            ptr::null_mut(),
        );

        if hwnd.is_null() {
            panic!(
                "failed to create Win32 window for class `{}`: error {}",
                WINDOW_CLASS_NAME,
                GetLastError()
            );
        }

        ShowWindow(hwnd, SW_SHOW);
        hwnd
    }
}

/// Window procedure: defers everything to the default handler.
///
/// The dev harness only needs a window to parent the WebView into; it does no
/// custom input or painting, so there is no message to intercept here.
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
