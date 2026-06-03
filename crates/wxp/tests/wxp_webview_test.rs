use host_window::create_window;
use novonotes_run_loop::{RunLoop, RunLoopLocal, test_harness};
use std::time::Duration;
use wxp::WebContext;
use wxp::dpi::{LogicalPosition, LogicalSize};
use wxp::{Rect, WxpWebViewBuilder};

fn test_web_context(name: &str) -> WebContext {
    // Windows WebView2 may keep profile state alive past WebView drop. GUI tests use isolated
    // profiles so CI failures do not depend on previous test binaries finishing teardown first.
    WebContext::new(std::env::temp_dir().join(format!("wxp-test-{}-{}", std::process::id(), name)))
}

fn schedule_on_run_loop<F>(run_loop: &RunLoopLocal, duration: Duration, callback: F)
where
    F: FnOnce(&RunLoopLocal) + 'static,
{
    run_loop.schedule(duration, callback).detach();
}

fn run_app(run_loop: &RunLoopLocal) {
    run_loop.run_app();
}

fn stop_app() {
    RunLoop::call(|run_loop| run_loop.stop_app()).unwrap();
}

fn main() {
    test_harness::run_gui_tests(vec![("basic WebView functionality", test_webview_basic)]);
}

fn test_webview_basic(run_loop: &RunLoopLocal) -> std::result::Result<(), String> {
    use parking_lot::Mutex;
    use std::sync::Arc;

    // Struct to hold resources
    struct Resources {
        _window: host_window::HostWindowHandle,
        _webview: wxp::WxpWebView,
    }

    let resources = Arc::new(Mutex::new(None));
    let resources_clone = resources.clone();

    schedule_on_run_loop(run_loop, Duration::ZERO, move |run_loop| {
        let window_width = 600.0;
        let window_height = 400.0;
        let window = create_window("WebView Test", window_width, window_height);

        let mut web_context = test_web_context("webview-basic");

        // Set bounds to match the parent window size
        let bounds = Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(window_width, window_height).into(),
        };

        let webview = WxpWebViewBuilder::new(&mut web_context)
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

        schedule_on_run_loop(run_loop, Duration::from_millis(1000), |_run_loop| {
            stop_app()
        });
    });

    run_app(run_loop);
    Ok(())
}
