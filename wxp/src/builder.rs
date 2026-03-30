use crate::initialization::get_initialization_scripts;
use crate::webview_ref::WebViewRef;
use crate::wxp_channel::internals::setup_channel_protocol;
use crate::wxp_command::{WxpCommandHandler, setup::setup_invoke_handler_internal};
use crate::wxp_webview::error::{Error, Result};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use wry::raw_window_handle;
use wry::{WebViewBuilder, http::Response};
use zip::ZipArchive;
use zip::result::ZipError;

pub struct WxpWebViewBuilder<'a> {
    builder: WebViewBuilder<'a>,
    command_handler: Option<Arc<WxpCommandHandler>>,
}

impl<'a> WxpWebViewBuilder<'a> {
    /// 新しいWebViewBuilderを作成（チャンネル機能は常に有効）
    ///
    /// # Arguments
    ///
    /// * `web_context` - wry の WebContext への可変参照
    ///                   wxp::WebContext::build_wry_context() で作成してください
    ///                   プラグイン環境では通常 `<system temp>/<plugin名>` を使用
    pub fn new(web_context: &'a mut wry::WebContext) -> Self {
        let builder = WebViewBuilder::new_with_web_context(web_context);
        let builder = setup_channel_protocol(builder);

        Self {
            builder,
            command_handler: None,
        }
    }

    pub fn with_serve_dir(
        mut self,
        protocol: impl Into<String>,
        base_path: impl Into<PathBuf>,
    ) -> Result<Self> {
        let base_path = base_path.into();
        if !base_path.exists() {
            return Err(Error::PathNotFound(base_path.display().to_string()));
        }

        let protocol = protocol.into();
        self.builder = self
            .builder
            .with_custom_protocol(protocol, move |_webview, request| {
                // Handle OPTIONS request for CORS preflight (required for Windows)
                if request.method() == "OPTIONS" {
                    return Response::builder()
                        .header("Access-Control-Allow-Origin", "*")
                        .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                        .header("Access-Control-Allow-Headers", "*")
                        .status(200)
                        .body(vec![].into())
                        .unwrap();
                }

                let uri = request.uri();
                let path = uri.path();

                let mut file_path = if path == "/" || path.is_empty() {
                    base_path.join("index.html")
                } else {
                    base_path.join(path.strip_prefix('/').unwrap_or(path))
                };

                if !file_path.exists() && !path.contains('.') {
                    file_path = base_path.join("index.html");
                }

                match std::fs::read(&file_path) {
                    Ok(content) => {
                        let mime_type = mime_guess::from_path(&file_path)
                            .first_or_octet_stream()
                            .to_string();

                        Response::builder()
                            .header("Content-Type", mime_type)
                            .header("Access-Control-Allow-Origin", "*")
                            .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                            .header("Access-Control-Allow-Headers", "*")
                            .body(content.into())
                            .unwrap()
                    }
                    Err(_) => Response::builder()
                        .status(404)
                        .header("Content-Type", "text/plain")
                        .header("Access-Control-Allow-Origin", "*")
                        .body("Not Found".as_bytes().to_vec().into())
                        .unwrap(),
                }
            });

        Ok(self)
    }

    pub fn with_serve_zip(
        mut self,
        protocol: impl Into<String>,
        zip_bytes: &'static [u8],
    ) -> Result<Self> {
        let cursor = Cursor::new(zip_bytes);
        let archive = ZipArchive::new(cursor).map_err(|err| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                err.to_string(),
            ))
        })?;
        let mut filepath_to_index = HashMap::<String, usize>::new();
        archive.file_names().enumerate().for_each(|(i, name)| {
            filepath_to_index.insert(name.to_string(), i);
        });

        let archive = Arc::new(Mutex::new(archive));

        let protocol = protocol.into();
        self.builder = self
            .builder
            // zipファイルのサイズが大きい場合、リクエスト毎の読み取り・展開に時間がかかる可能性がある
            // 大きなファイルを使用するユースケースをサポートする場合はwith_asynchronous_custom_protocol等の使用を検討する
            .with_custom_protocol(protocol, move |_webview, request| {
                // Handle OPTIONS request for CORS preflight (required for Windows)
                if request.method() == "OPTIONS" {
                    return Response::builder()
                        .header("Access-Control-Allow-Origin", "*")
                        .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                        .header("Access-Control-Allow-Headers", "*")
                        .status(200)
                        .body(vec![].into())
                        .unwrap();
                }

                let uri = request.uri();
                let path = uri.path();
                let file_path = if path == "/" || path.is_empty() {
                    PathBuf::from("index.html")
                } else {
                    PathBuf::from(path.trim_start_matches('/'))
                };

                let Ok(mut archive) = archive.lock() else {
                    return Response::builder()
                        .status(500)
                        .header("Content-Type", "text/plain")
                        .header("Access-Control-Allow-Origin", "*")
                        .body(
                            "Internal Server Error: Failed to lock ZIP archive"
                                .as_bytes()
                                .to_vec()
                                .into(),
                        )
                        .unwrap();
                };

                let file_path_cow = file_path.to_string_lossy();
                let entry_indx = filepath_to_index.get(file_path_cow.as_ref()).or_else(|| {
                    // windowsで作成されたzipファイルの中には、パス区切りがバックスラッシュになっているものがあるため、両方のパターンで検索する
                    let path_with_backslashes = file_path_cow.replace('/', "\\");
                    filepath_to_index.get(path_with_backslashes.as_str())
                });

                let Some(entry_index) = entry_indx else {
                    return Response::builder()
                        .status(404)
                        .header("Content-Type", "text/plain")
                        .header("Access-Control-Allow-Origin", "*")
                        .body("Not Found".as_bytes().to_vec().into())
                        .unwrap();
                };

                match archive.by_index(*entry_index) {
                    Ok(mut zip_file) => {
                        let mut body: Vec<u8> = Vec::with_capacity(zip_file.size() as usize);
                        // ここでzipファイル内の読み取りと展開が行われる
                        let Ok(_) = zip_file.read_to_end(&mut body) else {
                            return Response::builder()
                                .status(500)
                                .header("Content-Type", "text/plain")
                                .header("Access-Control-Allow-Origin", "*")
                                .body(
                                    "Internal Server Error: Failed to read ZIP file"
                                        .as_bytes()
                                        .to_vec()
                                        .into(),
                                )
                                .unwrap();
                        };

                        let mime_type = mime_guess::from_path(&file_path)
                            .first_or_octet_stream()
                            .to_string();

                        Response::builder()
                            .header("Content-Type", mime_type)
                            .header("Access-Control-Allow-Origin", "*")
                            .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                            .header("Access-Control-Allow-Headers", "*")
                            .body(body.into())
                            .unwrap()
                    }
                    Err(ZipError::FileNotFound) => Response::builder()
                        .status(404)
                        .header("Content-Type", "text/plain")
                        .header("Access-Control-Allow-Origin", "*")
                        .body("Not Found".as_bytes().to_vec().into())
                        .unwrap(),
                    Err(err) => Response::builder()
                        .status(500)
                        .header("Content-Type", "text/plain")
                        .header("Access-Control-Allow-Origin", "*")
                        .body(
                            format!("Internal Server Error: {}", err)
                                .as_bytes()
                                .to_vec()
                                .into(),
                        )
                        .unwrap(),
                }
            });

        Ok(self)
    }

    pub fn with_command_handler(mut self, handler: Arc<WxpCommandHandler>) -> Self {
        let builder = setup_invoke_handler_internal(self.builder, handler.clone());
        self.command_handler = Some(handler);
        Self {
            builder,
            command_handler: self.command_handler,
        }
    }

    pub fn with_url(self, url: impl Into<String>) -> Self {
        Self {
            builder: self.builder.with_url(&url.into()),
            command_handler: self.command_handler,
        }
    }

    pub fn with_html(self, html: impl Into<String>) -> Self {
        Self {
            builder: self.builder.with_html(&html.into()),
            command_handler: self.command_handler,
        }
    }

    pub fn with_devtools(self, devtools: bool) -> Self {
        Self {
            builder: self.builder.with_devtools(devtools),
            command_handler: self.command_handler,
        }
    }

    pub fn with_visible(self, visible: bool) -> Self {
        Self {
            builder: self.builder.with_visible(visible),
            command_handler: self.command_handler,
        }
    }

    pub fn with_bounds(self, bounds: wry::Rect) -> Self {
        Self {
            builder: self.builder.with_bounds(bounds),
            command_handler: self.command_handler,
        }
    }

    /// WebView を子ウィンドウとして構築
    ///
    /// # 生存期間の管理
    ///
    /// 返される `WebViewRef` を保持し続ける必要があります。
    /// ドロップされると WebView の表示が消えます。
    pub fn build_as_child<W>(self, window: &W) -> Result<WebViewRef>
    where
        W: raw_window_handle::HasWindowHandle,
    {
        // 初期化スクリプトを適用
        let has_command_handler = self.command_handler.is_some();
        let initialization_script = get_initialization_scripts(has_command_handler);
        let builder = self
            .builder
            .with_initialization_script(&initialization_script);

        let webview = builder.build_as_child(window)?;
        let webview_ref = WebViewRef::new(webview);

        // コマンドハンドラーにWebViewを設定
        if let Some(handler) = self.command_handler {
            handler.set_webview(webview_ref.clone());
        }

        Ok(webview_ref)
    }
}
