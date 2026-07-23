use std::cell::Cell;

use objc2::{msg_send, runtime::Sel, DeclaredClass};
use objc2_app_kit::{NSEvent, NSEventModifierFlags};

use crate::wkwebview::class::wry_web_view::WryWebView;
use crate::{
  KeyboardAcceleratorDestination, KeyboardEventDestination, KeyboardEventModifiers,
  KeyboardEventRouting,
};

pub(crate) fn set_routing(webview: &WryWebView, routing: KeyboardEventRouting<u16>) {
  *webview.ivars().keyboard_event_routing.lock().unwrap() = routing;
}

thread_local! {
  // Parent forwarding happens on the AppKit main thread, but keeping the guard thread-local makes
  // the contract explicit: only synchronous native event re-entry on the same thread is suppressed.
  // Later asynchronous key events must be routed normally so host shortcuts continue to work.
  static PARENT_FORWARD_DEPTH: Cell<u32> = const { Cell::new(0) };
}

pub(crate) fn parent_forwarding_is_active() -> bool {
  PARENT_FORWARD_DEPTH.with(|depth| depth.get() > 0)
}

struct ParentForwardGuard;

impl ParentForwardGuard {
  fn enter() -> Self {
    // Use a depth counter instead of a boolean because host code may synchronously call through
    // another forwarding path before unwinding. The outermost guard owns restoration.
    PARENT_FORWARD_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
    Self
  }
}

impl Drop for ParentForwardGuard {
  fn drop(&mut self) {
    PARENT_FORWARD_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
  }
}

pub(crate) fn route_accelerator(
  webview: &WryWebView,
  event: &NSEvent,
) -> KeyboardAcceleratorDestination {
  let key_code = event.keyCode();
  let modifiers = event_modifiers(event);
  let routing = webview.ivars().keyboard_event_routing.lock().unwrap();
  routing
    .accelerator_routes
    .iter()
    .find_map(|route| {
      (route.chord.key_code == key_code && modifiers_match(route.chord.modifiers, modifiers))
        .then_some(route.destination)
    })
    .unwrap_or(routing.accelerator_default)
}

pub(crate) fn standard_editing_action(event: &NSEvent) -> Option<Sel> {
  let modifiers = event.modifierFlags();
  if modifiers.0 & NSEventModifierFlags::DeviceIndependentFlagsMask.0
    != NSEventModifierFlags::Command.0
  {
    return None;
  }

  // WebKit's standard editing commands must stay on the responder-action path because forwarding
  // them as keyDown alone does not execute the browser's default select/copy/paste/cut behavior.
  match event.charactersIgnoringModifiers()?.to_string().as_str() {
    "a" => Some(objc2::sel!(selectAll:)),
    "c" => Some(objc2::sel!(copy:)),
    "v" => Some(objc2::sel!(paste:)),
    "x" => Some(objc2::sel!(cut:)),
    _ => None,
  }
}

pub(crate) fn handle_key_event(webview: &WryWebView, event: &NSEvent, selector: Sel) -> bool {
  // Some plugin hosts bounce a parent-routed keyDown back into the embedded WebView while the
  // original native event is still being forwarded. Treat that synchronous re-entry as handled so
  // the host receives the first event without letting AppKit recurse until the main stack overflows.
  // Do not route this nested event to either side; it is the same native dispatch returning through
  // the host, not a new user action.
  if parent_forwarding_is_active() {
    return true;
  }

  let key_code = event.keyCode();
  let modifiers = event_modifiers(event);
  let routing = webview.ivars().keyboard_event_routing.lock().unwrap();
  let destination = routing
    .key_event_routes
    .iter()
    .find_map(|route| {
      (route.chord.key_code == key_code && modifiers_match(route.chord.modifiers, modifiers))
        .then_some(route.destination)
    })
    .unwrap_or(routing.key_event_default);
  drop(routing);

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

  // Keep the guard around the whole responder swap and selector call. Some hosts forward keyDown
  // back to the current first responder before this function unwinds, so ending the guard before
  // restoring focus would reopen the recursion path that caused stack-overflow crashes.
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
