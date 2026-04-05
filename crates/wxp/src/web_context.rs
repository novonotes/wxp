use std::path::PathBuf;

/// wxp 用の WebContext 設定
///
/// WebView のユーザーデータ（キャッシュ、Cookie、ローカルストレージなど）を
/// 保存するディレクトリを指定します。
/// プラグイン環境では書き込み権限の問題が発生するため、
/// data_directory の指定を必須としています。
///
/// # Example
///
/// ```no_run
/// use wxp::WebContext;
///
/// let data_dir = std::env::temp_dir().join("my-plugin");
/// let web_context = WebContext::new(data_dir);
/// ```
#[derive(Debug, Clone)]
pub struct WebContext {
    data_directory: PathBuf,
}

impl WebContext {
    /// 新しい WebContext を作成します。
    ///
    /// # Arguments
    ///
    /// * `data_directory` - WebView のユーザーデータを保存するディレクトリ（必須）
    pub fn new(data_directory: impl Into<PathBuf>) -> Self {
        Self {
            data_directory: data_directory.into(),
        }
    }

    /// データディレクトリのパスを取得します
    pub fn data_directory(&self) -> &PathBuf {
        &self.data_directory
    }

    /// この設定から wry::WebContext を作成します。
    ///
    /// 返した `wry::WebContext` は **WebView の生存期間中ずっと保持する必要があります**。
    /// [`WxpWebViewBuilder::new`](crate::WxpWebViewBuilder::new) に渡した後も
    /// drop されないよう、呼び出し元で変数を保持し続けてください。
    pub fn build_wry_context(&self) -> wry::WebContext {
        wry::WebContext::new(Some(self.data_directory.clone()))
    }
}
