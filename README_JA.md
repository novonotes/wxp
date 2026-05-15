# wxp

`wxp` は WebView ベースのオーディオプラグイン UI 基盤です。
HTML / CSS / TypeScript でプラグイン GUI を記述し、[wry](https://github.com/tauri-apps/wry) をベースにした WebView 上で動作させます。
Tauri に似た IPC（`invoke` / `Channel`）を提供し、Rust と JavaScript の双方向通信を簡潔に記述できます。

## クイックスタート

```rust
use std::rc::Rc;
use wxp::{WebContext, WxpCommandHandler, WxpWebViewBuilder};

let mut web_context = WebContext::new(std::env::temp_dir().join("my-plugin"));
let handler = Rc::new(WxpCommandHandler::new());

// `webview` は UI を表示している間 drop させないこと（後述の Caveats を参照）。
let webview = WxpWebViewBuilder::new(&mut web_context)
    .with_command_handler(handler)
    .with_url("http://localhost:5173/")
    .build_as_child(&window)?;
```

## Caveats

- `WxpWebView` は native WebView の寿命を所有し、作成した run loop thread から動かさない設計です。
- background thread などから WebView 操作を post したい場合は `WebViewDispatch` を clone します。
  `WebViewDispatch` は `Send + Sync` ですが WebView の寿命は延ばさず、結果待ちではなく enqueue します。
- UI を表示している間は `WxpWebView` を保持してください。drop すると native WebView は閉じられ、
  stale な command/channel work は UI 寿命を延ばさずに無視されます。

詳しい使い方は [crates/wxp の README](./crates/wxp/README.md)、
プラグイン全体を通した例は [wrac-plugin-template](https://github.com/novonotes/wrac-plugin-template) を参照してください。

## このリポジトリの構成

| パス | 内容 |
|-----|------|
| `crates/run_loop` | wxp とプラグイン UI helper が使う、プラットフォーム独立の run loop |
| `crates/run_loop_timer` | `novonotes_run_loop` 上で動く軽量な繰り返し timer helper |
| `crates/wxp` | WebView UI 基盤（メインクレート） |
| `crates/wry` | upstream ベースの同梱 wry crate。プラグイン host lifecycle 向け修正を含みます。 |
| `crates/host_window` | wxp の dev-dependency。外部利用は想定されていません。 |
| `packages/webview-bridge` | JS/TS 側 IPC ブリッジ（`@novonotes/webview-bridge`） |

`crates/wry` は `tauri-apps/wry` を追従しつつ、通常のデスクトップアプリとは異なる
オーディオプラグイン host の editor 作成・親 window 接続・focus 順序に対応するための
意図ベースの小さな patch を保持します。

## プロジェクトのステータス

現時点のステータスは **alpha** (`0.1.0-alpha.x`) です。
wxp は NovoNotes のプロダクションで使用していますが、公開 API はまだ安定化の途上で、
alpha リリース間での非互換変更があり得ます。

## インストール方法

- Rust クレートは crate.io 未公開です。`git` + `rev` 固定で利用してください。
- `@novonotes/webview-bridge` はまだ npm publish していません。GitHub Releases などで tarball 配布します。

cargo の設定例:
```toml
[dependencies]
wxp = { git = "https://github.com/novonotes/wxp.git", rev = "<main ブランチの最新コミットハッシュ>" }
run_loop_timer = { git = "https://github.com/novonotes/wxp.git", package = "run_loop_timer", rev = "<main ブランチの最新コミットハッシュ>" }
```

npm のインストール方法:
```sh
npm install https://files.novonotes.download/libs/novonotes-webview-bridge-0.1.0-alpha.1.tgz
```

## ライセンス

MIT
