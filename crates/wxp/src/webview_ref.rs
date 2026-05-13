use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::sync::{Arc, Weak};
use wry::WebView;

use crate::{Rect, Result};

/// Struct for managing a reference to a WebView
///
/// [`WebViewRef`] is Send + Sync, but must only be accessed from the MainThread.
/// The reason for making it Send + Sync is to allow it to be held as a member
/// variable in structs that are temporarily moved to an audio thread,
/// such as audio plugin instances.
///
/// Lifetime management:
/// When all [`WebViewRef`] instances are dropped, the WebView is also destroyed
/// and the content in the window will no longer be displayed.
/// To keep the WebView visible, at least one [`WebViewRef`] must be held somewhere.
///
#[derive(Clone)]
pub struct WebViewRef {
    inner: Arc<SendWrapper<RefCell<WebView>>>,
}

impl std::fmt::Debug for WebViewRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebViewRef").finish()
    }
}

impl WebViewRef {
    /// Creates a new WebViewRef
    pub(crate) fn new(webview: WebView) -> Self {
        Self {
            inner: Arc::new(SendWrapper::new(RefCell::new(webview))),
        }
    }

    /// Evaluates JavaScript
    pub fn evaluate_script(&self, script: &str) -> Result<()> {
        self.inner.borrow().evaluate_script(script)?;
        Ok(())
    }

    /// Sets the bounds of the WebView
    pub fn set_bounds(&self, bounds: Rect) -> Result<()> {
        self.inner.borrow().set_bounds(bounds)?;
        Ok(())
    }

    /// Moves keyboard focus away from the WebView back to its parent.
    pub fn focus_parent(&self) -> Result<()> {
        self.inner.borrow().focus_parent()?;
        Ok(())
    }

    /// Returns a weak reference (for internal use)
    pub(crate) fn downgrade(&self) -> Weak<SendWrapper<RefCell<WebView>>> {
        Arc::downgrade(&self.inner)
    }
}
