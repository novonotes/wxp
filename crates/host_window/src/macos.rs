use objc2::rc::Id;
use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_foundation::{CGPoint, CGRect, CGSize, MainThreadMarker, NSString};
use raw_window_handle::{AppKitWindowHandle, HasWindowHandle, RawWindowHandle, WindowHandle};
use std::ptr::NonNull;

/// Window handle for host_window
///
/// This handle implements `Send` and `Sync`,
/// and can be safely shared across threads.
#[derive(Clone)]
pub struct HostWindowHandle {
    ns_window: Id<NSWindow>,
}

unsafe impl Send for HostWindowHandle {}
unsafe impl Sync for HostWindowHandle {}

impl HostWindowHandle {
    /// Returns the raw NSWindow pointer
    pub fn as_raw(&self) -> *mut NSWindow {
        Id::as_ptr(&self.ns_window) as *mut NSWindow
    }

    /// Shows the window
    pub fn show(&self) {
        self.ns_window.makeKeyAndOrderFront(None);
        unsafe {
            // Bring the window to the front again
            self.ns_window.orderFrontRegardless();
        }
    }

    /// Hides the window
    pub fn hide(&self) {
        self.ns_window.orderOut(None);
    }

    /// Checks whether the window is visible
    pub fn is_visible(&self) -> bool {
        self.ns_window.isVisible()
    }

    /// Destroys the window
    pub fn destroy(self) {
        // Check if the window is still visible
        if self.is_visible() {
            // Close the window
            self.ns_window.close();
        }
        // Id<NSWindow> is automatically released when dropped
    }
}

impl HasWindowHandle for HostWindowHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        let ns_window_ptr = self.as_raw();
        let mut handle = AppKitWindowHandle::new(
            NonNull::new(ns_window_ptr as *mut _)
                .ok_or(raw_window_handle::HandleError::Unavailable)?,
        );

        // Set the contentView of this window on AppKitWindowHandle's ns_view
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

/// Creates a window for the plugin environment (skips NSApp initialization)
pub(crate) fn create_window(title: &str, width: f64, height: f64) -> HostWindowHandle {
    let ns_window = create_ns_window(title, width, height);
    HostWindowHandle { ns_window }
}

/// Creates an NSWindow
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

        // Set the window title
        let title_str = NSString::from_str(title);
        ns_window.setTitle(&title_str);

        // Show the window
        ns_window.center();
        ns_window.makeKeyAndOrderFront(None);

        ns_window
    }
}
