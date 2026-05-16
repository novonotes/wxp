use thiserror::Error;

/// Errors returned by [`Channel`](super::Channel) operations.
#[derive(Error, Debug)]
pub enum Error {
    /// An underlying native WebView failure (wry error, flattened to its text).
    #[error("WebView error: {0}")]
    WebView(String),

    /// The target page/WebView is gone. Expected when a channel outlives its
    /// page; callers normally stop sending rather than treating it as fatal.
    #[error("WebView is closed")]
    WebViewClosed,

    /// The payload could not be serialized to JSON for delivery.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// No channel is registered for this id (e.g. JS already closed it).
    #[error("Channel {0} not found")]
    ChannelNotFound(u32),

    /// Operation attempted on a channel that was already closed/ended.
    #[error("Channel already closed")]
    ChannelClosed,

    /// The JS-supplied channel token did not match the expected
    /// `__CHANNEL__:<id>` format — usually a non-`Channel` value was passed to
    /// an `invoke` argument typed as a channel.
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
            // Preserve closure as a typed channel error so senders can distinguish a closed WebView
            // from native WebView failures or serialization bugs.
            crate::Error::WebViewClosed => Self::WebViewClosed,
            other => Self::WebView(other.to_string()),
        }
    }
}

pub(super) type Result<T> = std::result::Result<T, Error>;
