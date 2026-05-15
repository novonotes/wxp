# run_loop_timer

`novonotes_run_loop` 上で繰り返し処理を実行する小さな timer crate です。

callback は run loop thread 上で実行され、`Send` を要求しません。native GUI や WebView のように、特定の thread からしか操作できない object の定期更新に使用できます。

```rust
use run_loop_timer::Timer;
use std::time::Duration;

let timer = Timer::new(Duration::from_millis(100), || {
    // run loop thread 上で実行される
});
timer.start();
timer.stop();
```

async callback も利用できます。

```rust
let timer = Timer::new_async(Duration::from_millis(100), || async {
    // RunLoop::current().spawn(...) で実行される
});
timer.start();
```

## 前提

- `RunLoop::init()` 済みの thread 上で作成、開始、停止、破棄する
- drop 時に次回の schedule は cancel される
- 実行中の async task は timer 停止後も継続する
- タイミング精度は platform と run loop 負荷に依存する
