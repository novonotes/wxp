use objc2::rc::Id;
use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_foundation::{CGPoint, CGRect, CGSize, MainThreadMarker, NSString};
use raw_window_handle::{AppKitWindowHandle, HasWindowHandle, RawWindowHandle, WindowHandle};
use std::ptr::NonNull;

/// A handle to the development host window, usable as a `raw-window-handle` source.
///
/// Callers may hold or move this between threads, but every method that touches
/// the underlying `NSWindow` must still run on the main thread — AppKit requires
/// it. The `Send`/`Sync` impls below only make the handle *transportable*; they
/// do not make `NSWindow` itself thread-safe.
#[derive(Clone)]
pub struct HostWindowHandle {
    ns_window: Id<NSWindow>,
}

// SAFETY: only the handle is shared across threads. The dev harness keeps all
// AppKit calls on the main thread, so the unsynchronized `NSWindow` is never
// actually touched off-thread.
unsafe impl Send for HostWindowHandle {}
unsafe impl Sync for HostWindowHandle {}

impl HostWindowHandle {
    /// Returns the raw NSWindow pointer
    pub fn as_raw(&self) -> *mut NSWindow {
        Id::as_ptr(&self.ns_window) as *mut NSWindow
    }

    /// Shows the window and brings it to the foreground.
    pub fn show(&self) {
        self.ns_window.makeKeyAndOrderFront(None);
        unsafe {
            // `makeKeyAndOrderFront` is ignored when the dev harness is not the
            // active app (common when launched from a terminal/IDE), so force
            // the window forward regardless of activation state.
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

    /// Closes the window and releases the handle.
    ///
    /// Takes `self` by value so the window cannot be used after teardown. The
    /// `NSWindow` itself is reference-counted and freed when the last `Id` drops;
    /// `close()` is only needed to dismiss a still-visible window.
    pub fn destroy(self) {
        if self.is_visible() {
            self.ns_window.close();
        }
    }
}

impl HasWindowHandle for HostWindowHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        let ns_window_ptr = self.as_raw();
        let mut handle = AppKitWindowHandle::new(
            NonNull::new(ns_window_ptr as *mut _)
                .ok_or(raw_window_handle::HandleError::Unavailable)?,
        );

        // wry attaches its WKWebView as a subview of `ns_view`, so point the
        // handle at the window's content view rather than the window itself.
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

/// Builds the dev host window.
///
/// Deliberately does not bootstrap `NSApplication`: a plugin never owns the app
/// object, so the harness stays closer to the real embedded scenario and leaves
/// app/run-loop setup to the caller.
pub(crate) fn create_window(title: &str, width: f64, height: f64) -> HostWindowHandle {
    let ns_window = create_ns_window(title, width, height);
    HostWindowHandle { ns_window }
}

/// Allocates the backing `NSWindow`. Must be called on the main thread (asserted
/// via `MainThreadMarker`), since AppKit window creation is main-thread-only.
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
