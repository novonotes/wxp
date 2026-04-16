use host_window::create_window;
use log::error;
use novonotes_run_loop::{RunLoop, test_harness};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wry::dpi::{Position, Size};
use wxp::Rect;
use wxp::WebContext;
use wxp::dpi::{LogicalPosition, LogicalSize};
use wxp::{WxpCommandHandler, WxpWebViewBuilder};

fn main() {
    test_harness::run_gui_tests(vec![
        (
            "channel error handling",
            test_channel_error as fn() -> std::result::Result<(), String>,
        ),
        (
            "small json message handling",
            test_channel_json_small as fn() -> std::result::Result<(), String>,
        ),
        (
            "large json message handling",
            test_channel_json_large as fn() -> std::result::Result<(), String>,
        ),
        (
            "small binary message handling",
            test_channel_binary_small as fn() -> std::result::Result<(), String>,
        ),
        (
            "large binary message handling",
            test_channel_binary_large as fn() -> std::result::Result<(), String>,
        ),
    ]);
}

fn test_channel_error() -> std::result::Result<(), String> {
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
                try {
                    await window.invoke('bad_channel', {});
                    await window.invoke('report', { caught: false });
                } catch (e) {
                    await window.invoke('report', {
                        caught: e.message.includes('Channel parameter is required')
                    });
                }
            });
        </script>"#;

            let width = 600.0;
            let height = 400.0;
            let window = create_window("Channel Error Test", width, height);
            let handler = Arc::new(WxpCommandHandler::new());
            let caught = error_caught_clone.clone();

            handler.register_async("bad_channel", |_| async move {
                Err::<serde_json::Value, _>("Channel parameter is required")
            });

            handler.register_async("report", move |ctx| {
                if let Ok(result) = ctx.arg::<bool>("caught") {
                    caught.store(result, Ordering::SeqCst);
                }
                // Stop the test immediately after the report arrives
                RunLoop::current().stop_app();
                async move { Ok::<_, &str>(json!({})) }
            });

            let wxp_context = WebContext::new(std::env::temp_dir().join("wxp-test"));
            let mut wry_context = wxp_context.build_wry_context();

            let webview = WxpWebViewBuilder::new(&mut wry_context)
                .with_command_handler(handler)
                .with_html(html)
                .with_devtools(true)
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
        Err("Channel error test failed".to_string())
    }
}

fn test_channel_json_small() -> std::result::Result<(), String> {
    use parking_lot::Mutex;

    // Struct to hold resources
    struct Resources {
        _window: host_window::HostWindowHandle,
        _webview: wxp::WebViewRef,
    }

    let resources = Arc::new(Mutex::new(None));
    let resources_clone = resources.clone();

    let message_received = Arc::new(AtomicBool::new(false));
    let message_received_clone = message_received.clone();

    RunLoop::current()
        .schedule(Duration::ZERO, move || {
            let html = r#"<script>
            window.addEventListener('load', async () => {
                try {
                    // Wrap message reception in a Promise
                    const messageReceived = new Promise((resolve, reject) => {
                        const channel = new window.Channel((msg) => {
                            console.log('Message received:', msg);
                            if (msg && msg.type === 'small' && msg.data === 'test') {
                                resolve(true);
                            }
                        });

                        // Execute the send
                        window.invoke('send_small_json', { ch: channel.toIPC() })
                            .catch(reject);
                    });

                    // Timeout Promise
                    const timeout = new Promise((_, reject) => {
                        setTimeout(() => reject(new Error('Timeout: Message not received within 5 seconds')), 5000);
                    });

                    // Race timeout against reception
                    const received = await Promise.race([messageReceived, timeout]);
                    await window.invoke('report', { received });

                } catch (e) {
                    console.error('Small JSON test error:', e);
                    await window.invoke('report', { received: false });
                }
            });
        </script>"#;

            let width = 600.0;
            let height = 400.0;
            let window = create_window("Small JSON Message Test", width, height);
            let handler = Arc::new(WxpCommandHandler::new());
            let received = message_received_clone.clone();

            handler.register_async("send_small_json", move |ctx| {
                use wxp::Channel;
                let channel = ctx.arg::<Channel>("ch").unwrap();

                async move {
                    // Send a small JSON message
                    let small_message = json!({
                        "type": "small",
                        "data": "test",
                        "timestamp": 123456789
                    });

                    channel.send(small_message).map_err(|e| e.to_string())?;
                    Ok::<_, String>(json!({ "status": "sent" }))
                }
            });

            handler.register_async("report", move |ctx| {
                if let Ok(r) = ctx.arg::<bool>("received") {
                    received.store(r, Ordering::SeqCst);
                }
                // Stop the test immediately after the report arrives
                RunLoop::current().stop_app();
                async move { Ok::<_, &str>(json!({})) }
            });

            let wxp_context = WebContext::new(std::env::temp_dir().join("wxp-test"));
            let mut wry_context = wxp_context.build_wry_context();

            let webview = WxpWebViewBuilder::new(&mut wry_context)
                .with_command_handler(handler)
                .with_html(html)
                .with_devtools(true)
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

    if message_received.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err("Small JSON message test failed".to_string())
    }
}

fn test_channel_json_large() -> std::result::Result<(), String> {
    use parking_lot::Mutex;

    // Struct to hold resources
    struct Resources {
        _window: host_window::HostWindowHandle,
        _webview: wxp::WebViewRef,
    }

    let resources = Arc::new(Mutex::new(None));
    let resources_clone = resources.clone();

    let large_message_received = Arc::new(AtomicBool::new(false));
    let large_message_received_clone = large_message_received.clone();

    RunLoop::current()
        .schedule(Duration::ZERO, move || {
            let html = r#"<script>
            window.addEventListener('load', async () => {
                try {
                    // Wrap message reception in a Promise
                    const messageReceived = new Promise((resolve, reject) => {
                        const channel = new window.Channel((msg) => {
                            console.log('Message received with type:', msg?.type, 'data length:', msg?.data?.length);
                            if (msg && msg.type === 'large' && msg.data && msg.data.length > 8000) {
                                const expected = 'x'.repeat(10000);
                                if (msg.data !== expected) {
                                    console.error('Large message data corrupted');
                                    resolve(false);
                                } else {
                                    resolve(true);
                                }
                            }
                        });

                        // Execute the send
                        window.invoke('send_large', { ch: channel.toIPC() })
                            .catch(reject);
                    });

                    // Timeout Promise
                    const timeout = new Promise((_, reject) => {
                        setTimeout(() => reject(new Error('Timeout: Message not received within 5 seconds')), 5000);
                    });

                    // Race timeout against reception
                    const received = await Promise.race([messageReceived, timeout]);
                    await window.invoke('report_large', { received });

                } catch (e) {
                    console.error('Large message test error:', e);
                    await window.invoke('report_large', { received: false });
                }
            });
        </script>"#;

            let width = 600.0;
            let height = 400.0;
            let window = create_window("Large Message Test", width, height);
            let handler = Arc::new(WxpCommandHandler::new());
            let received = large_message_received_clone.clone();

            handler.register_async("send_large", move |ctx| {
                use wxp::Channel;
                let channel = ctx.arg::<Channel>("ch").unwrap();

                async move {
                    // Create a message larger than MAX_JSON_DIRECT_EXECUTE_THRESHOLD (8192 bytes)
                    let large_data = "x".repeat(10000);
                    let large_message = json!({
                        "type": "large",
                        "data": large_data,
                        "metadata": {
                            "size": 10000,
                            "test": "This message should trigger the large message handling path"
                        }
                    });

                    channel.send(large_message).map_err(|e| e.to_string())?;
                    Ok::<_, String>(json!({ "status": "sent" }))
                }
            });

            handler.register_async("report_large", move |ctx| {
                if let Ok(r) = ctx.arg::<bool>("received") {
                    received.store(r, Ordering::SeqCst);
                }
                // Stop the test immediately after the report arrives
                RunLoop::current().stop_app();
                async move { Ok::<_, &str>(json!({})) }
            });

            let wxp_context = WebContext::new(std::env::temp_dir().join("wxp-test"));
            let mut wry_context = wxp_context.build_wry_context();

            let webview = WxpWebViewBuilder::new(&mut wry_context)
                .with_command_handler(handler)
                .with_html(html)
                .with_devtools(true)
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
            error!("Test timeout: report_large was not received within 30 seconds");
            RunLoop::current().stop_app()
        })
        .detach();

    RunLoop::current().run_app();

    if large_message_received.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err("Large message test failed: message not received correctly".to_string())
    }
}

fn test_channel_binary_small() -> std::result::Result<(), String> {
    use parking_lot::Mutex;

    // Struct to hold resources
    struct Resources {
        _window: host_window::HostWindowHandle,
        _webview: wxp::WebViewRef,
    }

    let resources = Arc::new(Mutex::new(None));
    let resources_clone = resources.clone();

    let binary_message_received = Arc::new(AtomicBool::new(false));
    let binary_message_received_clone = binary_message_received.clone();

    RunLoop::current()
        .schedule(Duration::ZERO, move || {
            let html = r#"<script>
            window.addEventListener('load', async () => {
                try {
                    // Wrap message reception in a Promise
                    const messageReceived = new Promise((resolve, reject) => {
                        const channel = new window.Channel(async (msg) => {
                            if (msg instanceof ArrayBuffer) {
                                const bytes = new Uint8Array(msg);
                                console.log('Binary message received, size:', bytes.length);

                                if (bytes.length === 100) {
                                    // Small binary message
                                    let allMatch = true;
                                    for (let i = 0; i < bytes.length; i++) {
                                        if (bytes[i] !== i % 256) {
                                            allMatch = false;
                                            break;
                                        }
                                    }
                                    resolve(allMatch);
                                }
                            }
                        });

                        // Execute the send
                        window.invoke('send_binary_small', { ch: channel.toIPC() })
                            .catch(reject);
                    });

                    // Timeout Promise
                    const timeout = new Promise((_, reject) => {
                        setTimeout(() => reject(new Error('Timeout: Message not received within 5 seconds')), 5000);
                    });

                    // Race timeout against reception
                    const received = await Promise.race([messageReceived, timeout]);
                    await window.invoke('report_binary', { received });

                } catch (e) {
                    console.error('Binary test error:', e);
                    await window.invoke('report_binary', { received: false });
                }
            });
        </script>"#;

            let width = 600.0;
            let height = 400.0;
            let window = create_window("Small Binary Message Test", width, height);
            let handler = Arc::new(WxpCommandHandler::new());
            let received = binary_message_received_clone.clone();

            handler.register_async("send_binary_small", move |ctx| {
                use wxp::Channel;
                let channel = ctx.arg::<Channel>("ch").unwrap();

                async move {
                    // Send small binary message (100 bytes)
                    let small_data: Vec<u8> = (0..100u8).collect();
                    channel.send_bytes(small_data).map_err(|e| e.to_string())?;

                    Ok::<_, String>(json!({ "status": "sent" }))
                }
            });

            handler.register_async("report_binary", move |ctx| {
                if let Ok(r) = ctx.arg::<bool>("received") {
                    received.store(r, Ordering::SeqCst);
                }
                // Stop the test immediately after the report arrives
                RunLoop::current().stop_app();
                async move { Ok::<_, &str>(json!({})) }
            });

            let wxp_context = WebContext::new(std::env::temp_dir().join("wxp-test"));
            let mut wry_context = wxp_context.build_wry_context();

            let webview = WxpWebViewBuilder::new(&mut wry_context)
                .with_command_handler(handler)
                .with_html(html)
                .with_devtools(true)
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

    if binary_message_received.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err("Small binary message test failed".to_string())
    }
}

fn test_channel_binary_large() -> std::result::Result<(), String> {
    use parking_lot::Mutex;

    // Struct to hold resources
    struct Resources {
        _window: host_window::HostWindowHandle,
        _webview: wxp::WebViewRef,
    }

    let resources = Arc::new(Mutex::new(None));
    let resources_clone = resources.clone();

    let binary_message_received = Arc::new(AtomicBool::new(false));
    let binary_message_received_clone = binary_message_received.clone();

    RunLoop::current()
        .schedule(Duration::ZERO, move || {
            let html = r#"<script>
            window.addEventListener('load', async () => {
                try {
                    // Wrap message reception in a Promise
                    const messageReceived = new Promise((resolve, reject) => {
                        const channel = new window.Channel(async (msg) => {
                            if (msg instanceof ArrayBuffer) {
                                const bytes = new Uint8Array(msg);
                                console.log('Binary message received, size:', bytes.length);

                                if (bytes.length === 2000) {
                                    // Large binary message
                                    let allMatch = true;
                                    for (let i = 0; i < bytes.length; i++) {
                                        if (bytes[i] !== (i * 2) % 256) {
                                            allMatch = false;
                                            break;
                                        }
                                    }
                                    resolve(allMatch);
                                }
                            }
                        });

                        // Execute the send
                        window.invoke('send_binary_large', { ch: channel.toIPC() })
                            .catch(reject);
                    });

                    // Timeout Promise
                    const timeout = new Promise((_, reject) => {
                        setTimeout(() => reject(new Error('Timeout: Message not received within 5 seconds')), 5000);
                    });

                    // Race timeout against reception
                    const received = await Promise.race([messageReceived, timeout]);
                    await window.invoke('report_binary', { received });

                } catch (e) {
                    console.error('Binary test error:', e);
                    await window.invoke('report_binary', { received: false });
                }
            });
        </script>"#;

            let width = 600.0;
            let height = 400.0;
            let window = create_window("Large Binary Message Test", width, height);
            let handler = Arc::new(WxpCommandHandler::new());
            let received = binary_message_received_clone.clone();

            handler.register_async("send_binary_large", move |ctx| {
                use wxp::Channel;
                let channel = ctx.arg::<Channel>("ch").unwrap();

                async move {
                    // Send large binary message (2000 bytes)
                    let large_data: Vec<u8> = (0..2000u16).map(|i| ((i * 2) % 256) as u8).collect();
                    channel.send_bytes(large_data).map_err(|e| e.to_string())?;

                    Ok::<_, String>(json!({ "status": "sent" }))
                }
            });

            handler.register_async("report_binary", move |ctx| {
                if let Ok(r) = ctx.arg::<bool>("received") {
                    received.store(r, Ordering::SeqCst);
                }
                // Stop the test immediately after the report arrives
                RunLoop::current().stop_app();
                async move { Ok::<_, &str>(json!({})) }
            });

            let wxp_context = WebContext::new(std::env::temp_dir().join("wxp-test"));
            let mut wry_context = wxp_context.build_wry_context();

            let webview = WxpWebViewBuilder::new(&mut wry_context)
                .with_command_handler(handler)
                .with_html(html)
                .with_devtools(true)
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

    if binary_message_received.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err("Large binary message test failed".to_string())
    }
}
