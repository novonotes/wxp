# wxp

wxp (WebView X Plugin) is a WebView integration crate for audio plugin development.
Built on wry, it provides Tauri-like IPC features and simplifies building plugin UIs.

## Key Features

- **WxpWebViewBuilder**: Build and configure a WebView
- **Command API**: Type-safe request/response communication from JS to Rust
- **Channel API**: Push notifications and streaming from Rust to JavaScript

## WxpWebViewBuilder

Simplifies WebView construction and management.

```rust
use wxp::WxpWebViewBuilder;

let webview = WxpWebViewBuilder::new()
    .with_command_handler(handler)
    .with_html(HTML_CONTENT)
    .with_devtools(true)
    .build(&window)?;
```
## Command

An API similar to Tauri's `invoke` and `command`. Provides command-based bidirectional communication.

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

    async move {
        // Execute async processing
        let devices = fetch_devices(&filter).await?;
        Ok(json!({ "devices": devices }))
    }
});
```

## Channel

An API similar to Tauri's Channel API. Enables real-time data streaming from Rust to the WebView.

### Basic Usage

#### JavaScript side

```javascript
import { invoke, Channel } from "@novonotes/webview-bridge";

// Create a channel and receive messages
const ch = new Channel((message) => {
  console.log("Received event:", message);
});

// Register the channel
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

This implementation follows the same pattern as Tauri — use `instanceof ArrayBuffer` to distinguish binary data.
