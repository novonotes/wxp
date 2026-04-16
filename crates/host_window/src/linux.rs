use raw_window_handle::{HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle};
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_ulong};
use std::ptr;
use x11::xlib::*;

/// Window handle for host_window
///
/// This handle implements `Send` and `Sync`,
/// and can be safely shared across threads.
#[derive(Clone, Copy)]
pub struct HostWindowHandle {
    display: *mut Display,
    window: Window,
}

unsafe impl Send for HostWindowHandle {}
unsafe impl Sync for HostWindowHandle {}

impl HostWindowHandle {
    /// Returns the raw Window ID
    pub fn as_raw(&self) -> Window {
        self.window
    }

    /// Shows the window
    pub fn show(&self) {
        unsafe {
            XMapWindow(self.display, self.window);
            XFlush(self.display);
        }
    }

    /// Hides the window
    pub fn hide(&self) {
        unsafe {
            XUnmapWindow(self.display, self.window);
            XFlush(self.display);
        }
    }

    /// Checks whether the window is visible
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

    /// Destroys the window
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
        handle.visual_id = 0; // Use the default visual
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Xlib(handle)) })
    }
}

/// Creates a window for the plugin environment
pub(crate) fn create_window(title: &str, width: f64, height: f64) -> HostWindowHandle {
    unsafe {
        // Open the display
        let display = XOpenDisplay(ptr::null());
        if display.is_null() {
            panic!("Failed to open X11 display");
        }

        let screen = XDefaultScreen(display);
        let root = XRootWindow(display, screen);
        let black_pixel = XBlackPixel(display, screen);
        let white_pixel = XWhitePixel(display, screen);

        // Create the window
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

        // Set the window title
        let title_cstring = CString::new(title).unwrap();
        XStoreName(display, window, title_cstring.as_ptr() as *mut c_char);

        // Set up window manager protocols
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

        // Set the event mask
        XSelectInput(
            display,
            window,
            ExposureMask | KeyPressMask | StructureNotifyMask,
        );

        // Show the window
        XMapWindow(display, window);
        XFlush(display);

        HostWindowHandle { display, window }
    }
}

// Additional constant definitions
const ExposureMask: CLong = 1 << 15;
const KeyPressMask: CLong = 1 << 0;
const StructureNotifyMask: CLong = 1 << 17;

type CUint = u32;
type CLong = i64;
