use super::context::CommandContext;
use std::marker::PhantomData;

/// 関数ベースのコマンド実装
pub(crate) struct SyncCommandFn<F, R, E> {
    name: String,
    handler: F,
    _phantom: PhantomData<(R, E)>,
}

impl<F, R, E> SyncCommandFn<F, R, E> {
    pub(crate) fn new(name: &str, handler: F) -> Self {
        Self {
            name: name.to_string(),
            handler,
            _phantom: PhantomData,
        }
    }
}

impl<F, R, E> SyncCommandFn<F, R, E>
where
    F: Fn(CommandContext<'_>) -> Result<R, E> + Send + Sync,
{
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) async fn run(&self, ctx: CommandContext<'_>) -> Result<R, E> {
        (self.handler)(ctx)
    }
}
