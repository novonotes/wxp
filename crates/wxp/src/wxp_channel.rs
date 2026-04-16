//! Real-time push notification channel from Rust to JavaScript.
//!
//! Generate a `Channel` object on the JavaScript side and pass it to Rust as an argument to `invoke()`.
//! From the Rust side, call [`Channel::send`] at any time to deliver data to JS.

pub(crate) mod channel;
mod error;
pub(crate) mod internals;
pub(crate) mod try_from;

pub use channel::Channel;
