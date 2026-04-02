# wxp_clack

CLAP プラグイン基盤である [clack](https://github.com/prokopyl/clack) と wxp を統合するためのユーティリティクレートです。

wxp は特定のプラグインフレームワークに依存しない汎用の WebView UI 基盤です。clack の型（`GuiSize`、`Window` など）と wxp / wry が扱う型の間には変換が必要です。このクレートはその橋渡しを担います。

具体的な使用例は [`wxp-gain-example`](https://github.com/novonotes/wxp-gain-example/blob/main/README.md) を参照してください。
