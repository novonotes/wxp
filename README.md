# wxp

`wxp` is a WebView-based foundation for audio plugin UIs.
It lets you write plugin GUIs in HTML / CSS / TypeScript and run them on a WebView powered by [wry](https://github.com/tauri-apps/wry).
It provides Tauri-like IPC (`invoke` / `Channel`) for concise bidirectional communication between Rust and JavaScript.

## Usage

See the [crates/wxp README](./crates/wxp/README.md) for instructions on using the wxp crate.

For an introductory guide to plugin development with wxp, see [wxp-gain-example](https://github.com/novonotes/wxp-gain-example).

## Repository Structure

| Path | Description |
|-----|------|
| `crates/wxp` | WebView UI foundation (main crate) |
| `crates/wxp_clack` | Integration utilities for CLAP (clack) and wxp |
| `crates/host_window` | Dev dependency for wxp. Not intended for external use. |
| `packages/webview-bridge` | JS/TS IPC bridge (`@novonotes/webview-bridge`) |

## Project Status

The current status is alpha.

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
