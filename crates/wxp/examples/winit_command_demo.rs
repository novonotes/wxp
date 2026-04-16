// greet command demo - winit version (using CommandContext)

use novonotes_run_loop::RunLoop;
use serde_json::json;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};
use wxp::WebContext;
use wxp::dpi::{LogicalPosition, LogicalSize as WxpLogicalSize};
use wxp::{Rect, WxpCommandHandler, WxpWebViewBuilder};

const HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>wxp_command Demo (winit)</title>
    <style>
        body { font-family: system-ui; padding: 20px; }
        input, button { padding: 8px; margin: 4px; }
        #output { margin-top: 20px; padding: 10px; background: #f0f0f0; font-family: monospace; }
    </style>
</head>
<body>
    <h2>wxp_command Demo (winit + run_loop)</h2>
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

struct App {
    window: Option<Window>,
    webview: Option<wxp::WebViewRef>,
    handler: Arc<WxpCommandHandler>,
    _wry_context: Option<wry::WebContext>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            // Create a winit window
            let window_width = 600.0;
            let window_height = 400.0;
            let window_attrs = WindowAttributes::default()
                .with_title("wxp_command - Greet Demo (winit)")
                .with_inner_size(LogicalSize::new(window_width, window_height));
            let window = event_loop.create_window(window_attrs).unwrap();

            // Create the WebView
            let wxp_context = WebContext::new(std::env::temp_dir().join("wxp-example"));
            let mut wry_context = wxp_context.build_wry_context();

            // Set bounds to match the parent window size
            let bounds = Rect {
                position: LogicalPosition::new(0.0, 0.0).into(),
                size: WxpLogicalSize::new(window_width, window_height).into(),
            };

            let webview = WxpWebViewBuilder::new(&mut wry_context)
                .with_command_handler(self.handler.clone())
                .with_html(HTML)
                .with_devtools(true)
                .with_bounds(bounds)
                .build_as_child(&window)
                .unwrap();

            self.window = Some(window);
            self.webview = Some(webview);
            self._wry_context = Some(wry_context);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }
    }
}

impl App {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        // Create a command handler
        let handler = Arc::new(WxpCommandHandler::new());

        // Register commands
        handler.register_async("greet", |ctx| {
            // Retrieve argument with type safety (with a default value)
            let name = ctx.arg::<String>("name").unwrap_or_else(|_| "".to_string());

            async move {
                Ok::<_, &str>(json!({
                    "message": format!("Hello, {}! This is from wxp_command (winit version).", name)
                }))
            }
        });

        Ok(Self {
            window: None,
            webview: None,
            handler,
            _wry_context: None,
        })
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    RunLoop::init().unwrap();
    // Create the event loop
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new()?;

    // Run the event loop
    event_loop.run_app(&mut app)?;

    RunLoop::deinit();
    Ok(())
}
