use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("WebView error: {0}")]
    WebView(String),

    #[error("WebView is closed")]
    WebViewClosed,

    #[error("RunLoop is not initialized on the current thread")]
    RunLoopNotInitialized,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path not found: {0}")]
    PathNotFound(String),
}

impl From<wry::Error> for Error {
    fn from(value: wry::Error) -> Self {
        Self::WebView(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
