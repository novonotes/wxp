use std::path::PathBuf;

/// WebContext for wxp.
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
#[derive(Debug)]
pub struct WebContext {
    data_directory: PathBuf,
    wry_context: wry::WebContext,
}

impl WebContext {
    /// Creates a new WebContext.
    ///
    /// # Arguments
    ///
    /// * `data_directory` - Directory for storing WebView user data (required)
    pub fn new(data_directory: impl Into<PathBuf>) -> Self {
        let data_directory = data_directory.into();
        let wry_context = wry::WebContext::new(Some(data_directory.clone()));
        Self {
            data_directory,
            wry_context,
        }
    }

    /// Returns the path to the data directory.
    pub fn data_directory(&self) -> &PathBuf {
        &self.data_directory
    }

    pub(crate) fn wry_context_mut(&mut self) -> &mut wry::WebContext {
        &mut self.wry_context
    }
}
