use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("WebView error: {0}")]
    WebView(#[from] wry::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Channel {0} not found")]
    ChannelNotFound(u32),

    #[error("Channel already closed")]
    ChannelClosed,

    #[error("Invalid channel ID format: {0}")]
    InvalidChannelId(String),
}

pub(super) type Result<T> = std::result::Result<T, Error>;
