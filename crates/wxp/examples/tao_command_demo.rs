// greet command demo - tao-based (using CommandContext)

use novonotes_run_loop::RunLoop;
use serde_json::json;
use std::rc::Rc;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wxp::WebContext;
use wxp::dpi::{LogicalPosition, LogicalSize as WxpLogicalSize};
use wxp::{Rect, WxpCommandHandler, WxpWebViewBuilder};

const HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>wxp_command Demo</title>
    <style>
        body { font-family: system-ui; padding: 20px; }
        input, button { padding: 8px; margin: 4px; }
        #output { margin-top: 20px; padding: 10px; background: #f0f0f0; font-family: monospace; }
    </style>
</head>
<body>
    <h2>wxp_command Demo</h2>
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

fn main() -> wry::Result<()> {
    RunLoop::init().unwrap();
    let event_loop = EventLoop::new();

    let window_width = 600.0;
    let window_height = 400.0;
    let window = WindowBuilder::new()
        .with_title("wxp_command - Greet Demo (tao)")
        .with_inner_size(LogicalSize::new(window_width, window_height))
        .build(&event_loop)
        .unwrap();

    // Create a command handler
    let handler = Rc::new(WxpCommandHandler::new());

    // Register commands
    handler.register_async("greet", |ctx| {
        // Retrieve argument with type safety (with a default value)
        let name = ctx
            .arg::<String>("name")
            .unwrap_or_else(|_| "World".to_string());

        async move {
            Ok::<_, &str>(json!({
                "message": format!("Hello, {}! This is from wxp_command (tao version).", name)
            }))
        }
    });

    // Create the WebView
    let mut web_context = WebContext::new(std::env::temp_dir().join("wxp-example"));

    // Set bounds to match the parent window size
    let bounds = Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: WxpLogicalSize::new(window_width, window_height).into(),
    };

    let _webview = WxpWebViewBuilder::new(&mut web_context)
        .with_command_handler(handler.clone())
        .with_html(HTML)
        .with_devtools(true)
        .with_bounds(bounds)
        .build_as_child(&window)
        .unwrap();

    // Set up a timer to periodically run the JavaScript queue
    use std::time::{Duration, Instant};
    let mut last_check = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll; // Switch to polling mode

        // Check the JavaScript queue every 10ms
        if last_check.elapsed() > Duration::from_millis(10) {
            // In immediate-execution mode, running the queue is not needed
            last_check = Instant::now();
        }

        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
            RunLoop::deinit();
        }
    });
}
