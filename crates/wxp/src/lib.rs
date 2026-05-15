//! `wxp` is a WebView integration crate based on [`wry`](https://github.com/tauri-apps/wry).
//! It provides bidirectional communication between Rust and JavaScript, primarily for building
//! CLAP/VST3 audio plugin GUIs with WebView technology.
//!
//! See the [README](https://github.com/novonotes/wxp) for usage and examples.
//!
//! ## Caveats
//!
//! - WebViews must be created and destroyed on the run loop thread.
//! - [`WxpWebView`] owns the native WebView lifetime and is intentionally not `Send`/`Sync`.
//! - [`WebViewDispatch`] is the cloneable, thread-safe handle for posting WebView operations.

mod builder;
mod initialization;
mod web_context;
mod webview;
mod wxp_channel;
mod wxp_command;
mod wxp_webview;

// --------------------------------------------------
// Re-export wxp types
// --------------------------------------------------

pub use builder::WxpWebViewBuilder;
pub use web_context::WebContext;
pub use webview::{WebViewDispatch, WxpWebView};
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
