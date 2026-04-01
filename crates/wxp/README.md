# wxp

wxp （WebView X Plugin）は、オーディオプラグイン開発向けの WebView 統合クレートです。  
wry をベースに、Tauri に似た IPC の機能を提供し、プラグイン UI の構築を簡素化します。

## 主な機能

- **Command API**: 双方向の型安全なコマンド通信
- **Channel API**: Rust から JavaScript へのリアルタイムストリーミング
- **RunLoop 統合**: プラグイン環境に最適化された非同期処理

## Command

Tauri の `invoke` や `command` に似た API。コマンドベースの双方向通信を提供します。

### 非同期コマンド

#### JavaScript 側

```javascript
import { invoke } from "@novonotes/webview-bridge";

// 非同期コマンドの呼び出し
const result = await invoke("fetch_device_list", { filter: { type: "audio" } });
console.log(result.devices);
```

#### Rust 側

```rust
#[derive(Serialize, Deserialize)]
struct Filter {
    type: String,
}

// 非同期コマンド - async/await が使える
// 同期的なコマンドを登録する場合は register_sync を使用してください。
handler.register_async("fetch_device_list", |ctx| {
    // Filter 以外にも Deserialize 可能な任意の型で引数を取得できます。
    let filter = ctx.arg::<Filter>("filter").unwrap();

    async move {
        // 非同期処理を実行
        let devices = fetch_devices(&filter).await?;
        Ok(json!({ "devices": devices }))
    }
});
```

## Channel

Tauri の Channel API に似た API。Rust から WebView へのリアルタイムデータストリーミングを可能にします。

### 基本的な使い方

#### JavaScript 側

```javascript
import { invoke, Channel } from "@novonotes/webview-bridge";

// チャンネルを作成してメッセージを受信
const ch = new Channel((message) => {
  console.log("Received event:", message);
});

// チャンネルを登録
await invoke("subscribe_events", { channel: ch });
```

#### Rust 側

```rust
handler.register_sync("subscribe_events", |ctx| {
    // 引数で渡されたチャンネルを取得
    let channel = ctx.arg::<Channel>("channel").unwrap();

    // JSONメッセージを送信
    channel.send(json!({ "type": "connected" }))?;

    // バイナリデータを送信
    let binary_data: Vec<u8> = vec![0xFF, 0xD8, 0xFF]; // 例: JPEGヘッダー
    channel.send_bytes(binary_data)?;

    Ok(json!({ "status": "subscribed" }))
});
```

#### JavaScript 側でバイナリを受信

```javascript
const channel = new Channel((message) => {
  if (message instanceof ArrayBuffer) {
    // バイナリデータとして処理
    const bytes = new Uint8Array(message);
    console.log("Received binary:", bytes);
  } else {
    // JSONデータとして処理
    console.log("Received JSON:", message);
  }
});
```

この実装は Tauri と同じパターンで、`instanceof ArrayBuffer` でバイナリデータを判別します。

## webview の作成

WebView の構築と管理を簡素化します。

```rust
use wxp::WxpWebViewBuilder;

let webview = WxpWebViewBuilder::new()
    .with_command_handler(handler)
    .with_html(HTML_CONTENT)
    .with_devtools(true)
    .build(&window)?;
```
