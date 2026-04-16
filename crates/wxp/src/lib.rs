//! `wxp` は [`wry`](https://github.com/tauri-apps/wry) をベースに、Rust ↔ JavaScript の
//! 双方向通信を提供する WebView 統合クレートです。CLAP/VST3 などのオーディオプラグイン
//! GUI を WebView で構築することを主な用途としています。
//!
//! 使い方・サンプルコードは [README](https://github.com/novonotes/wxp) を参照してください。
//!
//! ## 注意点
//!
//! - WebView の構築・操作はメインスレッド限定です。[`WebViewRef`] は `Send + Sync` ですが、
//!   これは他スレッドの構造体に保持できるようにするためであり、別スレッドから操作してよいわけではありません。
//! - [`WebViewRef`] を全て drop すると WebView が破棄されます。表示を維持するには最低一つ保持してください。

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
