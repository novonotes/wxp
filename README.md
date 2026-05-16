# wxp

`wxp` is a WebView-based foundation for audio plugin UIs.
It lets you write plugin GUIs in HTML / CSS / TypeScript and run them on a WebView powered by [wry](https://github.com/tauri-apps/wry).
It provides WXP IPC (`invoke` / `Channel`) for concise bidirectional communication between Rust and JavaScript.

> 日本語: [README_JA.md](./README_JA.md)

## Quick Start

```rust
use std::rc::Rc;
use wxp::{WebContext, WxpCommandHandler, WxpWebViewBuilder};

let mut web_context = WebContext::new(std::env::temp_dir().join("my-plugin"));
let handler = Rc::new(WxpCommandHandler::new());

// `webview` must be kept alive while the UI is shown (see Caveats below).
let webview = WxpWebViewBuilder::new(&mut web_context)
    .with_command_handler(handler)
    .with_url("http://localhost:5173/")
    .build_as_child(&window)?;
```

## Caveats

- `WxpWebView` owns the native WebView and must stay on the run loop thread that created it.
- Clone `WebViewDispatch` when background work needs to post WebView operations. It is `Send + Sync`,
  but it does not keep the WebView alive and its methods enqueue work instead of waiting for results.
- Keep `WxpWebView` alive for as long as the UI should exist. Dropping it closes the native WebView,
  and stale command/channel work is ignored instead of extending the UI lifetime.

See the [crates/wxp README](./crates/wxp/README.md) for a detailed walkthrough of the crate
(including platform support and the main-thread / lifetime caveats), and
[wrac-plugin-template](https://github.com/novonotes/wrac-plugin-template) for a full plugin project.

## Repository Structure

| Path | Description |
|-----|------|
| `crates/run_loop` | Platform-independent run loop used by wxp and plugin UI helpers |
| `crates/run_loop_timer` | Lightweight repeating timer helper built on `novonotes_run_loop` |
| `crates/wxp` | WebView UI foundation (main crate) |
| `crates/wry` | Embedded upstream-based wry crate with plugin-host lifecycle fixes |
| `crates/host_window` | Dev dependency for wxp. Not intended for external use. |
| `packages/webview-bridge` | JS/TS IPC bridge (`@novonotes/webview-bridge`) |

`crates/wry` tracks `tauri-apps/wry` but keeps small intent-based patches for audio plug-in hosts,
where editor creation, parent-window attachment, and focus can happen in a different order than in a
normal desktop application.

## Project Status

The current status is **alpha** (`0.1.0-alpha.x`).
wxp is used in production by NovoNotes, but the public API is still stabilizing —
expect breaking changes between alpha releases.

## Installation

- The Rust crate is not published to crates.io. Use it with a `git` + `rev` pin.
- `@novonotes/webview-bridge` is a private package for WXP WebView frontends. Use it from this repository workspace or from an internal tarball.

Example Cargo configuration:
```toml
[dependencies]
wxp = { git = "https://github.com/novonotes/wxp.git", rev = "<latest commit hash on main branch>" }
run_loop_timer = { git = "https://github.com/novonotes/wxp.git", package = "run_loop_timer", rev = "<latest commit hash on main branch>" }
```

npm installation from a local checkout:
```sh
npm install /path/to/wxp/packages/webview-bridge
```

## License

MIT
