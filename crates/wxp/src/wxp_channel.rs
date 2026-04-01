//! Rust から JavaScript へのリアルタイムプッシュ通知チャネル。
//!
//! JavaScript 側で生成した `Channel` オブジェクトを `invoke()` の引数として Rust に渡し、
//! Rust 側から任意のタイミングで [`Channel::send`] を呼ぶことで JS にデータを送信できます。

pub(crate) mod channel;
mod error;
pub(crate) mod internals;
pub(crate) mod try_from;

pub use channel::Channel;
