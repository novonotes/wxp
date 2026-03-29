# run_loop テストガイド

run_loop を使ったテストには、通常のテストヘルパーと GUI 用のテストハーネスの 2 種類があります。

## test_helper（通常の非同期テスト用）

非同期コードをテストする際に使用します。RunLoop の初期化・実行・終了を自動的に処理します。

### 使い方

```rust
use novonotes_run_loop::test_helper as test;
use serial_test::serial;

#[test]
#[serial]  // 複数テストの場合、直列実行が必須。
fn test_example() {
    test::run_async(async {
        // ここに非同期テストコードを書く
        RunLoop::current().delay(Duration::from_millis(10)).await;
        42  // 任意の型を返せる
    });
}
```

- **必ず `#[serial]` を付ける**: RunLoop は複数スレッドによる同時実行をサポートしません。
- **戻り値は自由**: `Result<T, E>` や任意の型を返せます
- **パニックも処理**: テスト内のパニックは適切にキャッチされ、テスト失敗として報告されます

## test_harness（GUI 統合テスト用）

macOS/iOS などで GUI 操作が必要なテストは、メインスレッドで実行する必要があります。この場合は標準テストハーネスを無効化して、専用のハーネスを使います。

### 設定

Rust の標準ハーネスはテストをメインスレッドで実行しないため、無効化する必要があります。

```toml
# Cargo.toml
[[test]]
name = "gui_test"
path = "tests/gui_test.rs"
harness = false  # 標準ハーネスを無効化
```

### 使い方

```rust
use novonotes_run_loop::test_harness::run_gui_tests;

fn main() {
    run_gui_tests(vec![
        ("test_name", test_function),
        // 複数のテストを追加可能
    ]);
}

fn test_function() -> Result<(), String> {
    RunLoop::current().schedule(Duration::ZERO, move || {
        // 何かしらの GUI テストコード
        assert_eq!(1 + 1, 2);

        RunLoop::current().stop_app();
    })
    .detach();

    // run_app は stop_app が呼ばれるまで処理をブロックする。
    RunLoop::current().run_app();
    Ok(())
}
```
