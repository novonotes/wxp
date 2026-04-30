use super::async_command::AsyncCommandFn;
use super::command::SyncCommandFn;
use super::context::CommandContext;
use async_trait::async_trait;
use serde_json::Value;
use std::future::Future;
use std::rc::Rc;

/// Trait for handling both sync and async commands uniformly
#[async_trait(?Send)]
pub(super) trait UnifiedCommand {
    /// Executes the command
    async fn execute(&self, ctx: CommandContext<'_>) -> Result<Value, Value>;
}

/// Implements UnifiedCommand for WxpCommandFn
#[async_trait(?Send)]
impl<F, R, E> UnifiedCommand for SyncCommandFn<F, R, E>
where
    F: Fn(CommandContext<'_>) -> Result<R, E>,
    R: serde::Serialize,
    E: serde::Serialize,
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

/// Implements UnifiedCommand for AsyncCommandFn
#[async_trait(?Send)]
impl<F, Fut, R, E> UnifiedCommand for AsyncCommandFn<F, R, E>
where
    F: Fn(CommandContext<'_>) -> Fut,
    Fut: Future<Output = Result<R, E>>,
    R: serde::Serialize,
    E: serde::Serialize,
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

/// Command wrapper for dynamic dispatch
pub(super) type DynUnifiedCommand = Rc<dyn UnifiedCommand>;
