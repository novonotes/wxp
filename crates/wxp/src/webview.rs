use novonotes_run_loop::{RunLoop, RunLoopGuard};
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
    // The native WebView still lives behind SendWrapper because WebViewDispatch must be able to
    // carry a weak handle across threads. Direct access remains confined to this module.
    inner: Arc<SendWrapper<RefCell<WebView>>>,
    // Keep the process-wide run loop bound while the native WebView exists. Field order is
    // intentional: `inner` must be dropped before this guard releases the run loop binding.
    _run_loop_guard: RunLoopGuard,
    // Keep the owner !Send + !Sync so native WebView lifetime never moves away from the UI thread.
    _not_send_sync: PhantomData<Rc<()>>,
}

/// Thread-safe handle for posting operations to a [`WxpWebView`].
///
/// This handle does not keep the WebView alive. If the owner has been dropped, post methods return
/// [`Error::WebViewClosed`]. A successful post means the operation was accepted for dispatch, not
/// that the native WebView operation has completed. The native WebView stays private so callers can
/// keep this handle across threads without gaining direct access to the UI-thread-only object.
///
/// The API is intentionally post/enqueue based instead of blocking for completion. Plugin hosts can
/// call plugin code while also waiting on their UI thread, so a send-and-wait WebView API would make
/// host-driven deadlocks easy to create.
#[derive(Clone)]
pub struct WebViewDispatch {
    // Weak ownership keeps this handle Send + Sync without making it a hidden lifetime owner.
    inner: Weak<SendWrapper<RefCell<WebView>>>,
}

struct AbandonedPostCleanup<C: FnOnce() + Send + 'static> {
    callback: Option<C>,
}

impl<C: FnOnce() + Send + 'static> AbandonedPostCleanup<C> {
    fn new(callback: C) -> Self {
        Self {
            callback: Some(callback),
        }
    }

    fn run(&mut self) {
        if let Some(callback) = self.callback.take() {
            callback();
        }
    }

    fn disarm(&mut self) {
        self.callback = None;
    }
}

impl<C: FnOnce() + Send + 'static> Drop for AbandonedPostCleanup<C> {
    fn drop(&mut self) {
        self.run();
    }
}

const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    let _ = assert_send_sync::<WebViewDispatch>;
};

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
        if !RunLoop::is_run_loop_thread() {
            return Err(Error::RunLoopNotInitialized);
        }
        let run_loop_guard = RunLoop::init().map_err(|_| Error::RunLoopNotInitialized)?;
        Ok(Self {
            inner: Arc::new(SendWrapper::new(RefCell::new(webview))),
            _run_loop_guard: run_loop_guard,
            _not_send_sync: PhantomData,
        })
    }

    /// Returns a thread-safe dispatch handle for this WebView.
    pub fn dispatch(&self) -> WebViewDispatch {
        WebViewDispatch {
            // Dispatch handles intentionally do not prolong the native WebView lifetime.
            inner: Arc::downgrade(&self.inner),
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn set_parent_keyboard_passthrough_key_codes(
        &self,
        key_codes: Vec<u16>,
    ) -> Result<()> {
        use wry::WebViewExtMacOS;

        // Keep raw native access inside this owner so wxp can expose a stable routing policy
        // without leaking wry's platform extension traits through its public API.
        self.inner
            .borrow()
            .set_parent_keyboard_passthrough_key_codes(key_codes)
            .map_err(Into::into)
    }

    #[cfg(windows)]
    pub(crate) fn set_parent_keyboard_passthrough_virtual_keys(
        &self,
        virtual_keys: Vec<u32>,
    ) -> Result<()> {
        use wry::WebViewExtWindows;

        // WebView2 creation can complete after this call, so the wry backend stores the policy
        // and installs it once the controller and child HWNDs exist.
        self.inner
            .borrow()
            .set_parent_keyboard_passthrough_virtual_keys(virtual_keys)
            .map_err(Into::into)
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
        // This is a gate for stale commands, not a lifetime reservation; posted work must still
        // tolerate the owner disappearing before the run loop executes it.
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
            // Run cleanup synchronously when the owner is already gone, so callers do not leave
            // side data behind for a post that will never be queued.
            on_webview_closed();
            return Err(Error::WebViewClosed);
        }

        let inner = self.inner.clone();
        let run = move || {
            let mut cleanup = AbandonedPostCleanup::new(on_webview_closed);
            let Some(webview) = inner.upgrade() else {
                // The owner can be dropped after a cross-thread post was accepted but before the
                // run loop executes it. Let callers clean up side data for that abandoned post.
                cleanup.run();
                return;
            };
            if let Err(error) = op(&webview.borrow()) {
                log::error!("wxp WebView {operation} failed: {error}");
            }
            cleanup.disarm();
        };

        if RunLoop::is_run_loop_thread() {
            // Running inline preserves same-thread WebView error reporting and avoids queueing work
            // behind the command currently servicing the WebView.
            run();
        } else {
            RunLoop::post(move |_| run()).map_err(|_| Error::RunLoopNotInitialized)?;
        }

        Ok(())
    }
}
