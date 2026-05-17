use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use wry::{
    WebViewBuilder,
    http::{Response, response::Builder},
};

use super::channel::ChannelResponseBody;

// Process-global handoff buffer for large channel payloads. It is global
// because the custom-protocol handler is a plain closure with no access to
// per-Channel state; the numeric id in the request header is what reconnects a
// fetch to its payload. Entries are removed on fetch (or WebView close), so
// this is a transient handoff, not a cache.
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

fn empty_response(builder: Builder) -> Response<Cow<'static, [u8]>> {
    builder
        .header("Content-Length", "0")
        .body(Cow::Borrowed(&[] as &[u8]))
        .unwrap()
}

fn channel_data_response(data: ChannelResponseBody) -> Response<Cow<'static, [u8]>> {
    let (content_type, body) = match data {
        ChannelResponseBody::Json(json) => ("application/json", json.into_bytes()),
        ChannelResponseBody::Raw(bytes) => ("application/octet-stream", bytes),
    };
    let content_length = body.len().to_string();

    Response::builder()
        .header("Content-Type", content_type)
        .header("Content-Length", content_length)
        .header("Access-Control-Allow-Origin", "*")
        .status(200)
        .body(Cow::Owned(body))
        .unwrap()
}

/// Registers the channel protocol
pub(crate) fn setup_channel_protocol(builder: WebViewBuilder) -> WebViewBuilder {
    builder.with_custom_protocol("wxp-channel".into(), move |_webview, request| {
        // WebView2 (Windows) sends a CORS preflight for this custom protocol;
        // without an explicit OPTIONS response the channel fetch is blocked.
        // Harmless elsewhere, so always answer it.
        if request.method() == "OPTIONS" {
            return empty_response(
                Response::builder()
                    .header("Access-Control-Allow-Origin", "*")
                    .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                    .header("Access-Control-Allow-Headers", "*")
                    .status(200),
            );
        }

        if request.uri().path() == "/fetch" {
            if let Some(id_header) = request
                .headers()
                .get(super::channel::CHANNEL_ID_HEADER_NAME)
            {
                if let Ok(id_str) = id_header.to_str() {
                    if let Ok(id) = id_str.parse::<u32>() {
                        if let Some(data) = fetch_channel_data(id) {
                            return channel_data_response(data);
                        }
                    }
                }
            }
        }

        empty_response(Response::builder().status(404))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_data_response_sets_raw_content_length() {
        let response = channel_data_response(ChannelResponseBody::Raw(vec![0, 1, 2, 255]));

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/octet-stream"
        );
        assert_eq!(response.headers().get("Content-Length").unwrap(), "4");
        assert_eq!(response.body().as_ref(), &[0, 1, 2, 255]);
    }

    #[test]
    fn channel_data_response_sets_json_content_length() {
        let response =
            channel_data_response(ChannelResponseBody::Json("{\"ok\":true}".to_string()));

        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(response.headers().get("Content-Length").unwrap(), "11");
        assert_eq!(response.body().as_ref(), b"{\"ok\":true}");
    }
}
