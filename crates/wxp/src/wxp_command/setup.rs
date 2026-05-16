use super::handler::WxpCommandHandler;
use novonotes_run_loop::RunLoop;
use std::rc::Rc;
use wry::{WebViewBuilder, http::Request};

/// Wires the JS `invoke()` bridge into the WebView.
///
/// wry's IPC callback is synchronous and runs on the run loop thread, but
/// commands may be async. We therefore spawn the work onto the run loop and
/// return immediately: blocking here would stall the WebView and, in plugin
/// hosts, risk deadlocking the host's UI thread. The response is delivered
/// later by `handle_ipc` evaluating JS back into the page.
pub(crate) fn setup_invoke_handler_internal(
    builder: WebViewBuilder,
    handler: Rc<WxpCommandHandler>,
) -> WebViewBuilder {
    builder.with_ipc_handler(move |req: Request<String>| {
        let handler = handler.clone();
        let body = req.body().clone();

        let handle = RunLoop::current().spawn(async move {
            handler.handle_ipc(&body).await;
        });
        // Detach: the command resolves the JS promise itself, so nothing here
        // needs the result and awaiting it would defeat the purpose.
        drop(handle);
    })
}
