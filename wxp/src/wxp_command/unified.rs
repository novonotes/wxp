use super::async_command::AsyncCommandFn;
use super::command::SyncCommandFn;
use super::context::CommandContext;
use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;

/// 同期・非同期コマンドを統一的に扱うトレイト
#[async_trait]
pub(super) trait UnifiedCommand: Send + Sync {
    /// コマンドを実行
    async fn execute(&self, ctx: CommandContext<'_>) -> Result<Value, Value>;
}

/// WxpCommandFn に UnifiedCommand を実装
#[async_trait]
impl<F, R, E> UnifiedCommand for SyncCommandFn<F, R, E>
where
    F: Fn(CommandContext<'_>) -> Result<R, E> + Send + Sync,
    R: serde::Serialize + Send + Sync,
    E: serde::Serialize + Send + Sync,
{
    async fn execute(&self, ctx: CommandContext<'_>) -> Result<Value, Value> {
        match self.run(ctx).await {
            Ok(value) => serde_json::to_value(value)
                .map_err(|e| Value::String(format!("Failed to serialize result: {}", e))),
            Err(error) => Err(serde_json::to_value(error)
                .unwrap_or_else(|e| Value::String(format!("Failed to serialize error: {}", e)))),
        }
    }
}

/// AsyncCommandFn に UnifiedCommand を実装
#[async_trait]
impl<F, Fut, R, E> UnifiedCommand for AsyncCommandFn<F, R, E>
where
    F: Fn(CommandContext<'_>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<R, E>> + Send,
    R: serde::Serialize + Send + Sync,
    E: serde::Serialize + Send + Sync,
{
    async fn execute(&self, ctx: CommandContext<'_>) -> Result<Value, Value> {
        match self.run(ctx).await {
            Ok(value) => serde_json::to_value(value)
                .map_err(|e| Value::String(format!("Failed to serialize result: {}", e))),
            Err(error) => Err(serde_json::to_value(error)
                .unwrap_or_else(|e| Value::String(format!("Failed to serialize error: {}", e)))),
        }
    }
}

/// 動的ディスパッチ用のコマンドラッパー
pub(super) type DynUnifiedCommand = Arc<dyn UnifiedCommand>;
