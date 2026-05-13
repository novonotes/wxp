use super::handler::WxpCommandHandler;
use novonotes_run_loop::RunLoop;
use std::rc::Rc;
use wry::{WebViewBuilder, http::Request};

pub(crate) fn setup_invoke_handler_internal(
    builder: WebViewBuilder,
    handler: Rc<WxpCommandHandler>,
) -> WebViewBuilder {
    // Set up the IPC handler
    builder.with_ipc_handler(move |req: Request<String>| {
        let handler = handler.clone();
        let body = req.body().clone();

        // Run async processing on the RunLoop
        let handle = RunLoop::current().spawn(async move {
            // handle_ipc directly executes JavaScript
            handler.handle_ipc(&body).await;
        });
        drop(handle); // Do not wait
    })
}
