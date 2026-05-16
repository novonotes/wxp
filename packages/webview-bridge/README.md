# @novonotes/webview-bridge

The TypeScript-side IPC bridge for [wxp](https://github.com/novonotes/wxp).
It exposes `invoke` and `Channel` for communicating with the Rust side of a wxp WebView.

## Installation

This package is private to the wxp repository and is not published to npm.
Use it from this repository workspace or from an internal tarball when needed.

```sh
npm install /path/to/wxp/packages/webview-bridge
```

## Usage

For detailed usage of `invoke` and `Channel` including matching Rust-side code, see the
[wxp README](../../crates/wxp/README.md).

## Building the package (for maintainers)

If you need to produce an internal tarball:

```sh
npm install
npm run build
npm pack
```
