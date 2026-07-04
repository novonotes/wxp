use std::cell::Cell;

use objc2::{msg_send, runtime::Sel, DeclaredClass};
use objc2_app_kit::{NSEvent, NSEventModifierFlags};

use crate::wkwebview::class::wry_web_view::WryWebView;
use crate::{
  KeyboardEventDestination, KeyboardEventModifiers, KeyboardEventRouting, KeyboardEventRoutingKind,
};

pub(crate) fn set_routing(webview: &WryWebView, routing: KeyboardEventRouting<u16>) {
  *webview.ivars().keyboard_event_routing.lock().unwrap() = routing;
}

thread_local! {
  static PARENT_FORWARD_DEPTH: Cell<u32> = const { Cell::new(0) };
}

pub(crate) fn parent_forwarding_is_active() -> bool {
  PARENT_FORWARD_DEPTH.with(|depth| depth.get() > 0)
}

struct ParentForwardGuard;

impl ParentForwardGuard {
  fn enter() -> Self {
    PARENT_FORWARD_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
    Self
  }
}

impl Drop for ParentForwardGuard {
  fn drop(&mut self) {
    PARENT_FORWARD_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
  }
}

pub(crate) fn route_destination(
  webview: &WryWebView,
  event: &NSEvent,
  kind: KeyboardEventRoutingKind,
) -> KeyboardEventDestination {
  let key_code = event.keyCode();
  let modifiers = event_modifiers(event);
  let routing = webview.ivars().keyboard_event_routing.lock().unwrap();
  routing
    .routes
    .iter()
    .find_map(|route| {
      (route.chord.key_code == key_code && modifiers_match(route.chord.modifiers, modifiers))
        .then_some(route.destination)
    })
    .unwrap_or_else(|| routing.defaults.destination_for(kind))
}

pub(crate) fn handle_key_event(webview: &WryWebView, event: &NSEvent, selector: Sel) -> bool {
  // Some plugin hosts bounce a parent-routed keyDown back into the embedded WebView while the
  // original native event is still being forwarded. Treat that synchronous re-entry as handled so
  // the host receives the first event without letting AppKit recurse until the main stack overflows.
  if parent_forwarding_is_active() {
    return true;
  }

  let destination = route_destination(webview, event, KeyboardEventRoutingKind::KeyEvent);

  match destination {
    KeyboardEventDestination::WebView => false,
    KeyboardEventDestination::Parent => {
      forward_to_parent(webview, event, selector);
      true
    }
    KeyboardEventDestination::WebViewAndParent => {
      forward_to_parent(webview, event, selector);
      false
    }
  }
}

fn event_modifiers(event: &NSEvent) -> KeyboardEventModifiers {
  let flags = event.modifierFlags();
  KeyboardEventModifiers {
    shift: flags.contains(NSEventModifierFlags::Shift),
    control: flags.contains(NSEventModifierFlags::Control),
    alt: flags.contains(NSEventModifierFlags::Option),
    meta: flags.contains(NSEventModifierFlags::Command),
    any: false,
  }
}

fn modifiers_match(expected: KeyboardEventModifiers, actual: KeyboardEventModifiers) -> bool {
  expected.any
    || (expected.shift == actual.shift
      && expected.control == actual.control
      && expected.alt == actual.alt
      && expected.meta == actual.meta)
}

fn forward_to_parent(webview: &WryWebView, event: &NSEvent, selector: Sel) {
  let Some(parent) = (unsafe { webview.superview() }) else {
    return;
  };
  let target = if webview.ivars().is_child {
    match unsafe { parent.superview() } {
      Some(host_parent) => host_parent,
      None => parent.clone(),
    }
  } else {
    parent.clone()
  };

  // Child WebViews sit inside a lightweight wrapper. Parent-routed events must skip that wrapper
  // and continue to the host NSView, otherwise AppKit can re-enter our wrapper/WebView pair instead
  // of delivering parent-routed shortcuts to the embedding application.
  let _guard = ParentForwardGuard::enter();
  if let Some(window) = webview.window() {
    let _ = window.makeFirstResponder(Some(&target));
  }
  unsafe {
    let _: () = msg_send![&*target, performSelector: selector, withObject: event];
  }
  if let Some(window) = webview.window() {
    let _ = window.makeFirstResponder(Some(webview));
  }
}
