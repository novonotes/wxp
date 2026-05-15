use super::async_command::AsyncCommandFn;
use super::command::SyncCommandFn;
use super::context::CommandContext;
use super::invoke::{InvokeRequest, InvokeResponse};
use super::unified::DynUnifiedCommand;
use crate::WebViewDispatch;
use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;

/// A handler that manages and executes commands accepting `invoke()` calls from JavaScript.
///
/// Register commands with [`register_sync`](Self::register_sync) / [`register_async`](Self::register_async),
/// then pass it to the builder with
/// [`WxpWebViewBuilder::with_command_handler`](crate::WxpWebViewBuilder::with_command_handler).
///
/// Can be shared across multiple locations as `Rc<WxpCommandHandler>`.
/// Internally holds a [`WebViewDispatch`] that does not extend the WebView lifetime.
///
/// Command closures are executed on the run loop thread.
pub struct WxpCommandHandler {
    commands: RefCell<HashMap<String, DynUnifiedCommand>>,
    webview: RefCell<Option<WebViewDispatch>>,
}

impl WxpCommandHandler {
    /// Creates a new `WxpCommandHandler`.
    pub fn new() -> Self {
        Self {
            commands: RefCell::new(HashMap::new()),
            webview: RefCell::new(None),
        }
    }

    /// Sets the WebView dispatch handle.
    pub(crate) fn set_webview(&self, webview: WebViewDispatch) {
        *self.webview.borrow_mut() = Some(webview);
    }

    /// Registers a synchronous command
    pub fn register_sync<F, R, E>(&self, name: &str, handler: F) -> &Self
    where
        F: Fn(CommandContext<'_>) -> Result<R, E> + 'static,
        R: serde::Serialize + 'static,
        E: serde::Serialize + 'static,
    {
        let command = SyncCommandFn::new(name, handler);
        let mut commands = self.commands.borrow_mut();
        commands.insert(command.name().to_string(), std::rc::Rc::new(command));
        self
    }

    /// Registers an async command from a closure
    pub fn register_async<F, Fut, R, E>(&self, name: &str, handler: F) -> &Self
    where
        F: Fn(CommandContext<'_>) -> Fut + 'static,
        Fut: Future<Output = Result<R, E>> + 'static,
        R: serde::Serialize + 'static,
        E: serde::Serialize + 'static,
    {
        let command = AsyncCommandFn::new(name, handler);
        let mut commands = self.commands.borrow_mut();
        commands.insert(command.name().to_string(), std::rc::Rc::new(command));
        self
    }

    /// Processes an invoke request
    async fn invoke(&self, request: InvokeRequest) -> InvokeResponse {
        let command = self.commands.borrow().get(&request.cmd).cloned();
        let webview = match self.webview.borrow().as_ref().cloned() {
            Some(webview) => webview,
            None => return InvokeResponse::error("WebView no longer exists".to_string()),
        };

        match command {
            Some(command) => {
                // Create a CommandContext
                let ctx = CommandContext::new(&request.cmd, &request.inner.args, webview);

                match command.execute(ctx).await {
                    Ok(value) => InvokeResponse::success(value),
                    Err(error) => InvokeResponse::error(error),
                }
            }
            None => InvokeResponse::error(format!("Command not found: {}", request.cmd)),
        }
    }

    /// Processes an IPC message and executes JavaScript
    pub(crate) async fn handle_ipc(&self, body: &str) {
        if let Ok(request) = serde_json::from_str::<InvokeRequest>(body) {
            let callback_id = request.callback;
            let error_id = request.error;
            let response = self.invoke(request).await;

            // The WebView may have been destroyed while an async command was running. In that
            // case there is no page left to receive the response, so dropping it is the only
            // meaningful behavior.
            if let Some(webview) = self.webview.borrow().as_ref().cloned() {
                let js = match (response.value, response.error) {
                    (Some(value), None) => format!(
                        "window.__WXP_INTERNALS__.invoke[{}]({})",
                        callback_id,
                        serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string())
                    ),
                    (None, Some(error)) => format!(
                        "window.__WXP_INTERNALS__.invoke[{}](new Error({}))",
                        error_id,
                        serde_json::to_string(&error)
                            .unwrap_or_else(|_| "\"Unknown error\"".to_string())
                    ),
                    _ => return,
                };
                let _ = webview.post_eval_script(js);
            }
        }
    }
}

impl Default for WxpCommandHandler {
    fn default() -> Self {
        Self::new()
    }
}
