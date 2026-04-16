use std::path::PathBuf;

/// WebContext configuration for wxp
///
/// Specifies the directory for storing WebView user data
/// (cache, cookies, local storage, etc.).
/// A `data_directory` is required because permission issues can arise
/// in plugin environments.
///
/// # Example
///
/// ```no_run
/// use wxp::WebContext;
///
/// let data_dir = std::env::temp_dir().join("my-plugin");
/// let web_context = WebContext::new(data_dir);
/// ```
#[derive(Debug, Clone)]
pub struct WebContext {
    data_directory: PathBuf,
}

impl WebContext {
    /// Creates a new WebContext.
    ///
    /// # Arguments
    ///
    /// * `data_directory` - Directory for storing WebView user data (required)
    pub fn new(data_directory: impl Into<PathBuf>) -> Self {
        Self {
            data_directory: data_directory.into(),
        }
    }

    /// Returns the path to the data directory
    pub fn data_directory(&self) -> &PathBuf {
        &self.data_directory
    }

    /// Creates a wry::WebContext from this configuration
    pub fn build_wry_context(&self) -> wry::WebContext {
        wry::WebContext::new(Some(self.data_directory.clone()))
    }
}
