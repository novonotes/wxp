# wxp クレートの examples について

このディレクトリには wxp クレート自体の動作確認・開発用のスタンドアローンアプリサンプルが含まれています。

**プラグイン開発者はこれらのサンプルを参照する必要はありません。**
プラグイン開発の出発点としては [`examples/gain_plugin`](../../../examples/gain_plugin/README.md) を参照してください。

## サンプル一覧

| ファイル | 説明 |
|---------|------|
| `run_loop_command_demo.rs` | `novonotes_run_loop` バックエンドで Command API を使うデモ |
| `run_loop_channel_demo.rs` | `novonotes_run_loop` バックエンドで Channel API を使うデモ |
| `tao_command_demo.rs` | `tao` バックエンドで Command API を使うデモ |
| `tao_channel_demo.rs` | `tao` バックエンドで Channel API を使うデモ |
| `winit_command_demo.rs` | `winit` バックエンドで Command API を使うデモ |
| `winit_channel_demo.rs` | `winit` バックエンドで Channel API を使うデモ |

3 種類のウィンドウバックエンド（run_loop / tao / winit）それぞれで Command と Channel の基本動作を検証するためのものです。
