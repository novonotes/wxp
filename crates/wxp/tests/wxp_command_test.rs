use host_window::create_window;
use log::error;
use novonotes_run_loop::{RunLoop, test_harness};
use serde_json::json;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wxp::Rect;
use wxp::WebContext;
use wxp::dpi::{LogicalPosition, LogicalSize, Position, Size};
use wxp::{WxpCommandHandler, WxpWebViewBuilder};

fn main() {
    test_harness::run_gui_tests(vec![
        (
            "basic command invocation",
            test_command_basic as fn() -> std::result::Result<(), String>,
        ),
        (
            "command error handling",
            test_command_error as fn() -> std::result::Result<(), String>,
        ),
    ]);
}

fn test_command_basic() -> std::result::Result<(), String> {
    use parking_lot::Mutex;

    // Struct to hold resources
    struct Resources {
        _window: host_window::HostWindowHandle,
        _webview: wxp::WebViewRef,
    }

    let resources = Arc::new(Mutex::new(None));
    let resources_clone = resources.clone();

    let test_passed = Arc::new(AtomicBool::new(false));
    let test_passed_clone = test_passed.clone();

    RunLoop::current()
        .schedule(Duration::ZERO, move || {
            let html = r#"<script>
            window.addEventListener('load', async () => {
                console.log('Page loaded');
                console.log('__WXP_INTERNALS__ exists:', typeof window.__WXP_INTERNALS__ !== 'undefined');
                console.log('invoke exists:', typeof window.invoke !== 'undefined');

                try {
                    console.log('Calling echo...');
                    const echo = await window.invoke('echo', { test: 'data', num: 123 });
                    console.log('Echo result:', echo);

                    console.log('Calling add...');
                    const add = await window.invoke('add', { a: 10, b: 20 });
                    console.log('Add result:', add);

                    const passed = echo.received.test === 'data' &&
                                  echo.received.num === 123 &&
                                  add.result === 30;
                    console.log('Test passed:', passed);

                    await window.invoke('report', { passed: passed });
                } catch (e) {
                    console.error('Test error:', e);
                    await window.invoke('report', { passed: false });
                }
            });
        </script>"#;

            let width = 600.0;
            let height = 400.0;
            let window = create_window("Command Test", width, height);
            let handler = Rc::new(WxpCommandHandler::new());
            let passed = test_passed_clone.clone();

            handler.register_async("echo", |ctx| {
                let test = ctx.arg::<String>("test").ok();
                let num = ctx.arg::<i64>("num").ok();
                async move { Ok::<_, &str>(json!({ "received": { "test": test, "num": num } })) }
            });

            handler.register_async("add", |ctx| {
                let a = ctx.arg::<f64>("a").unwrap_or(0.0);
                let b = ctx.arg::<f64>("b").unwrap_or(0.0);
                async move { Ok::<_, &str>(json!({ "result": a + b })) }
            });

            handler.register_async("report", move |ctx| {
                if let Ok(result) = ctx.arg::<bool>("passed") {
                    passed.store(result, Ordering::SeqCst);
                }
                // Stop the test immediately after the report arrives
                RunLoop::current().stop_app();
                async move { Ok::<_, &str>(json!({})) }
            });

            let mut web_context = WebContext::new(std::env::temp_dir().join("wxp-test"));

            let webview = WxpWebViewBuilder::new(&mut web_context)
                .with_command_handler(handler)
                .with_html(html)
                .with_bounds(Rect {
                    position: Position::Logical(LogicalPosition::new(0.0, 0.0)),
                    size: Size::Logical(LogicalSize::new(width, height)),
                })
                .build_as_child(&window)
                .expect("Failed to create WebView");

            window.show();

            // Save resources to extend the WebView's lifetime
            *resources_clone.lock() = Some(Resources {
                _window: window,
                _webview: webview,
            });
        })
        .detach();

    // Timeout is set as a fallback to 30 seconds
    RunLoop::current()
        .schedule(Duration::from_millis(30000), || {
            error!("Test timeout: report was not received within 30 seconds");
            RunLoop::current().stop_app()
        })
        .detach();

    RunLoop::current().run_app();

    if test_passed.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err("Command test failed".to_string())
    }
}

fn test_command_error() -> std::result::Result<(), String> {
    use parking_lot::Mutex;

    // Struct to hold resources
    struct Resources {
        _window: host_window::HostWindowHandle,
        _webview: wxp::WebViewRef,
    }

    let resources = Arc::new(Mutex::new(None));
    let resources_clone = resources.clone();

    let error_caught = Arc::new(AtomicBool::new(false));
    let error_caught_clone = error_caught.clone();

    RunLoop::current()
        .schedule(Duration::ZERO, move || {
            let html = r#"<script>
            window.addEventListener('load', async () => {
                console.log('Error test page loaded');

                try {
                    await window.invoke('fail', {});
                    await window.invoke('report', { caught: false });
                } catch (e) {
                    console.log('Caught error:', e.message);
                    await window.invoke('report', {
                        caught: e.message.includes('This command always fails')
                    });
                }
            });
        </script>"#;

            let width = 600.0;
            let height = 400.0;
            let window = create_window("Error Test", width, height);
            let handler = Rc::new(WxpCommandHandler::new());
            let caught = error_caught_clone.clone();

            handler.register_async("fail", |_| async move {
                Err::<serde_json::Value, _>("This command always fails")
            });

            handler.register_async("report", move |ctx| {
                if let Ok(result) = ctx.arg::<bool>("caught") {
                    caught.store(result, Ordering::SeqCst);
                }
                // Stop the test immediately after the report arrives
                RunLoop::current().stop_app();
                async move { Ok::<_, &str>(json!({})) }
            });

            let mut web_context = WebContext::new(std::env::temp_dir().join("wxp-test"));

            let webview = WxpWebViewBuilder::new(&mut web_context)
                .with_command_handler(handler)
                .with_html(html)
                .with_bounds(Rect {
                    position: Position::Logical(LogicalPosition::new(0.0, 0.0)),
                    size: Size::Logical(LogicalSize::new(width, height)),
                })
                .build_as_child(&window)
                .expect("Failed to create WebView");

            window.show();

            // Save resources to extend the WebView's lifetime
            *resources_clone.lock() = Some(Resources {
                _window: window,
                _webview: webview,
            });
        })
        .detach();

    // Timeout is set as a fallback to 30 seconds
    RunLoop::current()
        .schedule(Duration::from_millis(30000), || {
            error!("Test timeout: report was not received within 30 seconds");
            RunLoop::current().stop_app()
        })
        .detach();

    RunLoop::current().run_app();

    if error_caught.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err("Error handling test failed".to_string())
    }
}
