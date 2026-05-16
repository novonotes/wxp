use serde::{Deserialize, Serialize};
use serde_json::Value;

// Wire format for the JS↔Rust `invoke` bridge. These structs must stay in sync
// with the message shape built by `INVOKE_INIT_SCRIPT` in `initialization.rs`:
// `callback`/`error` are JS callback ids that the response is routed back to,
// and `inner` carries the user-supplied arguments (flattened).

/// invoke request from the frontend
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InvokeRequest {
    pub(crate) cmd: String,
    pub(crate) callback: u32,
    pub(crate) error: u32,
    #[serde(default)]
    pub(crate) inner: InvokeBody,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct InvokeBody {
    #[serde(flatten)]
    pub(crate) args: Value,
}

/// Response to the frontend
#[derive(Debug, Serialize)]
pub(super) struct InvokeResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<Value>,
}

impl InvokeResponse {
    pub(super) fn success(value: Value) -> Self {
        Self {
            value: Some(value),
            error: None,
        }
    }

    pub(super) fn error<E: Serialize>(error: E) -> Self {
        Self {
            value: None,
            error: Some(
                serde_json::to_value(error)
                    .unwrap_or_else(|e| Value::String(format!("Failed to serialize error: {}", e))),
            ),
        }
    }
}
