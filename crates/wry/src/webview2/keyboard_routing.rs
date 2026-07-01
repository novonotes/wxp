use std::cell::RefCell;
use std::rc::Rc;

use windows::{
  core::BOOL,
  Win32::{Foundation::*, UI::WindowsAndMessaging::*},
};

use crate::webview2::{DefSubclassProc, SetWindowSubclass};
use crate::KeyboardEventDestination;

const KEYBOARD_ROUTING_SUBCLASS_ID: u32 = WM_USER + 0x67;

type KeyboardRoutes = Rc<RefCell<Vec<(u32, KeyboardEventDestination)>>>;

struct KeyboardRoutingSubclassData {
  parent: HWND,
  routes: KeyboardRoutes,
}

pub(super) unsafe fn install(parent: HWND, hwnd: HWND, routes: KeyboardRoutes) {
  attach_subclass(hwnd, parent, routes.clone());

  let data = KeyboardRoutingSubclassData { parent, routes };
  let _ = EnumChildWindows(
    Some(hwnd),
    Some(enum_child_proc),
    LPARAM((&data as *const KeyboardRoutingSubclassData) as isize),
  );
}

unsafe extern "system" fn subclass_proc(
  hwnd: HWND,
  msg: u32,
  wparam: WPARAM,
  lparam: LPARAM,
  _uidsubclass: usize,
  dwrefdata: usize,
) -> LRESULT {
  match msg {
    WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
      let data = &*(dwrefdata as *const KeyboardRoutingSubclassData);
      let destination = data
        .routes
        .borrow()
        .iter()
        .find_map(|(virtual_key, destination)| {
          (*virtual_key == wparam.0 as u32).then_some(*destination)
        })
        .unwrap_or(KeyboardEventDestination::WebView);

      match destination {
        KeyboardEventDestination::WebView => {}
        KeyboardEventDestination::Parent => {
          // Forward the original message while WebView2 still has the native event. Posting from
          // JavaScript later cannot trigger plugin-host transport accelerators reliably.
          return SendMessageW(data.parent, msg, Some(wparam), Some(lparam));
        }
        KeyboardEventDestination::WebViewAndParent => {
          let _ = SendMessageW(data.parent, msg, Some(wparam), Some(lparam));
        }
      }
    }

    WM_NCDESTROY => {
      if !(dwrefdata as *mut ()).is_null() {
        drop(Box::from_raw(dwrefdata as *mut KeyboardRoutingSubclassData));
      }
    }

    _ => (),
  }

  DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn enum_child_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
  let data = &*(lparam.0 as *const KeyboardRoutingSubclassData);
  attach_subclass(hwnd, data.parent, data.routes.clone());
  true.into()
}

unsafe fn attach_subclass(hwnd: HWND, parent: HWND, routes: KeyboardRoutes) {
  let _ = SetWindowSubclass(
    hwnd,
    Some(subclass_proc),
    KEYBOARD_ROUTING_SUBCLASS_ID as _,
    Box::into_raw(Box::new(KeyboardRoutingSubclassData { parent, routes })) as _,
  );
}
