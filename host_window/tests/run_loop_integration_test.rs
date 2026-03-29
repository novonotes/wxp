// カスタムテストハーネス - シンプルなウィンドウ作成テストから開始
use host_window::create_window;
use log::error;
use novonotes_run_loop::RunLoop;
use std::time::Duration;

fn main() {
    println!("Running wxp GUI tests on main thread...");

    // RunLoopを初期化
    RunLoop::init().unwrap();

    // テストを実行
    let mut failed = false;

    print!("Testing window creation... ");
    match std::panic::catch_unwind(|| test_simple_window()) {
        Ok(_) => println!("✓"),
        Err(e) => {
            println!("✗");
            error!("Error: {:?}", e);
            failed = true;
        }
    }

    // RunLoopをクリーンアップ
    RunLoop::deinit();

    if failed {
        error!("\nSome tests failed!");
        std::process::exit(1);
    } else {
        println!("\nAll tests passed!");
    }
}

fn test_simple_window() {
    // シンプルなウィンドウを作成
    let window_handle = create_window("Test Window", 400.0, 300.0);

    // ウィンドウを表示
    window_handle.show();

    // 少し待つ
    let mut handle = RunLoop::current().schedule(Duration::from_secs(1), move || {
        println!("Window test completed");
        RunLoop::current().stop_app();
    });
    handle.detach();

    // デスクトップ環境では run_app を使う
    RunLoop::current().run_app();

    // ウィンドウを破棄
    window_handle.destroy();
}
