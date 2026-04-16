# wxp

`wxp` は WebView ベースのオーディオプラグイン UI 基盤です。
HTML / CSS / TypeScript でプラグイン GUI を記述し、[wry](https://github.com/tauri-apps/wry) をベースにした WebView 上で動作させます。
Tauri に似た IPC（`invoke` / `Channel`）を提供し、Rust と JavaScript の双方向通信を簡潔に記述できます。

## 利用方法

wxp クレートの利用方法は、 [crates/wxp の README](./crates/wxp/README.md) にまとめています。

また、wxp を用いたプラグイン開発を俯瞰するための入門用ドキュメントとして、[wxp-gain-example](https://github.com/novonotes/wxp-gain-example) を用意しています。

## このリポジトリの構成

| パス | 内容 |
|-----|------|
| `crates/wxp` | WebView UI 基盤（メインクレート） |
| `crates/wxp_clack` | CLAP（clack）と wxp の統合ユーティリティ |
| `crates/host_window` | wxp の dev-dependency。外部利用は想定されていません。 |
| `packages/webview-bridge` | JS/TS 側 IPC ブリッジ（`@novonotes/webview-bridge`） |

## プロジェクトのステータス

現時点のステータスは alpha です。

## インストール方法

- Rust クレートは crate.io 未公開です。`git` + `rev` 固定で利用してください。
- `@novonotes/webview-bridge` はまだ npm publish していません。GitHub Releases などで tarball 配布します。

cargo の設定例:
```toml
[dependencies]
wxp = { git = "https://github.com/novonotes/wxp.git", rev = "<main ブランチの最新コミットハッシュ>" }
wxp_clack = { git = "https://github.com/novonotes/wxp.git", rev = "<main ブランチの最新コミットハッシュ>" }
```

npm のインストール方法:
```sh
npm install https://files.novonotes.download/libs/novonotes-webview-bridge-0.1.0-alpha.1.tgz
```

## ライセンス

MIT
