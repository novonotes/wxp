# wxp

`wxp` は WebView ベースのプラグイン UI 基盤です。

このリポジトリには以下を含みます。

- `wxp`
- `wxp_clack`
- `host_window`
- `@novonotes/webview-bridge`
- `wry` fork（submodule）

## 方針

- 現時点では alpha 前提です。
- Rust クレートは `git` + `rev` 固定で利用してください。
- `@novonotes/webview-bridge` は当面 npm publish せず、tarball 配布を前提にします。
- API 安定後に `crates.io` / npm publish を検討します。

## セットアップ

```sh
git clone https://github.com/novonotes/wxp.git
cd wxp
git submodule update --init --recursive
```

### Rust

`run_loop` は別リポジトリから `git` + `rev` 固定で参照しています。

```sh
cargo check --workspace --all-targets
```

### JavaScript

`webview-bridge` は単体 package として管理しています。

```sh
cd webview-bridge
npm install
npm run build
npm pack
```

生成された tarball を利用側でインストールしてください。

## CI

GitHub Actions で以下を実行します。

- Rust: `cargo check --workspace --all-targets`
- Rust: `cargo test --workspace --lib`
- JavaScript: `npm run build`

## ライセンス

MIT
