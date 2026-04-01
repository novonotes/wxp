//! Tauri 互換のコマンド API レイヤー。
//!
//! JavaScript からの `invoke()` 呼び出しを受け付けるコマンドを登録・実行します。

mod async_command;
pub(crate) mod command;
pub(crate) mod context;
mod handler;
mod invoke;
pub(crate) mod setup;
mod unified;

pub use context::CommandContext;
pub use handler::WxpCommandHandler;
