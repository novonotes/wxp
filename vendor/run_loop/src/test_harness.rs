// カスタムテストハーネス - GUI統合テスト用のヘルパー
use crate::RunLoop;
use log::{error, info};

/// 複数のGUIテストを順次実行するハーネス
/// GUI 関連の操作には、必ず main スレッドで実行しなければいけない処理があります。
/// そのような処理のテストは、標準のテストハーネスでは難しいので。これを使ってください。
///
/// # Example
/// ```ignore
/// run_gui_tests(vec![
///     ("test1", test_function1),
///     ("test2", test_function2),
/// ]);
/// ```
/// このハーネスを利用する場合、標準ハーネスを無効化する必要があります。
///
/// Cargo.toml の例
/// ```ignore
/// [[test]]
/// name = "wxp_webview_test"
/// path = "tests/wxp_webview_test.rs"
/// harness = false
/// ```
///
pub fn run_gui_tests<F>(tests: Vec<(&str, F)>)
where
    F: FnOnce() -> Result<(), String>,
{
    info!("Running GUI tests on main thread...");

    // RunLoopを初期化
    match RunLoop::init() {
        Ok(_) => {}
        Err(e) => {
            error!("Failed to initialize RunLoop: {:?}", e);
            std::process::exit(1);
        }
    }

    let mut failed = false;

    for (name, test_fn) in tests {
        print!("Testing {}... ", name);
        match test_fn() {
            Ok(_) => println!("✓"),
            Err(e) => {
                println!("✗");
                error!("Error: {}", e);
                failed = true;
            }
        }
    }

    // RunLoopをクリーンアップ
    RunLoop::deinit();

    if failed {
        error!("Some tests failed!");
        std::process::exit(1);
    } else {
        info!("All tests passed!");
    }
}
