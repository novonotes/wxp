use host_window::create_window;
use novonotes_run_loop::{RunLoop, test_harness};
use std::time::Duration;
use wxp::WebContext;
use wxp::dpi::{LogicalPosition, LogicalSize};
use wxp::{Rect, WxpWebViewBuilder};

fn main() {
    test_harness::run_gui_tests(vec![("basic WebView functionality", test_webview_basic)]);
}

fn test_webview_basic() -> Result<(), String> {
    use parking_lot::Mutex;
    use std::sync::Arc;

    // Struct to hold resources
    struct Resources {
        _window: host_window::HostWindowHandle,
        _webview: wxp::WebViewRef,
    }

    let resources = Arc::new(Mutex::new(None));
    let resources_clone = resources.clone();

    RunLoop::current()
        .schedule(Duration::ZERO, move || {
            let window_width = 600.0;
            let window_height = 400.0;
            let window = create_window("WebView Test", window_width, window_height);

            let wxp_context = WebContext::new(std::env::temp_dir().join("wxp-test"));
            let mut wry_context = wxp_context.build_wry_context();

            // Set bounds to match the parent window size
            let bounds = Rect {
                position: LogicalPosition::new(0.0, 0.0).into(),
                size: LogicalSize::new(window_width, window_height).into(),
            };

            let webview = WxpWebViewBuilder::new(&mut wry_context)
                .with_html(r#"<h1>WebView Test</h1>"#)
                .with_devtools(true)
                .with_bounds(bounds)
                .build_as_child(&window)
                .expect("Failed to create WebView");

            window.show();

            // Save resources
            *resources_clone.lock() = Some(Resources {
                _window: window,
                _webview: webview,
            });

            RunLoop::current()
                .schedule(Duration::from_millis(1000), || {
                    RunLoop::current().stop_app()
                })
                .detach();
        })
        .detach();

    RunLoop::current().run_app();
    Ok(())
}
