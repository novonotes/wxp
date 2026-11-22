use crate::initialization::get_initialization_scripts;
use crate::keyboard::{
    AcceleratorDestination, KeyEventDestination, KeyboardRouting, WebViewAcceleratorDelivery,
    apply_keyboard_routing,
};
use crate::page_load::PageLoadMonitor;
use crate::web_context::WebContext;
use crate::webview::WxpWebView;
use crate::wxp_channel::internals::setup_channel_protocol;
use crate::wxp_command::{WxpCommandHandler, setup::setup_invoke_handler_internal};
use crate::wxp_webview::error::{Error, Result};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
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
    keyboard_routing: KeyboardRouting,
}

impl<'a> WxpWebViewBuilder<'a> {
    /// Creates a new WebViewBuilder (channel feature is always enabled)
    ///
    /// # Arguments
    ///
    /// * `web_context` - Mutable reference to a wxp WebContext. In a plugin environment, typically
    ///   use `<system temp>/<plugin name>`.
    pub fn new(web_context: &'a mut WebContext) -> Self {
        // In plugin UIs, the first click on an inactive editor should still reach the WebView
        // so controls like knobs can start dragging immediately after window activation.
        let builder = WebViewBuilder::new_with_web_context(web_context.wry_context_mut())
            .with_accept_first_mouse(true);
        let builder = setup_channel_protocol(builder);

        Self {
            builder,
            command_handler: None,
            keyboard_routing: KeyboardRouting::new(
                KeyEventDestination::WebView,
                AcceleratorDestination::WebView(WebViewAcceleratorDelivery::PlatformDefault),
            ),
        }
    }

    /// Sets native keyboard routing policy for keys that should bypass the WebView.
    ///
    /// Plugin hosts commonly reserve a few global shortcuts while the WebView still needs normal
    /// key input. Keeping this policy in native code lets the original key event reach the host.
    pub fn with_keyboard_routing(mut self, routing: KeyboardRouting) -> Self {
        self.keyboard_routing = routing;
        self
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
                // WebView2 (Windows) issues a CORS preflight for custom-protocol
                // requests; without an explicit OPTIONS response the real
                // request is blocked and assets fail to load. Other platforms
                // tolerate this too, so answer it unconditionally.
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

                // SPA fallback: a missing, extension-less path is a client-side
                // route (e.g. `/settings`), not a real file, so serve the app
                // shell and let the front-end router handle it.
                if !file_path.exists() && !path.contains('.') {
                    file_path = base_path.join("index.html");
                }

                match std::fs::read(&file_path) {
                    Ok(content) => Response::builder()
                        .header("Content-Type", asset_content_type(&file_path))
                        .header("Access-Control-Allow-Origin", "*")
                        .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                        .header("Access-Control-Allow-Headers", "*")
                        .body(content.into())
                        .unwrap(),
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
        let archive = ZipArchive::new(cursor)
            .map_err(|err| Error::Io(std::io::Error::other(err.to_string())))?;
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
                // WebView2 (Windows) issues a CORS preflight for custom-protocol
                // requests; without an explicit OPTIONS response the real
                // request is blocked and assets fail to load. Other platforms
                // tolerate this too, so answer it unconditionally.
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

                        Response::builder()
                            .header("Content-Type", asset_content_type(&file_path))
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
                            format!("Internal Server Error: {err}")
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
            keyboard_routing: self.keyboard_routing,
        }
    }

    /// Sets the URL for the WebView to load.
    ///
    /// In debug builds, specify the Vite dev server (e.g. `http://localhost:5173/`).
    /// In release builds, use this in combination with [`with_serve_zip`](Self::with_serve_zip)
    /// and specify a custom protocol URL (e.g. `my-plugin://localhost/`).
    pub fn with_url(self, url: impl Into<String>) -> Self {
        Self {
            builder: self.builder.with_url(url.into()),
            command_handler: self.command_handler,
            keyboard_routing: self.keyboard_routing,
        }
    }

    /// Directly sets the HTML to be displayed in the WebView.
    ///
    /// Use this when passing an HTML string directly instead of specifying a URL.
    /// Use [`with_url`](Self::with_url) to load from a URL.
    pub fn with_html(self, html: impl Into<String>) -> Self {
        Self {
            builder: self.builder.with_html(html.into()),
            command_handler: self.command_handler,
            keyboard_routing: self.keyboard_routing,
        }
    }

    /// Enables or disables the browser DevTools.
    ///
    /// In debug builds, setting this to `true` allows opening DevTools via right-click → "Inspect".
    pub fn with_devtools(self, devtools: bool) -> Self {
        Self {
            builder: self.builder.with_devtools(devtools),
            command_handler: self.command_handler,
            keyboard_routing: self.keyboard_routing,
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
            keyboard_routing: self.keyboard_routing,
        }
    }

    /// Sets the initial size and position of the WebView.
    ///
    /// Host or wrapper integrations should convert their native GUI size into a DPI-aware
    /// [`Rect`](crate::Rect) before passing it here.
    pub fn with_bounds(self, bounds: crate::Rect) -> Self {
        Self {
            builder: self.builder.with_bounds(bounds),
            command_handler: self.command_handler,
            keyboard_routing: self.keyboard_routing,
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
        let page_load = PageLoadMonitor::new();
        let builder = self
            .builder
            .with_initialization_script(&initialization_script)
            .with_on_page_load_handler({
                let page_load = page_load.clone();
                move |event, _url| page_load.record(event)
            });

        let webview = builder.build_as_child(window)?;
        let webview = WxpWebView::new(webview, page_load)?;
        apply_keyboard_routing(&webview, self.keyboard_routing)?;
        let dispatch = webview.dispatch();

        if let Some(handler) = self.command_handler {
            // The handler gets a non-owning dispatch handle so command routing follows this WebView
            // without making the handler a hidden lifetime owner.
            handler.set_webview(dispatch);
        }

        Ok(webview)
    }
}

/// Returns the `Content-Type` for a served GUI asset, declaring UTF-8 for text formats.
///
/// Older WebKit (Safari 16 and earlier) decodes a script served without a `charset`
/// using the document encoding, even for module scripts. When the page does not declare
/// one either, that falls back to the user's locale encoding (e.g. Shift_JIS), so any
/// non-ASCII text in a UTF-8 bundle can turn into a syntax error and the whole GUI fails
/// to start. Declaring the charset on the response makes decoding independent of the page
/// and the user's locale. Assets are assumed to be UTF-8, which is what web bundlers emit.
fn asset_content_type(path: &Path) -> String {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let is_text = mime.type_() == mime_guess::mime::TEXT
        || matches!(mime.subtype().as_str(), "javascript" | "json" | "xml")
        || matches!(
            mime.suffix().map(|suffix| suffix.as_str()),
            Some("json" | "xml")
        );
    if is_text {
        format!("{}; charset=utf-8", mime.essence_str())
    } else {
        mime.essence_str().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::asset_content_type;
    use std::path::Path;

    #[test]
    fn text_assets_declare_utf8() {
        for (file, expected) in [
            ("index.html", "text/html; charset=utf-8"),
            ("assets/index.js", "text/javascript; charset=utf-8"),
            ("assets/index.css", "text/css; charset=utf-8"),
            ("data.json", "application/json; charset=utf-8"),
            ("icon.svg", "image/svg+xml; charset=utf-8"),
        ] {
            assert_eq!(asset_content_type(Path::new(file)), expected, "{file}");
        }
    }

    #[test]
    fn binary_assets_have_no_charset() {
        for (file, expected) in [
            ("image.png", "image/png"),
            ("font.woff2", "font/woff2"),
            ("module.wasm", "application/wasm"),
            ("unknown.bin", "application/octet-stream"),
        ] {
            assert_eq!(asset_content_type(Path::new(file)), expected, "{file}");
        }
    }
}
