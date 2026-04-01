use send_wrapper::SendWrapper;
use std::cell::RefCell;
use std::sync::{Arc, Weak};
use wry::WebView;

/// WebView への参照を管理する構造体
///
/// [`WebViewRef`] は Send + Sync だが、 MainThread からしかアクセスしてはいけない。
/// Send + Sync にする理由は、オーディオプラグインのインスタンスのような一時的にオーディオスレッドに移動される構造体でもメンバ変数に保持できるようにするため。
///
/// 生存期間の管理:
/// [`WebViewRef`] は全てのインスタンスがドロップされると WebView も破棄され、
/// ウィンドウ内のコンテンツが表示されなくなります。
/// WebView を表示し続けるには、[`WebViewRef`] を最低一つどこかで保持し続ける必要があります。
///
#[derive(Clone)]
pub struct WebViewRef {
    inner: Arc<SendWrapper<RefCell<WebView>>>,
}

impl std::fmt::Debug for WebViewRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebViewRef").finish()
    }
}

impl WebViewRef {
    /// 新しい WebViewRef を作成
    pub(crate) fn new(webview: WebView) -> Self {
        Self {
            inner: Arc::new(SendWrapper::new(RefCell::new(webview))),
        }
    }

    /// JavaScript を評価
    pub fn evaluate_script(&self, script: &str) -> Result<(), wry::Error> {
        self.inner.borrow().evaluate_script(script)
    }

    /// WebView の境界を設定
    pub fn set_bounds(&self, bounds: wry::Rect) -> Result<(), wry::Error> {
        self.inner.borrow().set_bounds(bounds)
    }

    /// 弱参照を取得（内部使用）
    pub(crate) fn downgrade(&self) -> Weak<SendWrapper<RefCell<WebView>>> {
        Arc::downgrade(&self.inner)
    }
}
