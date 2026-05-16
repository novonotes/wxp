use thiserror::Error;

/// Errors surfaced by the wxp WebView layer.
#[derive(Error, Debug)]
pub enum Error {
    /// An underlying `wry` failure, flattened to its message (wry's error type
    /// is not re-exported, so callers match on this variant, not on wry's).
    #[error("WebView error: {0}")]
    WebView(String),

    /// An operation was attempted after the WebView was dropped/closed. Expected
    /// during teardown races; treat it as "UI is gone", not as a bug.
    #[error("WebView is closed")]
    WebViewClosed,

    /// A WebView API was called from a thread without an initialized run loop.
    /// WebViews are thread-affine, so this almost always means the call is on
    /// the wrong thread.
    #[error("RunLoop is not initialized on the current thread")]
    RunLoopNotInitialized,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// A required asset/path (e.g. the data directory) does not exist.
    #[error("Path not found: {0}")]
    PathNotFound(String),
}

impl From<wry::Error> for Error {
    fn from(value: wry::Error) -> Self {
        Self::WebView(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
