# wxp

`wxp` は WebView ベースのオーディオプラグイン UI 基盤です。
HTML / CSS / TypeScript でプラグイン GUI を記述し、[wry](https://github.com/tauri-apps/wry) をベースにした WebView 上で動作させます。
Tauri に似た IPC（`invoke` / `Channel`）を提供し、Rust と JavaScript の双方向通信を簡潔に記述できます。

## Getting Started

[Getting Started](docs/getting-started.md) に `wxp-examples` を使った新規プロジェクト作成手順を記載しています。

## このリポジトリの構成

| パス | 内容 |
|-----|------|
| `crates/wxp` | WebView UI 基盤（メインクレート） |
| `crates/wxp_clack` | CLAP（clack）と wxp の統合ユーティリティ |
| `crates/host_window` | wxp の dev-dependency。外部利用は想定されていません。 |
| `packages/webview-bridge` | JS/TS 側 IPC ブリッジ（`@novonotes/webview-bridge`） |
| `wxp-examples` | サンプル・参照実装は別リポジトリで管理 |

## プロジェクトのステータス

- 現時点のステータスは alpha です。
- Rust クレートは `git` + `rev` 固定で利用してください。
- `@novonotes/webview-bridge` は当面 npm publish せず、GitHub Releases で tarball 配布します。

cargo の設定例:
```toml
[dependencies]
wxp = { git = "https://github.com/novonotes/wxp.git", rev = "<main ブランチの最新コミットハッシュ>" }
wxp_clack = { git = "https://github.com/novonotes/wxp.git", rev = "<main ブランチの最新コミットハッシュ>" }
```

npm のインストール方法:
```sh
npm install https://github.com/novonotes/wxp/releases/download/v0.1.0-alpha.1/novonotes-webview-bridge-0.1.0-alpha.1.tgz
```

## ライセンス

MIT
