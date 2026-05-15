use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("WebView error: {0}")]
    WebView(String),

    #[error("WebView is closed")]
    WebViewClosed,

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Channel {0} not found")]
    ChannelNotFound(u32),

    #[error("Channel already closed")]
    ChannelClosed,

    #[error("Invalid channel ID format: {0}")]
    InvalidChannelId(String),
}

impl From<wry::Error> for Error {
    fn from(value: wry::Error) -> Self {
        Self::WebView(value.to_string())
    }
}

impl From<crate::Error> for Error {
    fn from(value: crate::Error) -> Self {
        match value {
            crate::Error::WebViewClosed => Self::WebViewClosed,
            other => Self::WebView(other.to_string()),
        }
    }
}

pub(super) type Result<T> = std::result::Result<T, Error>;
