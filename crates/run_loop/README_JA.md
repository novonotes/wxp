# novonotes_run_loop

> English: [README.md](./README.md)

プラットフォーム独立なランループインターフェースを提供する Rust クレート。
[irondash_run_loop](https://github.com/irondash/irondash/tree/main/run_loop) をベースに、DLL 環境での安全性やエラーハンドリングなどを強化したフォークです。

## 目的

オーディオアプリケーション/プラグイン開発における非同期タスク管理での使用を想定。
ホストアプリケーションのメインスレッドをブロックすることなく、そのスレッドのランループへのアクセスを提供する。
これにより、プラグイン起点でメインスレッドのタスクを起動・スケジューリングすることが可能になる。

## 特徴

- **マルチプラットフォーム対応**: iOS/macOS、Android、Linux、Windows のネイティブランループを統一 API で操作
- **非同期タスク管理**: Rust 標準の async/await パターンをサポート
- **スレッド間通信**: 安全なメッセージパッシング機構
- **DLL/オーディオプラグイン対応**: 他社アプリケーションにリンクされた DLL で動作するユースケースをサポート

## 基本的な使い方

### 初期化

```rust
use novonotes_run_loop::{RunLoop, JoinError};

// アプリケーション/DLL の初期化処理で実行
RunLoop::init().expect("RunLoop の初期化に失敗");

// 現在のスレッドのRunLoopを取得
let run_loop = RunLoop::current();

// アプリケーション/DLL の終了処理で実行
RunLoop::deinit();
```

### タスクのスケジューリング

```rust
use std::time::Duration;

let run_loop = RunLoop::current();

// 10秒後の実行をスケジュール
let handle = run_loop.schedule(Duration::from_secs(10), || {
    println!("10秒経過しました");
});

// handleをドロップするとタイマーがキャンセルされる
// キャンセルを防ぐには detach() を使用
handle.detach();
```

### 非同期タスクの実行

```rust
// タスクをスポーンして結果を待機
let handle = run_loop.spawn(async {
    // 非同期処理
    RunLoop::current().delay(Duration::from_secs(1)).await;
    42
});

// 結果の取得（エラーハンドリング付き）
match handle.await {
    Ok(value) => println!("結果: {}", value),  // `結果: 42` が出力されるはず。
    Err(JoinError::Aborted) => println!("タスクが中断されました"),
    Err(JoinError::Panic(_)) => println!("タスクがパニックしました"),
}
```

### スレッド間通信

RunLoop は初期化されたスレッドを RunLoop スレッドとしてマークします。
別スレッドから RunLoop スレッドへのコールバック送信は `RunLoop::sender()` を使用します。

```rust
use std::thread;

fn main() {
    assert!(RunLoop::is_run_loop_thread());

    // 別スレッドからRunLoopスレッドにコールバックを送信
    thread::spawn(move || {
        let sender = RunLoop::sender();
        // 送信されたコールバックは RunLoop スレッドで非同期実行されます。
        sender.send(|| {
            assert!(RunLoop::is_run_loop_thread());
            println!("RunLoopスレッドで実行");
        });
    });
}
```


## プラットフォーム別の実装

| プラットフォーム | 基盤技術           | 特徴                                              |
| ---------------- | ------------------ | ------------------------------------------------- |
| iOS/macOS        | CFRunLoop          | Core Foundation ベース、カスタム RunLoopMode 使用 |
| Android          | ALooper            | NDK の ALooper、timerfd でタイマー実装            |
| Linux            | GMainContext       | GLib/GTK 統合、g_timeout_source でタイマー        |
| Windows          | Win32 Message Loop | 隠しウィンドウでメッセージ処理                    |

## 動作モデル

`init()` は現在のスレッドにネイティブループの基盤への参照を取得（存在しない場合は新規作成）します。スタンドアロンアプリでは自前での `run()` 呼び出しによって、ランループを駆動する必要があります。プラグイン環境ではホストが既にループを駆動しているため、`run()` の呼び出しは不要です。その後のコールバックやタイマー登録は、誰がループを駆動しているかに関係なく同じように動作します。

| パターン | `run()` | 誰がループを回すか |
|---|---|---|
| スタンドアロンアプリ | 呼ぶ | `run()` 自身 |
| プラグイン（CLAP/VST3 等） | 呼ばない | ホスト（DAW）の既存ループ |

## irondash_run_loop との主な違い

1. **DLL セーフティ**: thread-local ストレージを使用しないように変更。初期化・終了処理を見直し。複数 DLL 間での Win32 の Window Class 名や CFRunLoop の RunLoopMode 名の衝突を回避。
2. **明示的な abort() メソッド**: タスクの制御された中断が可能
3. **パニックリカバリ**: タスク内のパニックをキャッチして報告

## テスト

run_loop を利用したコードをテストする際に便利なヘルパーやテストハーネスを用意しています。  
使い方については、[テストガイド](docs/testing.md)を参照してください。

## プロジェクトのステータス

現時点のステータスは **alpha** です。
NovoNotes のプロダクションで使用していますが、公開 API はまだ安定化の途上で、破壊的変更があり得ます。

## インストール方法

- Rust クレートは crate.io 未公開です。`git` + `rev` 固定で利用してください。

## ライセンス

MIT License（オリジナルプロジェクトと同じ）

## Upstream

このリポジトリは [`irondash`](https://github.com/irondash/irondash) の
`run_loop` クレートをベースにしたフォークです。
