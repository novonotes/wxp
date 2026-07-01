use objc2::{msg_send, runtime::Sel, DeclaredClass};
use objc2_app_kit::NSEvent;

use crate::wkwebview::class::wry_web_view::WryWebView;
use crate::KeyboardEventDestination;

pub(crate) fn set_routes(webview: &WryWebView, routes: Vec<(u16, KeyboardEventDestination)>) {
  *webview.ivars().keyboard_event_routes.lock().unwrap() = routes;
}

pub(crate) fn handle_key_event(webview: &WryWebView, event: &NSEvent, selector: Sel) -> bool {
  let key_code = event.keyCode();
  let destination = webview
    .ivars()
    .keyboard_event_routes
    .lock()
    .unwrap()
    .iter()
    .find_map(|(route_key_code, destination)| (*route_key_code == key_code).then_some(*destination))
    .unwrap_or(KeyboardEventDestination::WebView);

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

fn forward_to_parent(webview: &WryWebView, event: &NSEvent, selector: Sel) {
  let Some(parent) = (unsafe { webview.superview() }) else {
    return;
  };

  // Route the original native event before WebKit consumes it; synthetic JS forwarding cannot
  // participate in plugin-host accelerator handling.
  if let Some(window) = webview.window() {
    let _ = window.makeFirstResponder(Some(&parent));
  }
  unsafe {
    let _: () = msg_send![&*parent, performSelector: selector, withObject: event];
  }
  if let Some(window) = webview.window() {
    let _ = window.makeFirstResponder(Some(webview));
  }
}
