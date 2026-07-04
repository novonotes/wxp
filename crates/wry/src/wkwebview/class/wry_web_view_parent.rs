// Copyright 2020-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

#[cfg(target_os = "macos")]
use objc2::DefinedClass;
use objc2::{define_class, msg_send, rc::Retained, runtime::Bool, MainThreadOnly};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplication, NSEvent, NSView, NSWindow, NSWindowButton};
use objc2_foundation::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_foundation::NSRect;
#[cfg(target_os = "ios")]
use objc2_ui_kit::UIView as NSView;
#[cfg(target_os = "macos")]
use std::cell::RefCell;

#[cfg(target_os = "macos")]
use super::wry_web_view::WryWebView;

pub struct WryWebViewParentIvars {
  #[cfg(target_os = "macos")]
  traffic_light_inset: std::cell::Cell<Option<(f64, f64)>>,
  #[cfg(target_os = "macos")]
  embedded_webview: RefCell<Option<Retained<WryWebView>>>,
}

define_class!(
  #[unsafe(super(NSView))]
  #[ivars = WryWebViewParentIvars]
  pub struct WryWebViewParent;

  /// Overridden NSView methods.
  impl WryWebViewParent {
    #[cfg(target_os = "macos")]
    #[unsafe(method(keyDown:))]
    fn key_down(&self, event: &NSEvent) {
      if self.ivars().embedded_webview.borrow().is_some() {
        // Child WebViews route parent-bound key events from the WebView itself. The wrapper must
        // not forward arbitrary keyDown events, because doing so can re-enter AppKit routing while
        // the WebView is still processing the original native event.
        unsafe { msg_send![super(self), keyDown: event] }
        return;
      }

      let mtm = MainThreadMarker::new().unwrap();
      let app = NSApplication::sharedApplication(mtm);
      if let Some(menu) = app.mainMenu() {
        menu.performKeyEquivalent(event);
      }
    }

    #[cfg(target_os = "macos")]
    #[unsafe(method(keyUp:))]
    fn key_up(&self, event: &NSEvent) {
      unsafe { msg_send![super(self), keyUp: event] }
    }

    #[cfg(target_os = "macos")]
    #[unsafe(method(performKeyEquivalent:))]
    fn perform_key_equivalent(&self, event: &NSEvent) -> Bool {
      let embedded_webview = self.ivars().embedded_webview.borrow();
      if let Some(webview) = embedded_webview.as_ref() {
        let destination = crate::wkwebview::keyboard_routing::route_destination(
          webview,
          event,
          crate::KeyboardEventRoutingKind::Accelerator,
        );

        match destination {
          crate::KeyboardEventDestination::WebView => {
            // AppKit sends command-key shortcuts through performKeyEquivalent before keyDown.
            // Send explicitly-routed WebView shortcuts back through the normal keyDown path so
            // JavaScript receives the same event shape as non-command keyboard shortcuts.
            unsafe {
              let _: () = msg_send![&**webview, keyDown: event];
            }
            return Bool::YES;
          }
          crate::KeyboardEventDestination::WebViewAndParent => {
            // This mode intentionally duplicates handling between the WebView and host parent.
            unsafe {
              let _: () = msg_send![&**webview, keyDown: event];
            }
          }
          crate::KeyboardEventDestination::Parent => {}
        }
      }

      unsafe { msg_send![super(self), performKeyEquivalent: event] }
    }

    #[cfg(target_os = "macos")]
    #[unsafe(method(drawRect:))]
    fn draw(&self, _dirty_rect: NSRect) {
      if let Some((x, y)) = self.ivars().traffic_light_inset.get() {
        unsafe { inset_traffic_lights(&self.window().unwrap(), x, y) };
      }
    }
  }
);

impl WryWebViewParent {
  #[allow(dead_code)]
  pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
    let delegate = WryWebViewParent::alloc(mtm).set_ivars(WryWebViewParentIvars {
      #[cfg(target_os = "macos")]
      traffic_light_inset: Default::default(),
      #[cfg(target_os = "macos")]
      embedded_webview: Default::default(),
    });
    unsafe { msg_send![super(delegate), init] }
  }

  #[cfg(target_os = "macos")]
  pub(crate) fn set_embedded_webview(&self, webview: Retained<WryWebView>) {
    self.ivars().embedded_webview.replace(Some(webview));
  }

  #[cfg(target_os = "macos")]
  pub fn set_traffic_light_inset(&self, ns_window: &NSWindow, position: dpi::Position) {
    let scale_factor = NSWindow::backingScaleFactor(ns_window);
    let position = position.to_logical(scale_factor);
    self
      .ivars()
      .traffic_light_inset
      .replace(Some((position.x, position.y)));

    unsafe {
      inset_traffic_lights(ns_window, position.x, position.y);
    }
  }
}

#[cfg(target_os = "macos")]
pub unsafe fn inset_traffic_lights(window: &NSWindow, x: f64, y: f64) {
  let Some(close) = window.standardWindowButton(NSWindowButton::CloseButton) else {
    #[cfg(feature = "tracing")]
    tracing::warn!("skipping inset_traffic_lights, close button not found");
    return;
  };
  let Some(miniaturize) = window.standardWindowButton(NSWindowButton::MiniaturizeButton) else {
    #[cfg(feature = "tracing")]
    tracing::warn!("skipping inset_traffic_lights, miniaturize button not found");
    return;
  };
  let zoom = window.standardWindowButton(NSWindowButton::ZoomButton);

  let title_bar_container_view = close.superview().unwrap().superview().unwrap();

  let close_rect = NSView::frame(&close);
  let title_bar_frame_height = close_rect.size.height + y;
  let mut title_bar_rect = NSView::frame(&title_bar_container_view);
  title_bar_rect.size.height = title_bar_frame_height;
  title_bar_rect.origin.y = window.frame().size.height - title_bar_frame_height;
  title_bar_container_view.setFrame(title_bar_rect);

  let space_between = NSView::frame(&miniaturize).origin.x - close_rect.origin.x;

  let mut window_buttons = vec![close, miniaturize];
  if let Some(zoom) = zoom {
    window_buttons.push(zoom);
  }

  for (i, button) in window_buttons.into_iter().enumerate() {
    let mut rect = NSView::frame(&button);
    rect.origin.x = x + (i as f64 * space_between);
    button.setFrameOrigin(rect.origin);
  }
}
