use novonotes_run_loop::{RunLoop, RunLoopSender};
use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Weak};
use wry::WebView;

use crate::{Error, Rect, Result};

/// UI-thread owner of a native WebView.
///
/// `WxpWebView` owns the native WebView lifetime and is intentionally `!Send + !Sync`.
/// Cloneable, cross-thread operations are exposed through [`WebViewDispatch`].
pub struct WxpWebView {
    inner: Arc<SendWrapper<RefCell<WebView>>>,
    sender: RunLoopSender,
    _not_send_sync: PhantomData<Rc<()>>,
}

/// Thread-safe handle for posting operations to a [`WxpWebView`].
///
/// This handle does not keep the WebView alive. If the owner has been dropped, post methods return
/// [`Error::WebViewClosed`]. A successful post means the operation was accepted for dispatch, not
/// that the native WebView operation has completed. The native WebView stays private so callers can
/// keep this handle across threads without gaining direct access to the UI-thread-only object.
#[derive(Clone)]
pub struct WebViewDispatch {
    inner: Weak<SendWrapper<RefCell<WebView>>>,
    sender: RunLoopSender,
}

impl std::fmt::Debug for WxpWebView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WxpWebView").finish()
    }
}

impl std::fmt::Debug for WebViewDispatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebViewDispatch").finish()
    }
}

impl WxpWebView {
    /// Creates a new WebView owner.
    pub(crate) fn new(webview: WebView) -> Result<Self> {
        RunLoop::try_current().map_err(|_| Error::RunLoopNotInitialized)?;
        Ok(Self {
            inner: Arc::new(SendWrapper::new(RefCell::new(webview))),
            sender: RunLoop::sender(),
            _not_send_sync: PhantomData,
        })
    }

    /// Returns a thread-safe dispatch handle for this WebView.
    pub fn dispatch(&self) -> WebViewDispatch {
        WebViewDispatch {
            inner: Arc::downgrade(&self.inner),
            sender: self.sender.clone(),
        }
    }
}

impl WebViewDispatch {
    /// Posts JavaScript evaluation to the WebView's run loop.
    pub fn post_eval_script(&self, script: impl Into<String>) -> Result<()> {
        let script = script.into();
        self.post_webview_op("evaluate_script", move |webview| {
            webview.evaluate_script(&script)
        })
    }

    /// Posts a bounds update to the WebView's run loop.
    pub fn post_set_bounds(&self, bounds: Rect) -> Result<()> {
        self.post_webview_op("set_bounds", move |webview| webview.set_bounds(bounds))
    }

    /// Posts a visibility update to the WebView's run loop.
    pub fn post_set_visible(&self, visible: bool) -> Result<()> {
        self.post_webview_op("set_visible", move |webview| webview.set_visible(visible))
    }

    /// Posts a request to move focus back to the parent window.
    pub fn post_focus_parent(&self) -> Result<()> {
        self.post_webview_op("focus_parent", move |webview| webview.focus_parent())
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.inner.strong_count() > 0
    }

    pub(crate) fn post_eval_script_or_else<C>(
        &self,
        script: impl Into<String>,
        on_webview_closed: C,
    ) -> Result<()>
    where
        C: FnOnce() + Send + 'static,
    {
        let script = script.into();
        self.post_webview_op_or_else(
            "evaluate_script",
            move |webview| webview.evaluate_script(&script),
            on_webview_closed,
        )
    }

    fn post_webview_op<F>(&self, operation: &'static str, op: F) -> Result<()>
    where
        F: FnOnce(&WebView) -> std::result::Result<(), wry::Error> + Send + 'static,
    {
        self.post_webview_op_or_else(operation, op, || {})
    }

    fn post_webview_op_or_else<F, C>(
        &self,
        operation: &'static str,
        op: F,
        on_webview_closed: C,
    ) -> Result<()>
    where
        F: FnOnce(&WebView) -> std::result::Result<(), wry::Error> + Send + 'static,
        C: FnOnce() + Send + 'static,
    {
        if !self.is_alive() {
            on_webview_closed();
            return Err(Error::WebViewClosed);
        }

        let inner = self.inner.clone();
        let run = move || {
            let Some(webview) = inner.upgrade() else {
                // The owner can be dropped after a cross-thread post was accepted but before the
                // run loop executes it. Let callers clean up side data for that abandoned post.
                on_webview_closed();
                return;
            };
            if let Err(error) = op(&webview.borrow()) {
                log::error!("wxp WebView {operation} failed: {error}");
            }
        };

        if self.sender.is_same_thread() {
            run();
        } else {
            self.sender.send(run);
        }

        Ok(())
    }
}
