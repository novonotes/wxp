# Getting Started

`examples/gain_plugin` をテンプレートとして新しい wxp プラグインを作成する手順を説明します。

## 前提条件

- **Rust**（最新の stable）
- **Node.js**（npm）
- macOS / Windows / Linux のいずれか

デバッグには VS Code + [CodeLLDB](https://marketplace.visualstudio.com/items?itemName=vadimcn.vscode-lldb) 拡張が必要です。

## 1. リポジトリのセットアップ

このガイドでは **wxp リポジトリ内に新しいプラグインを追加する**方法を説明します。
まず wxp リポジトリ本体をクローンしてください。

```sh
git clone https://github.com/novonotes/wxp.git
cd wxp
git submodule update --init --recursive
```

## 2. gain_plugin をテンプレートとして新規プロジェクトを作成

`examples/gain_plugin` をそのままコピーして、必要な箇所を書き換えます。

```sh
cp -r examples/gain_plugin examples/my_plugin
```

## 3. 書き換えが必要な箇所

### src-plugin/Cargo.toml

```toml
[package]
name = "my_plugin"          # ← プロジェクト名に変更
```

### src-plugin/src/plugin.rs

プラグイン ID とプラグイン名を変更します。

```rust
pub(crate) const PLUGIN_ID: &str = "com.your-company.my-plugin";  // ← 逆ドメイン形式で一意に
pub(crate) const PLUGIN_NAME: &str = "My Plugin";                   // ← ホストに表示される名前
```

> **重要:** `PLUGIN_ID` はプラグインのライフサイクル全体を通じてグローバルに一意である必要があります。一度公開したら変更できません。

### src-plugin/src/gui.rs

WebView のデータ保存先（localStorage キャッシュ等）を新しいプラグイン名に変更します。

```rust
let data_dir = std::env::temp_dir().join("my-plugin");  // ← プラグイン名に変更
```

### src-gui/package.json

```json
{
  "name": "my-plugin-gui"   // ← プロジェクト名に変更
}
```

### src-gui/index.html

`<title>` タグや UI ラベルをプラグイン名に変更します。

## 4. ワークスペースへの追加

ルートの `Cargo.toml` に新しいクレートを追加します。

```toml
[workspace]
members = [
    # ...既存のエントリ...
    "examples/my_plugin/src-plugin",
]
```

## 5. ビルド確認

```sh
cargo check --workspace --all-targets
```

## 6. ビルド & インストール

```sh
cd examples/my_plugin
./script/build_and_install.sh
```

インストール先は OS によって異なります:

| OS | インストール先 |
|----|--------------|
| macOS | `~/Library/Audio/Plug-Ins/CLAP/` |
| Windows | `%LOCALAPPDATA%/Programs/Common/CLAP/` |
| Linux | `~/.clap/` |

macOS では VST3 / AU も同時にインストールされます。

## 7. デバッグ

### VS Code でスタンドアローンアプリとしてデバッグ

DAW を使わずにプラグインを単体アプリとして起動し、デバッガーをアタッチできます。

> **注意:** スタンドアローンモードでは音声フィードバックがあります。**ヘッドフォンを使用してください。**

1. VS Code で「Debug gain_plugin standalone」構成を選択して実行します。
   - プラグインのビルド → スタンドアローンアプリ起動 → デバッガーアタッチ が自動で行われます。
   - `RUST_BACKTRACE=1` が設定されるためパニック時のスタックトレースも確認できます。

### GUI のホットリロード

デバッグビルドでは Vite dev server（`localhost:5173`）に接続するため、GUI をホットリロードしながら開発できます。

```sh
# 1. GUI の依存関係をインストール & Vite dev server を起動
cd examples/my_plugin/src-gui
npm install
npm run dev

# 2. 別ターミナルでプラグインをビルド & インストール
cd examples/my_plugin
./script/build_and_install.sh
```

プラグインをインストール後、DAW またはスタンドアローンアプリで開けば GUI が自動的に Vite dev server から読み込まれます。

## 次のステップ

- [concepts.md](./concepts.md) — wxp の主要概念（WxpWebViewBuilder・WxpCommandHandler・Channel）
- [examples/gain_plugin/README.md](../examples/gain_plugin/README.md) — スレッドモデル・通信フロー・パラメータ変更フローの詳細
