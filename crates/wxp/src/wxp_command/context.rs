use crate::webview_ref::WebViewRef;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// コマンドの引数のデシリアライズに必要な情報
pub struct DeserializeContext<'a> {
    /// コマンド名
    pub(crate) name: &'a str,
    /// Deserialize する引数のキー
    pub(crate) key: &'a str,
    /// 引数の値（JSON）
    pub(crate) args: &'a serde_json::Value,
    /// WebView参照
    pub(crate) webview: WebViewRef,
}

/// コマンド引数の変換を行うためのトレイト
pub trait TryFromDeserializeContext<'de>: Sized {
    /// DeserializeContext から Self への変換を試みる
    fn try_from(ctx: DeserializeContext<'de>) -> Result<Self, Value>;
}

/// Deserialize 可能な型に対して TryFromDeserializeContext を自動実装
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

/// コマンドコンテキスト - 引数へのアクセスを提供
pub struct CommandContext<'a> {
    /// コマンド名
    pub(crate) name: &'a str,
    /// 引数の値（JSON）
    pub(crate) args: &'a serde_json::Value,
    /// WebView参照
    pub(crate) webview: WebViewRef,
}

impl<'a> CommandContext<'a> {
    /// 新しい CommandContext を作成
    pub(crate) fn new(name: &'a str, args: &'a serde_json::Value, webview: WebViewRef) -> Self {
        Self {
            name,
            args,
            webview,
        }
    }

    /// 指定されたセレクターで引数を型安全に取得
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

    /// コマンド引数全体を JSON として取得
    pub fn args_json(&self) -> Value {
        self.args.clone()
    }

    /// WebView への参照を取得
    pub fn webview(&self) -> WebViewRef {
        self.webview.clone()
    }
}
