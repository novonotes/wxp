# WXP Example Gain Plugin

wxp を使った CLAP オーディオプラグインの入門サンプルです。
入力信号にゲイン（音量倍率）を掛けるだけのシンプルなエフェクトプラグインですが、
wxp プラグイン開発に必要な要素が一通り含まれています。

## ディレクトリ構成

```
examples/gain_plugin/
├── script/                # ビルド・インストール用スクリプト
├── src-plugin/            # Rust（プラグイン本体）
└── src-gui/               # TypeScript + HTML/CSS（GUI フロントエンド）
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

### ビルド & インストール

`script/` 配下のスクリプトで GUI ビルド → cargo ビルド → .clap バンドル作成に加え、
macOS / Windows では VST3 / AU / standalone の補助ビルドも行えます。

```sh
# デバッグビルド＆インストール（引数省略時は Debug ビルド）
./script/build_and_install.sh

# CLAP のみビルド＆インストール
CLAP_ONLY=1 ./script/build_and_install.sh

# standalone のみビルド
./script/build_standalone.sh
```

インストール先は OS によって異なります:

| OS | インストール先 |
|----|--------------|
| macOS | `~/Library/Audio/Plug-Ins/CLAP/` |
| Windows | `%LOCALAPPDATA%/Programs/Common/CLAP/` |
| Linux | `~/.clap/` |

macOS では `build_and_install.sh` 実行時に `~/Library/Audio/Plug-Ins/VST3/` と
`~/Library/Audio/Plug-Ins/Components/` へのインストールも行います。

### GUI のデバッグ開発

デバッグビルドでは Vite dev server に接続するため、ホットリロードが使えます。

```sh
# 1. GUI の依存関係をインストール & Vite dev server を起動（localhost:5173）
cd examples/gain_plugin/src-gui
npm install
npm run dev

# 2. 別ターミナルでプラグインをデバッグビルド＆インストール
./script/build_and_install.sh
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
