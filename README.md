# wxp

`wxp` is a WebView-based foundation for audio plugin UIs.
It lets you write plugin GUIs in HTML / CSS / TypeScript and run them on a WebView powered by [wry](https://github.com/tauri-apps/wry).
It provides Tauri-like IPC (`invoke` / `Channel`) for concise bidirectional communication between Rust and JavaScript.

## Quick Start

```rust
use std::sync::Arc;
use wxp::{WebContext, WxpCommandHandler, WxpWebViewBuilder};

let mut web_context = WebContext::new(std::env::temp_dir().join("my-plugin"))
    .build_wry_context();
let handler = Arc::new(WxpCommandHandler::new());

// `webview` must be kept alive while the UI is shown (see Caveats below).
let webview = WxpWebViewBuilder::new(&mut web_context)
    .with_command_handler(handler)
    .with_url("http://localhost:5173/")
    .build_as_child(&window)?;
```

See the [crates/wxp README](./crates/wxp/README.md) for a detailed walkthrough of the crate
(including platform support and the main-thread / lifetime caveats), and
[wrac-plugin-template](https://github.com/novonotes/wrac-plugin-template) for a full plugin project.

## Give It a Spin?

`wxp` is used in [wxp-gain-example](https://github.com/novonotes/wrac-plugin-template), which ships with a simple Gain plugin built on the WRAC stack.
Try loading it in your DAW and let us know how it works in practice.

Even a quick note like **"Works on Logic Pro 10.7"** is helpful for the community:
👉 [DAW Compatibility Reports](https://github.com/novonotes/wrac-plugin-template/discussions/6)

## Repository Structure

| Path | Description |
|-----|------|
| `crates/wxp` | WebView UI foundation (main crate) |
| `crates/wxp_clack` | Integration utilities for CLAP (clack) and wxp |
| `crates/host_window` | Dev dependency for wxp. Not intended for external use. |
| `packages/webview-bridge` | JS/TS IPC bridge (`@novonotes/webview-bridge`) |

## Project Status

The current status is **alpha** (`0.1.0-alpha.x`).
wxp is used in production by NovoNotes, but the public API is still stabilizing —
expect breaking changes between alpha releases.

## Installation

- The Rust crate is not published to crates.io. Use it with a `git` + `rev` pin.
- `@novonotes/webview-bridge` has not been published to npm yet. It is distributed as a tarball via GitHub Releases.

Example Cargo configuration:
```toml
[dependencies]
wxp = { git = "https://github.com/novonotes/wxp.git", rev = "<latest commit hash on main branch>" }
wxp_clack = { git = "https://github.com/novonotes/wxp.git", rev = "<latest commit hash on main branch>" }
```

npm installation:
```sh
npm install https://files.novonotes.download/libs/novonotes-webview-bridge-0.1.0-alpha.1.tgz
```

## License

MIT
