use super::error::{Error, Result};
use crate::WebViewDispatch;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc;

// Channel configuration constants
pub(crate) const IPC_PAYLOAD_PREFIX: &str = "__CHANNEL__:";
pub(crate) const CHANNEL_ID_HEADER_NAME: &str = "X-Channel-Data-Id";
const FETCH_CHANNEL_DATA_COMMAND: &str = "__wxp_channel_fetch_data__";

// Small messages go through evaluate_script to avoid an extra custom-protocol fetch. Larger
// messages use the fetch path because embedding big JSON strings or raw byte arrays into JS source
// makes the script expensive to allocate, escape, parse, and enqueue.
const MAX_JSON_DIRECT_EXECUTE_THRESHOLD: usize = 8192;
const MAX_RAW_DIRECT_EXECUTE_THRESHOLD: usize = 1024;

static CHANNEL_DATA_COUNTER: AtomicU32 = AtomicU32::new(1);

/// Possible values of a channel response body.
#[derive(Debug, Clone)]
pub(crate) enum ChannelResponseBody {
    /// JSON payload.
    Json(String),
    /// Raw bytes payload.
    Raw(Vec<u8>),
}

/// A channel for sending data from Rust to JavaScript.
///
/// Create a `new Channel(callback)` on the JavaScript side and pass it as an argument to `invoke()`.
/// On the Rust side, call [`send`](Self::send) / [`send_bytes`](Self::send_bytes) on the received
/// `Channel` to deliver data to the callback.
///
/// When `Channel` is dropped, the JavaScript side is notified that the channel has ended.
/// Use [`close`](Self::close) if you want to explicitly control when the channel ends.
#[derive(Debug, Clone)]
pub struct Channel {
    id: u32,
    webview: WebViewDispatch,
    inner: Arc<WxpChannelInner>,
}

#[derive(Debug)]
struct WxpChannelInner {
    current_index: AtomicU32,
    on_drop_tx: Option<mpsc::UnboundedSender<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WxpChannelMessage<T> {
    message: T,
    index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WxpChannelEnd {
    end: bool,
    index: u32,
}

impl Channel {
    pub(crate) fn new(id: u32, webview: WebViewDispatch) -> Self {
        Self {
            id,
            webview,
            inner: Arc::new(WxpChannelInner {
                current_index: AtomicU32::new(0),
                on_drop_tx: None,
            }),
        }
    }

    /// Returns this channel's ID, matching the JavaScript-side `Channel` object ID.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Sends JSON-serializable data to the JavaScript callback.
    ///
    /// On the JS side, the deserialized object is passed as the callback argument of `Channel`.
    pub fn send<T: Serialize>(&self, data: T) -> Result<()> {
        let current_index = self.inner.current_index.fetch_add(1, Ordering::SeqCst);

        let message = WxpChannelMessage {
            message: data,
            index: current_index,
        };

        let json_string = serde_json::to_string(&message)?;

        // Small JSON can be delivered inline without paying the custom-protocol fetch round trip.
        if json_string.len() < MAX_JSON_DIRECT_EXECUTE_THRESHOLD {
            self.execute_callback(&json_string)
        } else {
            // Large messages avoid embedding the callback envelope into JS source twice.
            let data_json = serde_json::to_string(&message.message)?;
            self.send_large_data(ChannelResponseBody::Json(data_json), current_index)
        }
    }

    /// Sends binary data as an `ArrayBuffer` to the JavaScript callback.
    ///
    /// On the JS side, use `message instanceof ArrayBuffer` to identify it.
    pub fn send_bytes(&self, data: Vec<u8>) -> Result<()> {
        let current_index = self.inner.current_index.fetch_add(1, Ordering::SeqCst);

        // Small byte buffers are cheaper to inline than to route through the global fetch store.
        if data.len() < MAX_RAW_DIRECT_EXECUTE_THRESHOLD {
            let bytes_as_json_array = serde_json::to_string(&data)?;
            let js = format!(
                "window.__WXP_INTERNALS__.runCallback({}, {{ message: new Uint8Array({}).buffer, index: {} }})",
                self.id, bytes_as_json_array, current_index
            );
            self.webview.post_eval_script(js)?;
            Ok(())
        } else {
            self.send_large_data(ChannelResponseBody::Raw(data), current_index)
        }
    }

    fn send_large_data(&self, body: ChannelResponseBody, current_index: u32) -> Result<()> {
        let data_id = CHANNEL_DATA_COUNTER.fetch_add(1, Ordering::SeqCst);

        super::internals::store_channel_data_typed(data_id, body);

        let js = format!(
            r#"window.__WXP_INTERNALS__.fetchChannelData('{}', {{'{}': '{}'}})
                .then((response) => window.__WXP_INTERNALS__.runCallback({}, {{ message: response, index: {} }}))
                .catch(console.error)"#,
            FETCH_CHANNEL_DATA_COMMAND, CHANNEL_ID_HEADER_NAME, data_id, self.id, current_index
        );
        self.webview.post_eval_script_or_else(js, move || {
            // Large payloads live in a global store until JS fetches them. If the WebView closes
            // before the fetch can be scheduled or run, there is no receiver left to consume it.
            super::internals::remove_channel_data(data_id);
        })?;
        Ok(())
    }

    /// Explicitly closes the channel.
    ///
    /// Sends an end notification to the JS callback and consumes the channel.
    /// The end notification is also sent automatically when `Channel` is dropped,
    /// so only use this when you need to explicitly control when the channel ends.
    pub fn close(self) -> Result<()> {
        self.send_end_message()
    }

    fn execute_callback(&self, json_data: &str) -> Result<()> {
        let js = format!(
            "window.__WXP_INTERNALS__.runCallback({}, {})",
            self.id, json_data
        );
        self.webview.post_eval_script(js)?;
        Ok(())
    }

    fn send_end_message(&self) -> Result<()> {
        let current_index = self.inner.current_index.load(Ordering::SeqCst);

        let end_message = WxpChannelEnd {
            end: true,
            index: current_index,
        };

        self.execute_callback(&serde_json::to_string(&end_message)?)
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        let _ = self.send_end_message();

        if let Some(ref tx) = self.inner.on_drop_tx {
            let _ = tx.send(self.id);
        }
    }
}

pub(crate) fn parse_channel_id(value: &str) -> Result<u32> {
    value
        .strip_prefix(IPC_PAYLOAD_PREFIX)
        .ok_or_else(|| Error::InvalidChannelId(value.to_string()))?
        .parse::<u32>()
        .map_err(|_| Error::InvalidChannelId(value.to_string()))
}
