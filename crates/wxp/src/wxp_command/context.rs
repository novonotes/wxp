use crate::webview_ref::WebViewRef;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Information required to deserialize command arguments
pub(crate) struct DeserializeContext<'a> {
    /// Command name
    pub(crate) name: &'a str,
    /// Key of the argument to deserialize
    pub(crate) key: &'a str,
    /// Argument value (JSON)
    pub(crate) args: &'a serde_json::Value,
    /// WebView reference
    pub(crate) webview: WebViewRef,
}

/// Trait for converting command arguments
pub(crate) trait TryFromDeserializeContext<'de>: Sized {
    /// Attempts to convert from a DeserializeContext into Self
    fn try_from(ctx: DeserializeContext<'de>) -> Result<Self, Value>;
}

/// Automatically implements TryFromDeserializeContext for Deserializable types
impl<'de, T: DeserializeOwned> TryFromDeserializeContext<'de> for T {
    fn try_from(cmd: DeserializeContext<'de>) -> Result<Self, Value> {
        let value = cmd.args.get(cmd.key).ok_or_else(|| {
            Value::String(format!(
                "Missing argument '{}' for command '{}'",
                cmd.key, cmd.name
            ))
        })?;

        serde_json::from_value(value.clone())
            .map_err(|e| Value::String(format!("Failed to deserialize {}: {}", cmd.key, e)))
    }
}

/// Command context — provides access to arguments
pub struct CommandContext<'a> {
    /// Command name
    pub(crate) name: &'a str,
    /// Argument value (JSON)
    pub(crate) args: &'a serde_json::Value,
    /// WebView reference
    pub(crate) webview: WebViewRef,
}

impl<'a> CommandContext<'a> {
    /// Creates a new CommandContext
    pub(crate) fn new(name: &'a str, args: &'a serde_json::Value, webview: WebViewRef) -> Self {
        Self {
            name,
            args,
            webview,
        }
    }

    /// Retrieves an argument with type safety using the specified key
    pub fn arg<T>(&self, key: &'a str) -> Result<T, Value>
    where
        T: TryFromDeserializeContext<'a>,
    {
        let ctx = DeserializeContext {
            name: self.name,
            key,
            args: self.args,
            webview: self.webview.clone(),
        };
        T::try_from(ctx)
    }

    /// Returns the full command arguments as JSON
    pub fn args_json(&self) -> Value {
        self.args.clone()
    }

    /// Returns a reference to the WebView
    pub fn webview(&self) -> WebViewRef {
        self.webview.clone()
    }
}
