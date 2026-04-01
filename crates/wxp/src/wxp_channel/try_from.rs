use crate::{
    wxp_channel::{
        Channel,
        channel::{IPC_PAYLOAD_PREFIX, parse_channel_id},
    },
    wxp_command::context::{DeserializeContext, TryFromDeserializeContext},
};
use serde_json::Value;

/// WxpChannel の WxpTryFrom 実装
impl<'de> TryFromDeserializeContext<'de> for Channel {
    fn try_from(cmd: DeserializeContext<'de>) -> Result<Self, Value> {
        // チャンネルIDを取得
        let value = cmd
            .args
            .get(&cmd.key)
            .ok_or_else(|| Value::String(format!("Missing channel argument '{}'", cmd.key)))?;

        let channel_id: String = serde_json::from_value(value.clone())
            .map_err(|e| Value::String(format!("Failed to deserialize channel ID: {}", e)))?;

        // チャンネルIDの形式を検証
        if !channel_id.starts_with(IPC_PAYLOAD_PREFIX) {
            return Err(Value::String(format!(
                "Invalid channel value '{}', expected a string in the '{}ID' format",
                channel_id, IPC_PAYLOAD_PREFIX
            )));
        }

        let id = parse_channel_id(&channel_id)
            .map_err(|e| Value::String(format!("Failed to parse channel ID: {}", e)))?;
        let webview_weak = cmd.webview.downgrade();
        Ok(Channel::new(id, webview_weak))
    }
}
