use crate::WebViewDispatch;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Information required to deserialize command arguments
#[doc(hidden)]
pub struct DeserializeContext<'a> {
    /// Command name
    pub(crate) name: &'a str,
    /// Key of the argument to deserialize
    pub(crate) key: &'a str,
    /// Argument value (JSON)
    pub(crate) args: &'a serde_json::Value,
    /// Dispatch handle for the WebView that invoked this command.
    pub(crate) webview: WebViewDispatch,
}

/// Extension point for extracting a typed command argument.
///
/// The blanket impl below covers ordinary `DeserializeOwned` types (plain JSON
/// deserialization). Types that need more than the JSON value — notably
/// [`Channel`](crate::Channel), which must also capture the WebView dispatch —
/// supply their own impl, which is why this receives the whole
/// `DeserializeContext` rather than just a `Value`.
#[doc(hidden)]
pub trait TryFromDeserializeContext<'de>: Sized {
    fn try_from(ctx: DeserializeContext<'de>) -> Result<Self, Value>;
}

/// Default path: any `DeserializeOwned` type is decoded straight from its JSON
/// argument. Custom impls (e.g. `Channel`) opt out by not being `Deserialize`.
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

    /// Returns the WebView dispatch handle for the WebView that invoked this command.
    pub fn webview(&self) -> &WebViewDispatch {
        // Expose dispatch rather than the owner so commands can post UI work without participating
        // in native WebView lifetime management.
        &self.webview
    }
}
