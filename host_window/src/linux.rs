use raw_window_handle::{HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle};
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_ulong};
use std::ptr;
use x11::xlib::*;

/// host_window のウィンドウハンドル
///
/// このハンドルは `Send` と `Sync` を実装しており、
/// スレッド間で安全に共有できます。
#[derive(Clone, Copy)]
pub struct HostWindowHandle {
    display: *mut Display,
    window: Window,
}

unsafe impl Send for HostWindowHandle {}
unsafe impl Sync for HostWindowHandle {}

impl HostWindowHandle {
    /// 生のWindow IDを取得
    pub fn as_raw(&self) -> Window {
        self.window
    }

    /// ウィンドウを表示
    pub fn show(&self) {
        unsafe {
            XMapWindow(self.display, self.window);
            XFlush(self.display);
        }
    }

    /// ウィンドウを非表示
    pub fn hide(&self) {
        unsafe {
            XUnmapWindow(self.display, self.window);
            XFlush(self.display);
        }
    }

    /// ウィンドウの可視性をチェック
    pub fn is_visible(&self) -> bool {
        unsafe {
            let mut attrs: XWindowAttributes = std::mem::zeroed();
            if XGetWindowAttributes(self.display, self.window, &mut attrs) != 0 {
                attrs.map_state == IsViewable
            } else {
                false
            }
        }
    }

    /// ウィンドウを破棄
    pub fn destroy(self) {
        unsafe {
            if !self.display.is_null() && self.window != 0 {
                XDestroyWindow(self.display, self.window);
                XFlush(self.display);
            }
        }
    }
}

impl HasWindowHandle for HostWindowHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        let mut handle = XlibWindowHandle::new(self.window);
        handle.visual_id = 0; // デフォルトビジュアルを使用
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Xlib(handle)) })
    }
}

/// プラグイン環境用のウィンドウを作成
pub(crate) fn create_window(title: &str, width: f64, height: f64) -> HostWindowHandle {
    unsafe {
        // ディスプレイを開く
        let display = XOpenDisplay(ptr::null());
        if display.is_null() {
            panic!("Failed to open X11 display");
        }

        let screen = XDefaultScreen(display);
        let root = XRootWindow(display, screen);
        let black_pixel = XBlackPixel(display, screen);
        let white_pixel = XWhitePixel(display, screen);

        // ウィンドウを作成
        let window = XCreateSimpleWindow(
            display,
            root,
            100,
            100,
            width as CUint,
            height as CUint,
            1,
            black_pixel,
            white_pixel,
        );

        // ウィンドウタイトルを設定
        let title_cstring = CString::new(title).unwrap();
        XStoreName(display, window, title_cstring.as_ptr() as *mut c_char);

        // ウィンドウマネージャーのプロトコルを設定
        let wm_protocols = XInternAtom(display, b"WM_PROTOCOLS\0".as_ptr() as *const c_char, False);
        let wm_delete_window = XInternAtom(
            display,
            b"WM_DELETE_WINDOW\0".as_ptr() as *const c_char,
            False,
        );
        let mut protocols = [wm_delete_window];
        XSetWMProtocols(
            display,
            window,
            protocols.as_mut_ptr(),
            protocols.len() as c_int,
        );

        // イベントマスクを設定
        XSelectInput(
            display,
            window,
            ExposureMask | KeyPressMask | StructureNotifyMask,
        );

        // ウィンドウを表示
        XMapWindow(display, window);
        XFlush(display);

        HostWindowHandle { display, window }
    }
}

// 追加の定数定義
const ExposureMask: CLong = 1 << 15;
const KeyPressMask: CLong = 1 << 0;
const StructureNotifyMask: CLong = 1 << 17;

type CUint = u32;
type CLong = i64;
