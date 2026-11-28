use crate::{Channel, WebViewDispatch, wxp_channel::channel::parse_channel_id};
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Command context — provides access to arguments
pub struct CommandContext<'a> {
    /// Command name
    pub(crate) name: &'a str,
    /// Argument value (JSON)
    pub(crate) args: &'a serde_json::Value,
    /// WebView dispatch handle
    pub(crate) webview: WebViewDispatch,
}

impl<'a> CommandContext<'a> {
    /// Creates a new CommandContext
    pub(crate) fn new(
        name: &'a str,
        args: &'a serde_json::Value,
        webview: WebViewDispatch,
    ) -> Self {
        Self {
            name,
            args,
            webview,
        }
    }

    /// Retrieves an argument with type safety using the specified key
    pub fn arg<T>(&self, key: &'a str) -> Result<T, Value>
    where
        T: DeserializeOwned,
    {
        let value = self.args.get(key).ok_or_else(|| {
            Value::String(format!(
                "Missing argument '{}' for command '{}'",
                key, self.name
            ))
        })?;

        serde_json::from_value(value.clone())
            .map_err(|e| Value::String(format!("Failed to deserialize {key}: {e}")))
    }

    /// Retrieves a JavaScript-created [`Channel`] argument.
    ///
    /// Channels are explicit because they need the invoking WebView dispatch in
    /// addition to the JSON channel id. Ordinary JSON arguments should use
    /// [`arg`](Self::arg).
    pub fn channel(&self, key: &'a str) -> Result<Channel, Value> {
        let value: String = self.arg(key)?;
        let id = parse_channel_id(&value)
            .map_err(|error| Value::String(format!("Failed to deserialize {key}: {error}")))?;
        Ok(Channel::new(id, self.webview.clone()))
    }

    /// Returns the full command arguments as JSON
    pub fn args_json(&self) -> Value {
        self.args.clone()
    }

    /// Returns the WebView dispatch handle for the WebView that invoked this command.
    pub fn webview(&self) -> &WebViewDispatch {
        // Expose dispatch rather than the owner so commands can post UI work without participating
        // in native WebView lifetime management.
        &self.webview
    }
}
