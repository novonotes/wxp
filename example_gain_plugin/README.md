# WXP Example Gain Plugin

wxp を使った CLAP オーディオプラグインの入門サンプルです。
入力信号にゲイン（音量倍率）を掛けるだけのシンプルなエフェクトプラグインですが、
wxp プラグイン開発に必要な要素が一通り含まれています。

## ディレクトリ構成

```
example_gain_plugin/
├── src-plugin/          # Rust（プラグイン本体）
│   ├── src/
│   │   ├── lib.rs       # CLAP エントリーポイント、RunLoop の初期化
│   │   ├── plugin.rs    # プラグイン定義、共有状態、コマンドハンドラ登録
│   │   ├── audio.rs     # オーディオ処理（リアルタイムスレッド）
│   │   ├── params.rs    # パラメータ公開、状態の保存・復元
│   │   └── gui.rs       # WebView GUI の生成・リサイズ管理
│   ├── build.rs         # リリースビルド時に GUI アセットを ZIP 化
│   └── Cargo.toml
└── src-gui/             # TypeScript + HTML/CSS（GUI フロントエンド）
    ├── index.html       # ノブ UI の HTML
    ├── src/
    │   ├── main.ts      # ノブ操作、Rust との通信ロジック
    │   └── style.css    # ノブ・パネルのスタイル
    ├── vite.config.ts
    └── package.json
```

## アーキテクチャ概要

### スレッドモデル

CLAP プラグインは主に 2 つのスレッドで動作します。

```
┌─────────────────────────────────────────────────────────────────┐
│ メインスレッド（= RunLoop スレッド）                                │
│  - GUI の生成・破棄・リサイズ (gui.rs)                             │
│  - パラメータ情報の公開 (params.rs)                                │
│  - WxpCommandHandler によるコマンド処理                            │
│  - 状態の保存・復元                                                │
│  - wxp の WebView イベント処理                                     │
│  - RunLoopSender 経由で他スレッドからタスクを受け取る                 │
│  - Channel::send() による Rust → JS 通知もここで実行               │
├─────────────────────────────────────────────────────────────────┤
│ オーディオスレッド（リアルタイム）                                    │
│  - process() でサンプルにゲインを掛ける (audio.rs)                  │
│  - ロック・メモリ割り当て・I/O は禁止                                │
└─────────────────────────────────────────────────────────────────┘
```

> **補足:** RunLoop はメインスレッド上で初期化されるため、
> このプラグインでは RunLoop スレッド＝メインスレッドです。
> `RunLoopSender` はオーディオスレッドなど別のスレッドから
> メインスレッドにクロージャをポストするために使います。

### Rust ↔ JavaScript 通信

```
JavaScript (main.ts)                    Rust (plugin.rs)
──────────────────                      ────────────────
invoke("set_gain", {value})  ──────►   WxpCommandHandler
                                        └─ register_sync("set_gain", ...)

Channel コールバック        ◄──────    RunLoopSender → Channel::send()
  └─ render(state)                      └─ notify_gui()
```

- **JS → Rust**: `invoke()` で Rust 側に登録されたコマンドを RPC 呼び出し
- **Rust → JS**: `Channel` によるプッシュ通知。ホストがオートメーション等で値を変更したとき、`RunLoopSender` 経由でメインスレッドにディスパッチし、`Channel::send()` で JS に JSON を送信

### パラメータ変更の流れ

**UI → ホスト:**

```
1. ユーザーがノブをドラッグ開始
2. JS: invoke("begin_parameter_gesture")
3. JS: invoke("set_gain", {value})          ← ドラッグ中に繰り返し
4. Rust: SharedStateInner の AtomicF32 を更新 + pending フラグを立てる
5. オーディオスレッド: process() で pending フラグを読み取り、output events としてホストに通知
6. ユーザーがドラッグ終了
7. JS: invoke("end_parameter_gesture")
```

**ホスト → UI:**

```
1. ホストがオートメーション等で値を変更
2. Rust: process() の input events から ParamValue を受け取る
3. Rust: SharedStateInner の AtomicF32 を更新
4. Rust: notify_gui() → RunLoopSender → Channel::send()
5. JS: Channel コールバックで render() が呼ばれ、UI が更新される
```

## 開発の始め方

### 前提条件

- Rust（cargo）
- Node.js（npm）

### GUI のデバッグ開発

デバッグビルドでは Vite dev server に接続するため、ホットリロードが使えます。

```sh
# 1. GUI の依存関係をインストール
cd example_gain_plugin/src-gui
npm install

# 2. Vite dev server を起動（localhost:5173）
npm run dev

# 3. 別ターミナルでプラグインをデバッグビルド
cargo build -p wxp_example_gain_plugin
```

### リリースビルド

リリースビルドでは GUI アセットがバイナリに埋め込まれます。

```sh
# 1. GUI をビルド（dist/ に出力される）
cd wxp/example_gain_plugin/src-gui
npm install
npm run build

# 2. プラグインをリリースビルド（build.rs が dist/ を ZIP 化して埋め込む）
cargo build -p wxp_example_gain_plugin --release
```

## 主要な依存クレート

| クレート | 役割 |
|---------|------|
| `clack-plugin` / `clack-extensions` | CLAP プラグイン API の Rust バインディング |
| `wxp` | WebView GUI フレームワーク（WxpWebViewBuilder, WxpCommandHandler, Channel） |
| `wxp_clack` | wxp と CLAP を繋ぐユーティリティ（DPI 変換、ウィンドウハンドル変換） |
| `novonotes_run_loop` | プラットフォーム抽象化されたイベントループ（RunLoop, RunLoopSender） |
| `wry` | WebView エンジン（wxp が内部で使用） |
| `@novonotes/webview-bridge` | JS 側の通信ライブラリ（invoke, Channel） |
