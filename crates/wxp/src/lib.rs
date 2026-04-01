// wxp - Webview X Plugin

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
pub use wxp_command::{CommandContext, WxpCommandHandler};

// --------------------------------------------------
// Re-export types from wry
// --------------------------------------------------

pub use wry::Rect;
pub mod dpi {
    pub use wry::dpi::{LogicalPosition, LogicalSize, Size};
}
pub mod raw_window_handle {
    pub use wry::raw_window_handle::{
        AppKitWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle, XcbWindowHandle,
    };
}
