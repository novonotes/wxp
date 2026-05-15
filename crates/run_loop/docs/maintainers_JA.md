# run_loop 保守・設計メモ

この文書は **保守・移植を行う人向け** です。利用者向けのドキュメントは
[README.md](../README.md) と [lib.rs のクレートドキュメント](../src/lib.rs) を参照してください。

---

## upstream との関係

[irondash_run_loop](https://github.com/irondash/irondash/tree/main/run_loop) をベースに
フォークしたクレートですが、**upstream との継続的な sync は行わない方針**です。

irondash はマルチプラットフォーム Flutter プラグイン開発を目的としていますが、
このクレートはオーディオプラグイン（CLAP/VST3）固有の要件（DLL セーフティ、
DAW ホストへの panic 伝播防止など）に最適化する方向で独自に進化させます。

---

## irondash からの主な差分

| 変更 | 理由 |
|---|---|
| thread-local ストレージを廃止、グローバル singleton に変更 | DLL unload 時の TLS destructor 問題を回避（後述） |
| `init()` / `deinit()` の参照カウント方式 | CLAP の `init` / `deinit` と対応させるため（後述） |
| Win32 の Window Class 名、CFRunLoop の RunLoopMode 名を固有名に変更 | 同一プロセスに複数 DLL が読み込まれたときの名前衝突を回避 |
| `abort()` メソッドを追加 | タスクの制御された中断が可能に |
| task 内 panic を `catch_unwind` でキャッチ | DAW ホストを巻き込まないため（後述） |
| `block_on()` を追加 | CLAP GUI スレッドで同期的に Future を待機するため（後述） |

---

## singleton 制約

プロセス内で run loop スレッドは **常に 1 本だけ** という制約があります。

Darwin の `CFRunLoop` や Linux の `GMainContext` は「現在のスレッドの run loop」
というスレッドローカルな概念を前提とした API です。複数スレッドがそれぞれ
run loop を持つ設計も可能ですが、オーディオプラグインでは「GUI スレッド = run loop
スレッド」という対応で十分であり、複雑さを増やすメリットがありません。

テスト環境では任意のスレッドを run loop スレッドに指定できるため、
必ず `#[serial_test::serial]` で直列化してください。

---

## `init` / `deinit` の設計

参照カウント方式（`INIT_COUNT: AtomicUsize`）を採用しています。

CLAP / VST3 では `InitDll` / `ExitDll`（または `init` / `deinit`）が
**複数回呼ばれることがある**ためです（複数プラグインが同一 DLL を参照する場合など）。
`INIT_COUNT` が 0→1 になったときに実際の初期化、1→0 になったときにクリーンアップが走ります。

誤用パターン:
- `deinit()` を `init()` より多く呼ぶ → カウントがアンダーフローし、次の `init()` で
  クリーンアップ済みインスタンスを参照するリスクがあります（`fetch_sub` の wrap-around のため検出が困難）。
- `deinit()` を呼ばずに DLL がアンロードされる → `RunLoopInner::drop` でフォールバック処理が
  走りますが、ベストエフォートです。shutdown パスでは panic を起こさないことが重要です。

---

## TLS を避けている理由

thread-local ストレージは DLL unload 時の destructor の順序・タイミングの制御が難しいです。
特に Windows では `DLL_THREAD_DETACH` / `DLL_PROCESS_DETACH` の順序がホスト依存であり、
他の TLS にアクセスする destructor がクラッシュする既知の問題があります。

また、同一プロセス内で別 DLL やホスト側コードが同じスレッドを使い続ける状況では、
TLS に保持していた値がアンロード済み DLL 側のコード・データを参照してしまう危険があります。

そのため、`RUN_LOOP_INSTANCE` と `RUN_LOOP_THREAD_ID` は `static` な
グローバル変数（`Mutex` でガード）として保持しています。

---

## `block_on` の意図

`pollster::block_on` などの外部 executor は **run loop を駆動しません**。
そのため、`spawn` で投入したタスクが完了するまで待ちたい場合、外部 executor を使うと
デッドロックします。

`RunLoop::block_on` はプラットフォーム固有のポーリング（`platform_run_loop.poll_once`）を
回し続けながら Future を poll します。これにより、待機対象の Future が run loop 上の
別タスクの完了に依存していても正しく進行できます。

ネストした `block_on` は `BLOCK_ON_ACTIVE` フラグで検出し、パニックさせます（再入によるデッドロックを防ぐため）。

---

## platform backend の責務

各プラットフォームの backend（`src/platform/`）は以下を実装します：

- `PlatformRunLoop` — run loop の作成・破棄・ポーリング（`poll_once`）
- `PlatformRunLoopSender` — 他スレッドからコールバックをキューに投入
- `PollSession` — `block_on` 内でのポーリング状態管理

新しいプラットフォームを追加する場合は `src/platform/mod.rs` の
`cfg` 分岐と `PollSession` の実装を参照してください。

---

## 変更時の注意

- **shutdown パスで panic しない**: DAW ホストを巻き込みます。`catch_unwind` で囲むか、
  panic が起きない実装にしてください。
- **main thread 判定ロジックを軽率に変えない**: `RUN_LOOP_THREAD_ID` の取得・比較は
  複数箇所で行われており、変更すると `sender()`、`current()`、`is_run_loop_thread()` の
  整合性が崩れます。
- **Win32 の Window Class 名・CFRunLoop の RunLoopMode 名はユニークに保つ**:
  `irondash` や他ライブラリとの名前衝突を防ぐため、クレート固有のプレフィックスを維持してください。
