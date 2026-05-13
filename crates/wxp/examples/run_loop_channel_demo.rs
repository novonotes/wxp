// Channel streaming demo - run_loop version (using CommandContext)

use host_window::{HostWindowHandle, create_window};
use log::info;
use novonotes_run_loop::RunLoop;
use serde_json::json;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use wxp::WebContext;
use wxp::dpi::{LogicalPosition, LogicalSize};
use wxp::{Channel, Rect, WxpCommandHandler, WxpWebViewBuilder};

const HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Streaming Demo (run_loop)</title>
    <style>
        body { font-family: monospace; padding: 20px; }
        button { margin: 5px; }
        #messages {
            border: 1px solid #ccc;
            padding: 10px;
            margin-top: 10px;
            height: 400px;
            overflow-y: auto;
        }
        .done { color: green; }
    </style>
</head>
<body>
    <h1>Streaming Demo (run_loop)</h1>
    <button id="startBtn" onclick="startStreaming()">Start</button>
    <button onclick="document.getElementById('messages').innerHTML=''">Clear</button>
    <div id="messages"></div>

    <script>
        let currentChannel = null;

        function addMessage(data) {
            const div = document.createElement('div');
            if (data.done) div.className = 'done';
            div.textContent = `[${new Date().toLocaleTimeString()}] ${JSON.stringify(data)}`;
            messages.appendChild(div);
            messages.scrollTop = messages.scrollHeight;
        }

        async function startStreaming() {
            try {
                startBtn.disabled = true;

                currentChannel = new Channel((message) => {
                    addMessage(message);
                    if (message.done) startBtn.disabled = false;
                });

                addMessage({ info: `Channel: ${currentChannel.id}` });

                const response = await window.invoke('start_streaming', {
                    channel: currentChannel.toIPC()
                });

                addMessage({ info: `Response: ${JSON.stringify(response)}` });

            } catch (error) {
                addMessage({ error: error.message });
                startBtn.disabled = false;
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
    let handler = Rc::new(WxpCommandHandler::new());

    // Register commands
    handler.register_async("start_streaming", |ctx| {
        // Retrieve required values from context in advance
        let channel = Arc::new(ctx.arg::<Channel>("channel").unwrap());

        // Async block
        async move {
            // Get the channel ID
            let channel_id = channel.id();

            info!("Received channel ID: {}", channel_id);

            // Schedule message sending on the RunLoop
            for i in 1..=10 {
                let channel_clone = channel.clone();
                let mut handle =
                    RunLoop::current().schedule(Duration::from_millis(i as u64 * 500), move || {
                        let message = json!({
                            "count": i,
                            "message": format!("Streaming message #{}", i),
                            "timestamp": chrono::Local::now().format("%H:%M:%S").to_string()
                        });

                        info!("Sending message #{}", i);

                        if let Err(e) = channel_clone.send(message) {
                            info!("Failed to send message #{}: {:?}", i, e);
                        }
                    });
                handle.detach();
            }

            // Completion message
            let channel_clone = channel.clone();
            let mut handle = RunLoop::current().schedule(Duration::from_millis(5500), move || {
                info!("Streaming completed");
                let _ = channel_clone.send(json!({
                    "done": true,
                    "message": "Streaming completed!"
                }));
            });
            handle.detach();

            Ok::<_, &str>(json!({
                "status": "streaming_started",
                "channel_id": channel_id
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
        let window_height = 500.0;
        let window = create_window(
            "Streaming Demo - wxp_channel (run_loop)",
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
