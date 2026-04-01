// チャンネルストリーミングデモ - taoベース（CommandContext使用）

use log::info;
use novonotes_run_loop::RunLoop;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
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
    <title>Streaming Demo</title>
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
    <h1>Streaming Demo</h1>
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

fn main() -> wry::Result<()> {
    RunLoop::init().unwrap();
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window_width = 600.0;
    let window_height = 500.0;
    let window = WindowBuilder::new()
        .with_title("Streaming Demo - wxp_channel (tao)")
        .with_inner_size(LogicalSize::new(window_width, window_height))
        .build(&event_loop)
        .unwrap();

    // コマンドハンドラーを作成
    let handler = Arc::new(WxpCommandHandler::new());

    // 簡略化されたコマンド登録
    let proxy_clone = proxy.clone();
    handler.register_async("start_streaming", move |ctx| {
        // コンテキストから必要な値を事前に取得
        let proxy = proxy_clone.clone();
        // チャンネル作成
        let channel = Arc::new(ctx.arg::<Channel>("channel").unwrap());

        // 非同期ブロック
        async move {
            // チャンネルIDを取得
            let channel_id = channel.id();

            info!("Received channel ID: {}", channel_id);

            // イベントループにストリーミング開始を通知
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

    // WebViewを作成
    let wxp_context = WebContext::new(std::env::temp_dir().join("wxp-example"));
    let mut wry_context = wxp_context.build_wry_context();

    // 親ウィンドウと同じサイズを設定
    let bounds = Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: WxpLogicalSize::new(window_width, window_height).into(),
    };

    let _webview = WxpWebViewBuilder::new(&mut wry_context)
        .with_command_handler(handler)
        .with_html(HTML)
        .with_devtools(true)
        .with_bounds(bounds)
        .build_as_child(&window)
        .unwrap();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
                RunLoop::deinit();
            }
            Event::UserEvent(user_event) => {
                match user_event {
                    UserEvent::StartStreaming(channel_id, channel) => {
                        info!("Event: Starting streaming to channel {}", channel_id);
                        // 最初のメッセージを送信
                        let _ =
                            proxy.send_event(UserEvent::SendNextMessage(channel_id, channel, 0));
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
                                // 500ms後に次のメッセージを送信
                                let proxy_clone = proxy.clone();
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
                            // ストリーミング終了
                            let _ = channel.send(json!({
                                "done": true,
                                "message": "Streaming completed!"
                            }));
                            info!("Streaming completed");
                        }
                    }
                }
            }
            _ => {}
        };
    });
}
