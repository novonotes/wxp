use super::async_command::AsyncCommandFn;
use super::command::SyncCommandFn;
use super::context::CommandContext;
use super::invoke::{InvokeRequest, InvokeResponse};
use super::unified::DynUnifiedCommand;
use crate::webview_ref::WebViewRef;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

/// JavaScript からの `invoke()` 呼び出しを受け付けるコマンドを管理・実行するハンドラー。
///
/// [`register_sync`](Self::register_sync) / [`register_async`](Self::register_async) で
/// コマンドを登録し、[`WxpWebViewBuilder::with_command_handler`](crate::WxpWebViewBuilder::with_command_handler)
/// でビルダーに渡してください。
pub struct WxpCommandHandler {
    commands: Arc<RwLock<HashMap<String, DynUnifiedCommand>>>,
    webview: Arc<RwLock<Option<WebViewRef>>>,
}

impl WxpCommandHandler {
    /// 新しい `WxpCommandHandler` を作成する。
    pub fn new() -> Self {
        Self {
            commands: Arc::new(RwLock::new(HashMap::new())),
            webview: Arc::new(RwLock::new(None)),
        }
    }

    /// WebView を設定
    pub(crate) fn set_webview(&self, webview: WebViewRef) {
        *self.webview.write() = Some(webview);
    }

    /// 同期コマンドを登録
    pub fn register_sync<F, R, E>(&self, name: &str, handler: F) -> &Self
    where
        F: Fn(CommandContext<'_>) -> Result<R, E> + Send + Sync + 'static,
        R: serde::Serialize + Send + Sync + 'static,
        E: serde::Serialize + Send + Sync + 'static,
    {
        let command = SyncCommandFn::new(name, handler);
        let mut commands = self.commands.write();
        commands.insert(command.name().to_string(), Arc::new(command));
        self
    }

    /// クロージャから非同期コマンドを登録
    pub fn register_async<F, Fut, R, E>(&self, name: &str, handler: F) -> &Self
    where
        F: Fn(CommandContext<'_>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<R, E>> + Send + 'static,
        R: serde::Serialize + Send + Sync + 'static,
        E: serde::Serialize + Send + Sync + 'static,
    {
        let command = AsyncCommandFn::new(name, handler);
        let mut commands = self.commands.write();
        commands.insert(command.name().to_string(), Arc::new(command));
        self
    }

    /// invokeリクエストを処理
    async fn invoke(&self, request: InvokeRequest) -> InvokeResponse {
        let commands = self.commands.read();
        let webview = match self.webview.read().clone() {
            Some(wv) => wv,
            None => return InvokeResponse::error("WebView not set".to_string()),
        };

        match commands.get(&request.cmd) {
            Some(command) => {
                // CommandContext を作成
                let ctx = CommandContext::new(&request.cmd, &request.inner.args, webview);

                match command.execute(ctx).await {
                    Ok(value) => InvokeResponse::success(value),
                    Err(error) => InvokeResponse::error(error),
                }
            }
            None => InvokeResponse::error(format!("Command not found: {}", request.cmd)),
        }
    }

    /// IPCメッセージを処理してJavaScriptを実行
    pub(crate) async fn handle_ipc(&self, body: &str) {
        if let Ok(request) = serde_json::from_str::<InvokeRequest>(body) {
            let callback_id = request.callback;
            let error_id = request.error;
            let response = self.invoke(request).await;

            // WebViewが設定されていれば、JavaScriptを実行
            if let Some(webview) = self.webview.read().as_ref() {
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
                let _ = webview.evaluate_script(&js);
            }
        }
    }
}

impl Default for WxpCommandHandler {
    fn default() -> Self {
        Self::new()
    }
}
