use raw_window_handle::{HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle};
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_ulong};
use std::ptr;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use x11::xlib::*;

use crate::{HostWindowCallbacks, HostWindowSize};

/// A handle to the development host window, usable as a `raw-window-handle` source.
///
/// The handle may be moved between threads, but Xlib's `Display` connection is
/// not thread-safe; all calls below must come from the harness thread that owns
/// it. The `Send`/`Sync` impls only make the handle transportable.
#[derive(Clone)]
pub struct HostWindowHandle {
    display: *mut Display,
    window: Window,
    wm_delete_window: Atom,
    callbacks: Arc<HostWindowCallbacks>,
    running: Arc<AtomicBool>,
}

// SAFETY: only the handle is shared. The dev harness serializes all Xlib calls
// on its owning thread, so the unsynchronized `Display` is never used elsewhere.
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
            self.poll_events();
            XMapWindow(self.display, self.window);
            XFlush(self.display);
        }
    }

    /// Hides the window
    pub fn hide(&self) {
        unsafe {
            self.poll_events();
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

    /// Returns the window size.
    pub fn size(&self) -> HostWindowSize {
        unsafe {
            self.poll_events();
            let mut attrs: XWindowAttributes = std::mem::zeroed();
            if XGetWindowAttributes(self.display, self.window, &mut attrs) != 0 {
                HostWindowSize {
                    width: attrs.width.max(0) as u32,
                    height: attrs.height.max(0) as u32,
                }
            } else {
                HostWindowSize {
                    width: 0,
                    height: 0,
                }
            }
        }
    }

    /// Resizes the X11 window.
    pub fn set_size(&self, width: u32, height: u32) {
        unsafe {
            self.poll_events();
            XResizeWindow(self.display, self.window, width, height);
            XFlush(self.display);
        }
    }

    /// Processes pending X11 events for this host window.
    pub fn poll_events(&self) {
        unsafe {
            while XPending(self.display) > 0 {
                let mut event = std::mem::zeroed::<XEvent>();
                XNextEvent(self.display, &mut event);
                match event.get_type() {
                    ClientMessage => {
                        let client = event.client_message;
                        if client.window == self.window
                            && client.data.get_long(0) as Atom == self.wm_delete_window
                        {
                            if let Some(on_close) = self.callbacks.on_close.as_ref() {
                                on_close();
                            }
                            XUnmapWindow(self.display, self.window);
                        }
                    }
                    ConfigureNotify => {
                        let configure = event.configure;
                        if configure.window == self.window
                            && let Some(on_resize) = self.callbacks.on_resize.as_ref()
                        {
                            let current = HostWindowSize {
                                width: configure.width.max(0) as u32,
                                height: configure.height.max(0) as u32,
                            };
                            let adjusted = on_resize(current);
                            if adjusted != current {
                                XResizeWindow(
                                    self.display,
                                    self.window,
                                    adjusted.width,
                                    adjusted.height,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            XFlush(self.display);
        }
    }

    /// Destroys the window
    pub fn destroy(self) {
        unsafe {
            if !self.display.is_null() && self.window != 0 {
                self.running.store(false, Ordering::Release);
                XDestroyWindow(self.display, self.window);
                XFlush(self.display);
            }
        }
    }
}

impl HasWindowHandle for HostWindowHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        let mut handle = XlibWindowHandle::new(self.window);
        // 0 tells consumers to use the screen's default visual; the dev window
        // is created with `XCreateSimpleWindow`, which inherits exactly that.
        handle.visual_id = 0;
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Xlib(handle)) })
    }
}

/// Builds the dev host window.
pub(crate) fn create_window(
    title: &str,
    width: f64,
    height: f64,
    callbacks: HostWindowCallbacks,
) -> HostWindowHandle {
    unsafe {
        XInitThreads();
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

        // Opt into WM_DELETE_WINDOW so the window manager's close button delivers
        // a client message instead of killing the X connection out from under us.
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

        let callbacks = Arc::new(callbacks);
        let running = Arc::new(AtomicBool::new(true));
        spawn_event_thread(
            display,
            window,
            wm_delete_window,
            Arc::clone(&callbacks),
            Arc::clone(&running),
        );

        HostWindowHandle {
            display,
            window,
            wm_delete_window,
            callbacks,
            running,
        }
    }
}

fn spawn_event_thread(
    display: *mut Display,
    window: Window,
    wm_delete_window: Atom,
    callbacks: Arc<HostWindowCallbacks>,
    running: Arc<AtomicBool>,
) {
    let display_addr = display as usize;
    thread::spawn(move || unsafe {
        let display = display_addr as *mut Display;
        while running.load(Ordering::Acquire) {
            let mut event = std::mem::zeroed::<XEvent>();
            XNextEvent(display, &mut event);
            handle_x_event(display, window, wm_delete_window, &callbacks, event);
        }
    });
}

unsafe fn handle_x_event(
    display: *mut Display,
    window: Window,
    wm_delete_window: Atom,
    callbacks: &HostWindowCallbacks,
    event: XEvent,
) {
    unsafe {
        match event.get_type() {
            ClientMessage => {
                let client = event.client_message;
                if client.window == window && client.data.get_long(0) as Atom == wm_delete_window {
                    if let Some(on_close) = callbacks.on_close.as_ref() {
                        on_close();
                    }
                    XUnmapWindow(display, window);
                }
            }
            ConfigureNotify => {
                let configure = event.configure;
                if configure.window == window
                    && let Some(on_resize) = callbacks.on_resize.as_ref()
                {
                    let current = HostWindowSize {
                        width: configure.width.max(0) as u32,
                        height: configure.height.max(0) as u32,
                    };
                    let adjusted = on_resize(current);
                    if adjusted != current {
                        XResizeWindow(display, window, adjusted.width, adjusted.height);
                    }
                }
            }
            _ => {}
        }
        XFlush(display);
    }
}

// The `x11` crate does not re-export these event-mask bits, so define the ones
// we need here. Values are fixed by the X11 protocol.
const ExposureMask: CLong = 1 << 15;
const KeyPressMask: CLong = 1 << 0;
const StructureNotifyMask: CLong = 1 << 17;

type CUint = u32;
type CLong = i64;
