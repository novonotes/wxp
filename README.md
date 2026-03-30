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
- 初回の公開版は `v0.1.0-alpha.1` を想定しています。

## セットアップ

```sh
git clone https://github.com/novonotes/wxp.git
cd wxp
git submodule update --init --recursive
```

### Rust

`run_loop` は将来的に別リポジトリを `git` + `rev` 固定で参照する前提です。
現時点では GitHub Actions での公開 CI を通すため、このリポジトリ内に vendored copy を同梱しています。

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

GitHub Releases に添付された tarball を使う場合は、例えば次のようにインストールします。

```sh
npm install https://github.com/novonotes/wxp/releases/download/v0.1.0-alpha.1/novonotes-webview-bridge-0.1.0-alpha.1.tgz
```

## CI

GitHub Actions で以下を実行します。

- Rust: `cargo check --workspace --all-targets`
- Rust: `cargo test --workspace --lib`
- JavaScript: `npm run build`

## ライセンス

MIT
