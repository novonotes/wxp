// Channel streaming demo - winit version (using CommandContext)

use log::info;
use novonotes_run_loop::RunLoop;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};
use wxp::WebContext;
use wxp::dpi::{LogicalPosition, LogicalSize as WxpLogicalSize};
use wxp::{Channel, Rect, WxpCommandHandler, WxpWebViewBuilder};

#[derive(Debug, Clone)]
enum UserEvent {
    StartStreaming(String, Arc<wxp::Channel>),
    SendNextMessage(String, Arc<wxp::Channel>, usize),
}

const HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Streaming Demo (winit)</title>
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
    <h1>Streaming Demo (winit)</h1>
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

struct App {
    window: Option<Window>,
    webview: Option<wxp::WebViewRef>,
    handler: Arc<WxpCommandHandler>,
    event_loop_proxy: winit::event_loop::EventLoopProxy<UserEvent>,
    _web_context: Option<WebContext>,
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_none() {
            // Create a winit window
            let window_width = 600.0;
            let window_height = 500.0;
            let window_attrs = WindowAttributes::default()
                .with_title("Streaming Demo - wxp_command + wxp_channel (winit)")
                .with_inner_size(LogicalSize::new(window_width, window_height));
            let window = event_loop.create_window(window_attrs).unwrap();

            // Create the WebView
            let mut web_context = WebContext::new(std::env::temp_dir().join("wxp-example"));

            // Set bounds to match the parent window size
            let bounds = Rect {
                position: LogicalPosition::new(0.0, 0.0).into(),
                size: WxpLogicalSize::new(window_width, window_height).into(),
            };

            let webview = WxpWebViewBuilder::new(&mut web_context)
                .with_command_handler(self.handler.clone())
                .with_html(HTML)
                .with_devtools(true)
                .with_bounds(bounds)
                .build_as_child(&window)
                .unwrap();

            self.window = Some(window);
            self.webview = Some(webview);
            self._web_context = Some(web_context);
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

    fn user_event(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::StartStreaming(channel_id, channel) => {
                info!("Event: Starting streaming to channel {}", channel_id);
                // Send the first message
                let _ = self
                    .event_loop_proxy
                    .send_event(UserEvent::SendNextMessage(channel_id, channel, 0));
            }
            UserEvent::SendNextMessage(channel_id, channel, index) => {
                if index < 10 {
                    let message = json!({
                        "count": index + 1,
                        "message": format!("Streaming message #{}", index + 1),
                        "timestamp": chrono::Local::now().format("%H:%M:%S").to_string()
                    });

                    info!("Sending message #{}", index + 1);

                    if channel.send(message).is_ok() {
                        // Send the next message after 500ms
                        let proxy_clone = self.event_loop_proxy.clone();
                        let channel_id_clone = channel_id.clone();
                        let channel_clone = channel.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(500));
                            let _ = proxy_clone.send_event(UserEvent::SendNextMessage(
                                channel_id_clone,
                                channel_clone,
                                index + 1,
                            ));
                        });
                    } else {
                        info!("Failed to send message #{}", index + 1);
                    }
                } else {
                    // Streaming finished
                    let _ = channel.send(json!({
                        "done": true,
                        "message": "Streaming completed!"
                    }));
                    info!("Streaming completed");
                }
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        // In immediate-execution mode, checking the queue is not needed
    }
}

impl App {
    fn new(
        event_loop_proxy: winit::event_loop::EventLoopProxy<UserEvent>,
    ) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        // Create a command handler
        let handler = Arc::new(WxpCommandHandler::new());

        // Register commands
        let proxy_clone = event_loop_proxy.clone();
        handler.register_async("start_streaming", move |ctx| {
            // Retrieve required values from context in advance
            let proxy = proxy_clone.clone();
            // Create the channel
            let channel = Arc::new(ctx.arg::<Channel>("channel").unwrap());

            // Async block
            async move {
                // Error handling
                let channel_id = channel.id();

                info!("Received channel ID: {}", channel_id);

                // Notify the event loop to start streaming
                let _ = proxy.send_event(UserEvent::StartStreaming(
                    channel_id.to_string(),
                    channel.clone(),
                ));

                Ok::<_, &str>(json!({
                    "status": "streaming_started",
                    "channel_id": channel_id
                }))
            }
        });

        Ok(Self {
            window: None,
            webview: None,
            handler,
            event_loop_proxy,
            _web_context: None,
        })
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    RunLoop::init().unwrap();
    // Create the event loop
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy)?;

    // Run the event loop
    event_loop.run_app(&mut app)?;

    RunLoop::deinit();
    Ok(())
}
