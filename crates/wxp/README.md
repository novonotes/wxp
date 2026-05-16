# wxp

wxp (WebView X Plugin) is a WebView foundation for audio plugin development.
Built on wry, it provides WXP IPC features and simplifies building plugin UIs.

## Key Features

- **WxpWebViewBuilder**: Build and configure a WebView
- **Command API**: Type-safe request/response communication from JS to Rust
- **Channel API**: Push notifications and streaming from Rust to JavaScript

## Supported Platforms

macOS, Windows, and Linux. Primary verification targets are macOS and Windows.

## Caveats

Before using wxp, keep the following constraints in mind:

- **UI-thread owner**: `WxpWebView` owns the native WebView lifetime and is intentionally
  `!Send + !Sync`, so it stays on the run loop thread that created it.
- **Thread-safe dispatch**: clone `WebViewDispatch` when another thread needs to post WebView
  operations. It does not keep the native WebView alive, and its API enqueues work instead of
  blocking for completion.
- **Hold on to `WxpWebView`**: the WebView is destroyed when the owner is dropped. Keep it alive
  for as long as you want the UI to stay visible.

## WxpWebViewBuilder

Simplifies WebView construction and management.

```rust
use std::rc::Rc;
use wxp::{WebContext, WxpCommandHandler, WxpWebViewBuilder};

let mut web_context = WebContext::new(std::env::temp_dir().join("my-plugin"));
let handler = Rc::new(WxpCommandHandler::new());

let webview = WxpWebViewBuilder::new(&mut web_context)
    .with_command_handler(handler)
    .with_html(HTML_CONTENT)
    .with_devtools(true)
    .build_as_child(&window)?;
```
## Command

The WXP `invoke` and command API provides request/response communication from JavaScript to Rust.

### Async Commands

#### JavaScript side

```javascript
import { invoke } from "@novonotes/webview-bridge";

// Call an async command
const result = await invoke("fetch_device_list", { filter: { type: "audio" } });
console.log(result.devices);
```

#### Rust side

```rust
#[derive(Serialize, Deserialize)]
struct Filter {
    type: String,
}

// Async command — supports async/await
// Use register_sync to register a synchronous command.
handler.register_async("fetch_device_list", |ctx| {
    // Arguments can be any Deserializable type, not just Filter.
    let filter = ctx.arg::<Filter>("filter").unwrap();
    let webview = ctx.webview().clone();

    async move {
        // Execute async processing
        let devices = fetch_devices(&filter).await?;
        let _ = webview.post_eval_script("window.dispatchEvent(new Event('devices-ready'))");
        Ok(json!({ "devices": devices }))
    }
});
```

`CommandContext::webview()` returns `WebViewDispatch`, not the native WebView owner. Commands and
channels may outlive the page that created them, so dispatch methods return `WebViewClosed` if the
`WxpWebView` has already been dropped.

## Channel

The WXP Channel API enables real-time data streaming from Rust to the WebView.

### Basic Usage

#### JavaScript side

```javascript
import { invoke, Channel } from "@novonotes/webview-bridge";

// Create a channel and receive messages
const ch = new Channel((message) => {
  console.log("Received event:", message);
});

// Pass the Channel object directly as an argument to invoke().
await invoke("subscribe_events", { channel: ch });
```

#### Rust side

```rust
handler.register_sync("subscribe_events", |ctx| {
    // Retrieve the channel passed as an argument
    let channel = ctx.arg::<Channel>("channel").unwrap();

    // Send a JSON message
    channel.send(json!({ "type": "connected" }))?;

    // Send binary data
    let binary_data: Vec<u8> = vec![0xFF, 0xD8, 0xFF]; // e.g. JPEG header
    channel.send_bytes(binary_data)?;

    Ok(json!({ "status": "subscribed" }))
});
```

#### Receiving binary data on the JavaScript side

```javascript
const channel = new Channel((message) => {
  if (message instanceof ArrayBuffer) {
    // Handle as binary data
    const bytes = new Uint8Array(message);
    console.log("Received binary:", bytes);
  } else {
    // Handle as JSON data
    console.log("Received JSON:", message);
  }
});
```

Use `instanceof ArrayBuffer` to distinguish binary data from JSON messages.
