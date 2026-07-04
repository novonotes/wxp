use std::cell::RefCell;
use std::rc::Rc;

use windows::{
  core::BOOL,
  Win32::{
    Foundation::*,
    UI::{Input::KeyboardAndMouse::GetKeyState, WindowsAndMessaging::*},
  },
};

use crate::webview2::{DefSubclassProc, SetWindowSubclass};
use crate::{
  KeyboardEventDestination, KeyboardEventModifiers, KeyboardEventRouting, KeyboardEventRoutingKind,
};

const KEYBOARD_ROUTING_SUBCLASS_ID: u32 = WM_USER + 0x67;

type KeyboardRouting = Rc<RefCell<KeyboardEventRouting<u32>>>;

struct KeyboardRoutingSubclassData {
  parent: HWND,
  routing: KeyboardRouting,
}

pub(super) unsafe fn install(parent: HWND, hwnd: HWND, routing: KeyboardRouting) {
  attach_subclass(hwnd, parent, routing.clone());

  let data = KeyboardRoutingSubclassData { parent, routing };
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
      let kind = routing_kind(msg, wparam);
      let destination = route_destination(&data.routing.borrow(), wparam.0 as u32, kind);

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

fn route_destination(
  routing: &KeyboardEventRouting<u32>,
  virtual_key: u32,
  kind: KeyboardEventRoutingKind,
) -> KeyboardEventDestination {
  let modifiers = current_modifiers();
  routing
    .routes
    .iter()
    .find_map(|route| {
      (route.chord.key_code == virtual_key && modifiers_match(route.chord.modifiers, modifiers))
        .then_some(route.destination)
    })
    .unwrap_or_else(|| routing.defaults.destination_for(kind))
}

fn routing_kind(msg: u32, wparam: WPARAM) -> KeyboardEventRoutingKind {
  if matches!(msg, WM_SYSKEYDOWN | WM_SYSKEYUP)
    || current_modifiers().control
    || current_modifiers().alt
    || !maps_to_text_key(wparam.0 as u32)
  {
    KeyboardEventRoutingKind::Accelerator
  } else {
    KeyboardEventRoutingKind::KeyEvent
  }
}

fn current_modifiers() -> KeyboardEventModifiers {
  KeyboardEventModifiers {
    shift: key_is_down(0x10),
    control: key_is_down(0x11),
    alt: key_is_down(0x12),
    meta: key_is_down(0x5B) || key_is_down(0x5C),
    any: false,
  }
}

fn key_is_down(virtual_key: i32) -> bool {
  unsafe { GetKeyState(virtual_key) < 0 }
}

fn modifiers_match(expected: KeyboardEventModifiers, actual: KeyboardEventModifiers) -> bool {
  expected.any
    || (expected.shift == actual.shift
      && expected.control == actual.control
      && expected.alt == actual.alt
      && expected.meta == actual.meta)
}

fn maps_to_text_key(virtual_key: u32) -> bool {
  matches!(
    virtual_key,
    0x30..=0x39 // 0-9
      | 0x41..=0x5A // A-Z
      | 0x60..=0x6F // numpad digits and operators
      | 0x20 // Space
      | 0xBA..=0xC0 // OEM punctuation
      | 0xDB..=0xDF // OEM punctuation
      | 0xE2 // OEM 102
  )
}

unsafe extern "system" fn enum_child_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
  let data = &*(lparam.0 as *const KeyboardRoutingSubclassData);
  attach_subclass(hwnd, data.parent, data.routing.clone());
  true.into()
}

unsafe fn attach_subclass(hwnd: HWND, parent: HWND, routing: KeyboardRouting) {
  let _ = SetWindowSubclass(
    hwnd,
    Some(subclass_proc),
    KEYBOARD_ROUTING_SUBCLASS_ID as _,
    Box::into_raw(Box::new(KeyboardRoutingSubclassData { parent, routing })) as _,
  );
}
