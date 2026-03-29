use super::handler::WxpCommandHandler;
use novonotes_run_loop::RunLoop;
use std::sync::Arc;
use wry::{WebViewBuilder, http::Request};

pub(crate) fn setup_invoke_handler_internal(
    builder: WebViewBuilder,
    handler: Arc<WxpCommandHandler>,
) -> WebViewBuilder {
    // IPCハンドラーを設定
    builder.with_ipc_handler(move |req: Request<String>| {
        let handler = handler.clone();
        let body = req.body().clone();

        // RunLoopで非同期処理を実行
        let handle = RunLoop::current().spawn(async move {
            // handle_ipcメソッドが直接JavaScriptを実行する
            handler.handle_ipc(&body).await;
        });
        drop(handle); // 待機しない
    })
}
