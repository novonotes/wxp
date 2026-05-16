//! WXP command API layer.
//!
//! Registers and executes commands that accept `invoke()` calls from JavaScript.

mod async_command;
pub(crate) mod command;
pub(crate) mod context;
mod handler;
mod invoke;
pub(crate) mod setup;
mod unified;

pub use context::CommandContext;
#[doc(hidden)]
pub use context::TryFromDeserializeContext;
pub use handler::WxpCommandHandler;
