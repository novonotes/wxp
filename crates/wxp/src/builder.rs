use crate::initialization::get_initialization_scripts;
use crate::web_context::WebContext;
use crate::webview::WxpWebView;
use crate::wxp_channel::internals::setup_channel_protocol;
use crate::wxp_command::{WxpCommandHandler, setup::setup_invoke_handler_internal};
use crate::wxp_webview::error::{Error, Result};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use wry::{WebViewBuilder, http::Response};
use zip::ZipArchive;
use zip::result::ZipError;

/// Builder for constructing a WebView instance.
///
/// Used in combination with [`WxpCommandHandler`] to configure bidirectional JavaScript ↔ Rust
/// communication. The Channel API (push notifications from Rust → JS) is always enabled.
///
/// [`build_as_child`](Self::build_as_child) must be called on the **main thread**.
///
/// ## Asset Serving Options
///
/// | Method | Use case |
/// |---|---|
/// | [`with_url`](Self::with_url) | Point directly to a Vite dev server during development |
/// | [`with_serve_dir`](Self::with_serve_dir) | Serve pre-built assets from the filesystem |
/// | [`with_serve_zip`](Self::with_serve_zip) | Serve a ZIP embedded via `include_bytes!` (for release builds) |
///
/// The typical release pattern is to combine `with_serve_zip("my-plugin", BYTES)` with
/// `with_url("my-plugin://localhost/")`.
///
/// # Basic Usage
///
/// ```no_run
/// use wxp::{WxpWebViewBuilder, WxpCommandHandler, WebContext};
/// use std::rc::Rc;
/// # fn example(window: &impl wxp::raw_window_handle::HasWindowHandle) -> Result<(), Box<dyn std::error::Error>> {
///
/// let mut web_context = WebContext::new(std::env::temp_dir().join("my-plugin"));
/// let handler = Rc::new(WxpCommandHandler::new());
/// let webview = WxpWebViewBuilder::new(&mut web_context)
///     .with_command_handler(handler)
///     .with_url("http://localhost:5173/")
///     .build_as_child(&window)?;
/// # Ok(())
/// # }
/// ```
pub struct WxpWebViewBuilder<'a> {
    builder: WebViewBuilder<'a>,
    command_handler: Option<Rc<WxpCommandHandler>>,
}

impl<'a> WxpWebViewBuilder<'a> {
    /// Creates a new WebViewBuilder (channel feature is always enabled)
    ///
    /// # Arguments
    ///
    /// * `web_context` - Mutable reference to a wxp WebContext.
    ///                   In a plugin environment, typically use `<system temp>/<plugin name>`.
    pub fn new(web_context: &'a mut WebContext) -> Self {
        // In plugin UIs, the first click on an inactive editor should still reach the WebView
        // so controls like knobs can start dragging immediately after window activation.
        let builder = WebViewBuilder::new_with_web_context(web_context.wry_context_mut())
            .with_accept_first_mouse(true);
        let builder = setup_channel_protocol(builder);

        Self {
            builder,
            command_handler: None,
        }
    }

    /// Serves the contents of a directory via a custom protocol.
    ///
    /// Primarily used to serve pre-built assets from the local filesystem.
    /// If `protocol` is set to `"my-plugin"`, the assets are accessible at `my-plugin://localhost/`.
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

    /// Serves the contents of a ZIP byte array via a custom protocol.
    ///
    /// Used to serve GUI assets embedded in the binary in release builds.
    /// If `protocol` is set to `"my-plugin"`, the assets are accessible at `my-plugin://localhost/`.
    ///
    /// `zip_bytes` must be `'static`. Pass a byte array from `include_bytes!` or generated by `build.rs`.
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
            // If the ZIP file is large, reading and extracting per request may take time.
            // Consider using with_asynchronous_custom_protocol if supporting large file use cases.
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
                    // Some ZIP files created on Windows use backslashes as path separators,
                    // so search with both patterns.
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
                        // Reading and extracting the file from the ZIP archive happens here
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

    /// Sets the command handler.
    ///
    /// Used to register commands that can be called via `invoke()` from JavaScript.
    /// Register commands in advance with `WxpCommandHandler::register_sync` / `register_async`,
    /// then pass the handler to the builder with this method.
    pub fn with_command_handler(mut self, handler: Rc<WxpCommandHandler>) -> Self {
        let builder = setup_invoke_handler_internal(self.builder, handler.clone());
        self.command_handler = Some(handler);
        Self {
            builder,
            command_handler: self.command_handler,
        }
    }

    /// Sets the URL for the WebView to load.
    ///
    /// In debug builds, specify the Vite dev server (e.g. `http://localhost:5173/`).
    /// In release builds, use this in combination with [`with_serve_zip`](Self::with_serve_zip)
    /// and specify a custom protocol URL (e.g. `my-plugin://localhost/`).
    pub fn with_url(self, url: impl Into<String>) -> Self {
        Self {
            builder: self.builder.with_url(&url.into()),
            command_handler: self.command_handler,
        }
    }

    /// Directly sets the HTML to be displayed in the WebView.
    ///
    /// Use this when passing an HTML string directly instead of specifying a URL.
    /// Use [`with_url`](Self::with_url) to load from a URL.
    pub fn with_html(self, html: impl Into<String>) -> Self {
        Self {
            builder: self.builder.with_html(&html.into()),
            command_handler: self.command_handler,
        }
    }

    /// Enables or disables the browser DevTools.
    ///
    /// In debug builds, setting this to `true` allows opening DevTools via right-click → "Inspect".
    pub fn with_devtools(self, devtools: bool) -> Self {
        Self {
            builder: self.builder.with_devtools(devtools),
            command_handler: self.command_handler,
        }
    }

    /// Sets the initial visibility of the WebView.
    ///
    /// Specifying `false` creates the WebView in a hidden state. To show it later,
    /// operate via [`WebViewDispatch`](crate::WebViewDispatch).
    pub fn with_visible(self, visible: bool) -> Self {
        Self {
            builder: self.builder.with_visible(visible),
            command_handler: self.command_handler,
        }
    }

    /// Sets the initial size and position of the WebView.
    ///
    /// For CLAP plugins, pass the GUI size notified by the host.
    /// Use [`wxp_clack::dpi::DpiConverter::create_webview_bounds`] to obtain a DPI-aware Rect.
    pub fn with_bounds(self, bounds: crate::Rect) -> Self {
        Self {
            builder: self.builder.with_bounds(bounds),
            command_handler: self.command_handler,
        }
    }

    /// Builds the WebView as a child window
    ///
    /// # Lifetime Management
    ///
    /// The returned `WxpWebView` must be kept alive.
    /// When it is dropped, the WebView content disappears.
    pub fn build_as_child<W>(self, window: &W) -> Result<WxpWebView>
    where
        W: crate::raw_window_handle::HasWindowHandle,
    {
        // Apply initialization scripts
        let has_command_handler = self.command_handler.is_some();
        let initialization_script = get_initialization_scripts(has_command_handler);
        let builder = self
            .builder
            .with_initialization_script(&initialization_script);

        let webview = builder.build_as_child(window)?;
        let webview = WxpWebView::new(webview)?;
        let dispatch = webview.dispatch();

        if let Some(handler) = self.command_handler {
            // The handler gets a non-owning dispatch handle so command routing follows this WebView
            // without making the handler a hidden lifetime owner.
            handler.set_webview(dispatch);
        }

        Ok(webview)
    }
}
