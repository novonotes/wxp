# @novonotes/webview-bridge

The TypeScript-side IPC bridge for [wxp](https://github.com/novonotes/wxp).
It exposes `invoke` and `Channel` for communicating with the Rust side of a wxp WebView.

## Installation

The package is not yet published to npm. Install it from the tarball distributed via
GitHub Releases:

```sh
npm install https://files.novonotes.download/libs/novonotes-webview-bridge-0.1.0-alpha.1.tgz
```

## Usage

For detailed usage of `invoke` and `Channel` including matching Rust-side code, see the
[wxp README](../../crates/wxp/README.md).

## Building the package (for maintainers)

If you need to produce a tarball yourself (for example, to cut a new release):

```sh
npm install
npm run build
npm pack
```
