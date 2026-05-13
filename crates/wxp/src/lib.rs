//! `wxp` is a WebView integration crate based on [`wry`](https://github.com/tauri-apps/wry).
//! It provides bidirectional communication between Rust and JavaScript, primarily for building
//! CLAP/VST3 audio plugin GUIs with WebView technology.
//!
//! See the [README](https://github.com/novonotes/wxp) for usage and examples.
//!
//! ## Caveats
//!
//! - WebViews must be created and operated on the main thread. [`WebViewRef`] is `Send + Sync`
//!   so it can be stored in data structures owned by other threads, not so it can be operated
//!   from those threads.
//! - The WebView is destroyed when every [`WebViewRef`] is dropped. Keep at least one reference
//!   alive while the UI should remain visible.

mod builder;
mod initialization;
mod web_context;
mod webview_ref;
mod wxp_channel;
mod wxp_command;
mod wxp_webview;

// --------------------------------------------------
// Re-export wxp types
// --------------------------------------------------

pub use builder::WxpWebViewBuilder;
pub use web_context::WebContext;
pub use webview_ref::WebViewRef;
pub use wxp_channel::Channel;
#[doc(hidden)]
pub use wxp_command::TryFromDeserializeContext;
pub use wxp_command::{CommandContext, WxpCommandHandler};
pub use wxp_webview::error::{Error, Result};

// --------------------------------------------------
// Re-export types from wry
// --------------------------------------------------

pub use wry::Rect;
pub mod dpi {
    pub use wry::dpi::{LogicalPosition, LogicalSize, Position, Size};
}
pub mod raw_window_handle {
    pub use wry::raw_window_handle::{
        AppKitWindowHandle, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
        XcbWindowHandle,
    };
}
