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

/// host_window のウィンドウハンドル
///
/// このハンドルは `Send` と `Sync` を実装しており、
/// スレッド間で安全に共有できます。
#[derive(Clone, Copy)]
pub struct HostWindowHandle {
    hwnd: HWND,
}

unsafe impl Send for HostWindowHandle {}
unsafe impl Sync for HostWindowHandle {}

impl HostWindowHandle {
    /// 生のHWNDポインタを取得
    pub fn as_raw(&self) -> HWND {
        self.hwnd
    }

    /// ウィンドウを表示
    pub fn show(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_SHOW);
        }
    }

    /// ウィンドウを非表示
    pub fn hide(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// ウィンドウの可視性をチェック
    pub fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd) != 0 }
    }

    /// ウィンドウを破棄
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

/// プラグイン環境用のウィンドウを作成
pub(crate) fn create_window(title: &str, width: f64, height: f64) -> HostWindowHandle {
    unsafe {
        let hinstance = GetModuleHandleW(ptr::null());

        // ウィンドウクラスを登録（初回のみ）
        if !WINDOW_CLASS_REGISTERED.swap(true, Ordering::SeqCst) {
            register_window_class(hinstance);
        }

        let hwnd = create_win32_window(title, width as i32, height as i32, hinstance);

        HostWindowHandle { hwnd }
    }
}

/// ウィンドウクラスの登録
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

/// Win32ウィンドウの作成
unsafe fn create_win32_window(title: &str, width: i32, height: i32, hinstance: HINSTANCE) -> HWND {
    unsafe {
        let class_name: Vec<u16> = OsStr::new(WINDOW_CLASS_NAME)
            .encode_wide()
            .chain(once(0))
            .collect();
        let window_name: Vec<u16> = OsStr::new(title).encode_wide().chain(once(0)).collect();

        // ウィンドウサイズを計算（クライアント領域のサイズを指定サイズにする）
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

/// ウィンドウプロシージャ
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
