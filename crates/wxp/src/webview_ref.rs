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

/// Weak reference to a WebView.
///
/// Use this when a component needs temporary access to a WebView without
/// participating in its lifetime ownership. Command routing is one such case:
/// the runtime that embeds the WebView should own the lifetime, while handlers
/// only need to touch it while a command is actively being processed.
#[derive(Clone)]
pub struct WebViewWeakRef {
    inner: Weak<SendWrapper<RefCell<WebView>>>,
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

    /// Sets whether the WebView is visible.
    ///
    /// This is intentionally separate from dropping the last [`WebViewRef`].
    /// Hosts such as audio plugin wrappers can hide/show an embedded view many
    /// times within one GUI session, and destroying the WebView for every hide
    /// loses browser state and complicates parent-window ownership.
    pub fn set_visible(&self, visible: bool) -> Result<()> {
        self.inner.borrow().set_visible(visible)?;
        Ok(())
    }

    /// Moves keyboard focus away from the WebView back to its parent.
    pub fn focus_parent(&self) -> Result<()> {
        self.inner.borrow().focus_parent()?;
        Ok(())
    }

    /// Returns a weak reference.
    pub fn downgrade(&self) -> WebViewWeakRef {
        WebViewWeakRef {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

impl WebViewWeakRef {
    /// Attempts to upgrade the weak reference to a strong [`WebViewRef`].
    pub fn upgrade(&self) -> Option<WebViewRef> {
        self.inner.upgrade().map(|inner| WebViewRef { inner })
    }

    pub(crate) fn into_inner(self) -> Weak<SendWrapper<RefCell<WebView>>> {
        self.inner
    }
}
