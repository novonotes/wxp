# @novonotes/webview-bridge

A TypeScript library that provides a unified interface for IPC communication between frontend code running inside a WebView and the backend.

## Overview

This package provides a unified IPC communication API across different WebView environments (Tauri, wxp). It automatically detects the runtime environment and uses the appropriate backend API, enabling frontend code reuse.

### Supported Backends

- **Tauri** - Tauri applications
- **wxp** - WebView X Plugin (for audio plugin development)

## Usage

For the time being, distribution is done via tarball rather than npm publish.

```sh
npm install
npm run build
npm pack
```

For integration with the Rust side, refer to the documentation for [wxp](https://github.com/novonotes/wxp) or [tauri](https://github.com/tauri-apps/tauri).

### Command

```typescript
import { invoke, Channel } from "@novonotes/webview-bridge";

// Invoke a command
const result = await invoke("greet", { name: "World" });
console.log(result); // "Hello, World!"
```

### Channel

```typescript
import { invoke, Channel } from "@novonotes/webview-bridge";

// Streaming channel
const channel = new Channel<MyMessageType>((message) => {
  console.log("Received:", message);
});

await invoke("start_streaming", { channel });
```

### Environment Detection

```typescript
import { detectEnvironment } from "@novonotes/webview-bridge";

const env = detectEnvironment();
console.log("Current environment:", env); // 'tauri' | 'wxp' | 'unknown'
```
