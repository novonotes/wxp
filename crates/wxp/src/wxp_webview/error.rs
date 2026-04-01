use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("WebView error: {0}")]
    WebView(#[from] wry::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path not found: {0}")]
    PathNotFound(String),
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
