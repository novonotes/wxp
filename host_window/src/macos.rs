use objc2::rc::Id;
use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_foundation::{CGPoint, CGRect, CGSize, MainThreadMarker, NSString};
use raw_window_handle::{AppKitWindowHandle, HasWindowHandle, RawWindowHandle, WindowHandle};
use std::ptr::NonNull;

/// host_window のウィンドウハンドル
///
/// このハンドルは `Send` と `Sync` を実装しており、
/// スレッド間で安全に共有できます。
#[derive(Clone)]
pub struct HostWindowHandle {
    ns_window: Id<NSWindow>,
}

unsafe impl Send for HostWindowHandle {}
unsafe impl Sync for HostWindowHandle {}

impl HostWindowHandle {
    /// 生のNSWindowポインタを取得
    pub fn as_raw(&self) -> *mut NSWindow {
        Id::as_ptr(&self.ns_window) as *mut NSWindow
    }

    /// ウィンドウを表示
    pub fn show(&self) {
        self.ns_window.makeKeyAndOrderFront(None);
        unsafe {
            // ウィンドウを再度最前面に
            self.ns_window.orderFrontRegardless();
        }
    }

    /// ウィンドウを非表示
    pub fn hide(&self) {
        self.ns_window.orderOut(None);
    }

    /// ウィンドウの可視性をチェック
    pub fn is_visible(&self) -> bool {
        self.ns_window.isVisible()
    }

    /// ウィンドウを破棄
    pub fn destroy(self) {
        // ウィンドウがまだ表示されているかチェック
        if self.is_visible() {
            // ウィンドウを閉じる
            self.ns_window.close();
        }
        // Id<NSWindow>のdropで自動的にreleaseされる
    }
}

impl HasWindowHandle for HostWindowHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        let ns_window_ptr = self.as_raw();
        let mut handle = AppKitWindowHandle::new(
            NonNull::new(ns_window_ptr as *mut _)
                .ok_or(raw_window_handle::HandleError::Unavailable)?,
        );

        // AppKitWindowHandle の ns_view にこの window の contentView を設定
        let content_view = self.ns_window.contentView();
        if let Some(view) = content_view {
            let view_ptr = Id::as_ptr(&view) as *mut _;
            if let Some(non_null_ptr) = NonNull::new(view_ptr) {
                handle.ns_view = non_null_ptr;
            }
        }

        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::AppKit(handle)) })
    }
}

/// プラグイン環境用のウィンドウを作成（NSApp初期化をスキップ）
pub(crate) fn create_window(title: &str, width: f64, height: f64) -> HostWindowHandle {
    let ns_window = create_ns_window(title, width, height);
    HostWindowHandle { ns_window }
}

/// NSWindowの作成処理
fn create_ns_window(title: &str, width: f64, height: f64) -> Id<NSWindow> {
    unsafe {
        let mtm = MainThreadMarker::new().expect("Must be on main thread");

        let content_rect = CGRect::new(CGPoint::new(100.0, 100.0), CGSize::new(width, height));

        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;

        let ns_window = NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc::<NSWindow>(),
            content_rect,
            style,
            NSBackingStoreType::NSBackingStoreBuffered,
            false,
        );

        // ウィンドウタイトルの設定
        let title_str = NSString::from_str(title);
        ns_window.setTitle(&title_str);

        // ウィンドウを表示
        ns_window.center();
        ns_window.makeKeyAndOrderFront(None);

        ns_window
    }
}
