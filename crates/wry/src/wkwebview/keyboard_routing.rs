use objc2::{msg_send, runtime::Sel, DeclaredClass};
use objc2_app_kit::{NSEvent, NSEventModifierFlags};

use crate::wkwebview::class::wry_web_view::WryWebView;
use crate::{
  KeyboardEventDestination, KeyboardEventModifiers, KeyboardEventRouting, KeyboardEventRoutingKind,
};

pub(crate) fn set_routing(webview: &WryWebView, routing: KeyboardEventRouting<u16>) {
  *webview.ivars().keyboard_event_routing.lock().unwrap() = routing;
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
