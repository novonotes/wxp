// greet command demo - run_loop version (using CommandContext)

use host_window::{HostWindowHandle, create_window};
use novonotes_run_loop::RunLoop;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use wxp::WebContext;
use wxp::dpi::{LogicalPosition, LogicalSize};
use wxp::{Rect, WxpCommandHandler, WxpWebViewBuilder};

const HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>wxp_command Demo (run_loop)</title>
    <style>
        body { font-family: system-ui; padding: 20px; }
        input, button { padding: 8px; margin: 4px; }
        #output { margin-top: 20px; padding: 10px; background: #f0f0f0; font-family: monospace; }
    </style>
</head>
<body>
    <h2>wxp_command Demo (run_loop)</h2>
    <input type="text" id="name" placeholder="Your name" value="World">
    <button onclick="greet()">Greet</button>
    <div id="output"></div>
    <script>
        async function greet() {
            const name = document.getElementById('name').value;
            const output = document.getElementById('output');
            try {
                const result = await window.invoke('greet', { name });
                output.textContent = `Result: ${JSON.stringify(result)}`;
            } catch (error) {
                output.textContent = `Error: ${error.message}`;
            }
        }
    </script>
</body>
</html>"#;

// Struct to hold resources
struct Resources {
    _window: HostWindowHandle,
    _webview: wxp::WebViewRef,
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Initialize RunLoop
    RunLoop::init().unwrap();

    // Create a command handler
    let handler = Arc::new(WxpCommandHandler::new());

    // Register commands
    handler.register_async("greet", |ctx| {
        // Retrieve argument with type safety (with a default value)
        let name = ctx
            .arg::<String>("name")
            .unwrap_or_else(|_| "World".to_string());

        async move {
            Ok::<_, &str>(json!({
                "message": format!("Hello, {}! This is from wxp_command (run_loop version).", name)
            }))
        }
    });

    // Variable to hold resources
    let resources = Arc::new(parking_lot::Mutex::new(None));
    let resources_for_schedule = resources.clone();

    // Schedule WebView creation
    let mut handle = RunLoop::current().schedule(Duration::ZERO, move || {
        // Create window
        let window_width = 600.0;
        let window_height = 400.0;
        let window = create_window(
            "wxp_command - Greet Demo (run_loop)",
            window_width,
            window_height,
        );

        // Create WebView
        let mut web_context = WebContext::new(std::env::temp_dir().join("wxp-example"));

        // Set bounds to match the parent window size
        let bounds = Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(window_width, window_height).into(),
        };

        let webview = WxpWebViewBuilder::new(&mut web_context)
            .with_command_handler(handler)
            .with_html(HTML)
            .with_devtools(true)
            .with_bounds(bounds)
            .build_as_child(&window)
            .unwrap();

        // Show the window
        window.show();

        // Save resources
        *resources_for_schedule.lock() = Some(Resources {
            _window: window,
            _webview: webview,
        });
    });
    handle.detach();

    // Run the app
    RunLoop::current().run_app();

    // Resources are automatically dropped
    RunLoop::deinit();

    Ok(())
}
