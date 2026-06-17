//! Hand-written Win32 FFI for the Windows backend.
//!
//! Declares only the handful of user32/kernel32 symbols and structs this crate
//! needs, to avoid a heavy `windows`/`winapi` dependency in the run loop. Layout
//! and names match the Win32 SDK verbatim — do not reorder struct fields.

#[allow(non_camel_case_types, non_snake_case, clippy::upper_case_acronyms)]
pub(crate) mod windows {
    pub(crate) type DWORD = u32;
    pub(crate) type HWND = isize;
    pub(crate) type HANDLE = isize;
    pub(crate) type LPARAM = isize;
    pub(crate) type WPARAM = usize;
    pub(crate) type LRESULT = isize;
    pub(crate) type PWSTR = *mut u16;
    pub(crate) type HINSTANCE = isize;
    pub(crate) type WINDOW_EX_STYLE = u32;
    pub(crate) type WINDOW_STYLE = u32;
    pub(crate) type HMENU = isize;
    pub(crate) type WINDOW_LONG_PTR_INDEX = i32;
    pub(crate) type HCURSOR = isize;
    pub(crate) type WNDCLASS_STYLES = u32;
    pub(crate) type HICON = isize;
    pub(crate) type HBRUSH = isize;
    pub(crate) type BOOL = i32;
    pub(crate) type WNDPROC = unsafe extern "system" fn(
        param0: HWND,
        param1: u32,
        param2: WPARAM,
        param3: LPARAM,
    ) -> LRESULT;
    pub(crate) type TIMERPROC =
        unsafe extern "system" fn(param0: HWND, param1: u32, param2: usize, param3: u32);
    pub(crate) type QUEUE_STATUS_FLAGS = u32;
    pub(crate) type MSG_WAIT_FOR_MULTIPLE_OBJECTS_EX_FLAGS = u32;
    pub(crate) type PEEK_MESSAGE_REMOVE_TYPE = u32;

    pub(crate) const GWLP_USERDATA: WINDOW_LONG_PTR_INDEX = -21i32;
    pub(crate) const IDC_ARROW: PWSTR = 32512i32 as _;

    pub(crate) const WM_NCCREATE: u32 = 129u32;
    pub(crate) const WM_NCDESTROY: u32 = 130u32;
    pub(crate) const WM_TIMER: u32 = 275u32;
    pub(crate) const WM_USER: u32 = 1024u32;

    pub(crate) const HWND_MESSAGE: isize = (-3i32) as _;

    pub(crate) const QS_POSTMESSAGE: QUEUE_STATUS_FLAGS = 8u32;
    pub(crate) const QS_TIMER: QUEUE_STATUS_FLAGS = 0x10u32;

    pub(crate) const MWMO_INPUTAVAILABLE: MSG_WAIT_FOR_MULTIPLE_OBJECTS_EX_FLAGS = 4u32;

    pub(crate) const PM_REMOVE: PEEK_MESSAGE_REMOVE_TYPE = 1u32;
    pub(crate) const PM_NOYIELD: PEEK_MESSAGE_REMOVE_TYPE = 2u32;

    #[repr(C)]
    pub(crate) struct WNDCLASSW {
        pub(crate) style: WNDCLASS_STYLES,
        pub(crate) lpfnWndProc: WNDPROC,
        pub(crate) cbClsExtra: i32,
        pub(crate) cbWndExtra: i32,
        pub(crate) hInstance: HINSTANCE,
        pub(crate) hIcon: HICON,
        pub(crate) hCursor: HCURSOR,
        pub(crate) hbrBackground: HBRUSH,
        pub(crate) lpszMenuName: PWSTR,
        pub(crate) lpszClassName: PWSTR,
    }

    #[repr(C)]
    pub(crate) struct CREATESTRUCTW {
        pub(crate) lpCreateParams: *mut ::core::ffi::c_void,
        pub(crate) hInstance: HINSTANCE,
        pub(crate) hMenu: HMENU,
        pub(crate) hwndParent: HWND,
        pub(crate) cy: i32,
        pub(crate) cx: i32,
        pub(crate) y: i32,
        pub(crate) x: i32,
        pub(crate) style: i32,
        pub(crate) lpszName: PWSTR,
        pub(crate) lpszClass: PWSTR,
        pub(crate) dwExStyle: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    pub(crate) struct POINT {
        pub(crate) x: i32,
        pub(crate) y: i32,
    }

    #[repr(C)]
    #[derive(Default)]
    pub(crate) struct MSG {
        pub(crate) hwnd: HWND,
        pub(crate) message: u32,
        pub(crate) wParam: WPARAM,
        pub(crate) lParam: LPARAM,
        pub(crate) time: u32,
        pub(crate) pt: POINT,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub(crate) fn GetModuleHandleW(lpmodulename: PWSTR) -> HINSTANCE;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        pub(crate) fn CreateWindowExW(
            dwexstyle: WINDOW_EX_STYLE,
            lpclassname: PWSTR,
            lpwindowname: PWSTR,
            dwstyle: WINDOW_STYLE,
            x: i32,
            y: i32,
            nwidth: i32,
            nheight: i32,
            hwndparent: HWND,
            hmenu: HMENU,
            hinstance: HINSTANCE,
            lpparam: *const ::core::ffi::c_void,
        ) -> HWND;
        pub(crate) fn DestroyWindow(hWnd: HWND) -> BOOL;
        pub(crate) fn DefWindowProcW(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT;
        pub(crate) fn GetWindowLongPtrW(hwnd: HWND, nindex: WINDOW_LONG_PTR_INDEX) -> isize;
        pub(crate) fn LoadCursorW(hinstance: HINSTANCE, lpcursorname: PWSTR) -> HCURSOR;
        pub(crate) fn RegisterClassW(lpwndclass: *const WNDCLASSW) -> u16;
        pub(crate) fn SetWindowLongPtrW(
            hwnd: HWND,
            nindex: WINDOW_LONG_PTR_INDEX,
            dwnewlong: isize,
        ) -> isize;
        pub(crate) fn UnregisterClassW(lpclassname: PWSTR, hinstance: HINSTANCE) -> BOOL;
        pub(crate) fn DispatchMessageW(lpmsg: *const MSG) -> LRESULT;
        pub(crate) fn GetMessageW(
            lpmsg: *mut MSG,
            hwnd: HWND,
            wmsgfiltermin: u32,
            wmsgfiltermax: u32,
        ) -> BOOL;
        pub(crate) fn PostMessageW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> BOOL;
        pub(crate) fn SetTimer(
            hwnd: HWND,
            nidevent: usize,
            uelapse: u32,
            lptimerfunc: ::core::option::Option<TIMERPROC>,
        ) -> usize;
        pub(crate) fn TranslateMessage(lpmsg: *const MSG) -> BOOL;
        pub(crate) fn MsgWaitForMultipleObjectsEx(
            ncount: u32,
            phandles: *const HANDLE,
            dwmilliseconds: u32,
            dwwakemask: QUEUE_STATUS_FLAGS,
            dwflags: MSG_WAIT_FOR_MULTIPLE_OBJECTS_EX_FLAGS,
        ) -> u32;
        pub(crate) fn PeekMessageW(
            lpmsg: *mut MSG,
            hwnd: HWND,
            wmsgfiltermin: u32,
            wmsgfiltermax: u32,
            wremovemsg: PEEK_MESSAGE_REMOVE_TYPE,
        ) -> BOOL;
        pub(crate) fn GetCurrentThreadId() -> DWORD;
    }
}
