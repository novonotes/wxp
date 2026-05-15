use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use wry::{WebViewBuilder, http::Response};

use super::channel::ChannelResponseBody;

static CHANNEL_DATA_STORE: Lazy<Arc<Mutex<HashMap<u32, ChannelResponseBody>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

pub(crate) fn store_channel_data_typed(id: u32, data: ChannelResponseBody) {
    // Large channel payloads are fetched by JS through the custom protocol because they should not
    // be embedded into evaluate_script source.
    CHANNEL_DATA_STORE.lock().insert(id, data);
}

pub(crate) fn fetch_channel_data(id: u32) -> Option<ChannelResponseBody> {
    // Fetch is one-shot: once JS receives the payload, keeping a second copy only leaks memory.
    CHANNEL_DATA_STORE.lock().remove(&id)
}

pub(crate) fn remove_channel_data(id: u32) {
    // Used when the WebView closes before the scheduled fetch can consume the payload.
    CHANNEL_DATA_STORE.lock().remove(&id);
}

/// Registers the channel protocol
pub(crate) fn setup_channel_protocol(builder: WebViewBuilder) -> WebViewBuilder {
    builder.with_custom_protocol("wxp-channel".into(), move |_webview, request| {
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

        if request.uri().path() == "/fetch" {
            if let Some(id_header) = request
                .headers()
                .get(super::channel::CHANNEL_ID_HEADER_NAME)
            {
                if let Ok(id_str) = id_header.to_str() {
                    if let Ok(id) = id_str.parse::<u32>() {
                        if let Some(data) = fetch_channel_data(id) {
                            let (content_type, body) = match data {
                                ChannelResponseBody::Json(json) => {
                                    ("application/json", json.into_bytes())
                                }
                                ChannelResponseBody::Raw(bytes) => {
                                    ("application/octet-stream", bytes)
                                }
                            };

                            return Response::builder()
                                .header("Content-Type", content_type)
                                .header("Access-Control-Allow-Origin", "*")
                                .status(200)
                                .body(body.into())
                                .unwrap();
                        }
                    }
                }
            }
        }

        Response::builder().status(404).body(vec![].into()).unwrap()
    })
}
