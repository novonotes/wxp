use super::context::CommandContext;
use std::future::Future;
use std::marker::PhantomData;

/// Closure-based async command
pub(super) struct AsyncCommandFn<F, R, E> {
    name: String,
    handler: F,
    _phantom: PhantomData<(R, E)>,
}

impl<F, R, E> AsyncCommandFn<F, R, E> {
    pub(super) fn new(name: &str, handler: F) -> Self {
        Self {
            name: name.into(),
            handler,
            _phantom: PhantomData,
        }
    }
}

impl<F, Fut, R, E> AsyncCommandFn<F, R, E>
where
    F: Fn(CommandContext<'_>) -> Fut,
    Fut: Future<Output = Result<R, E>>,
{
    pub(super) fn name(&self) -> &str {
        &self.name
    }

    pub(super) async fn run(&self, ctx: CommandContext<'_>) -> Result<R, E> {
        (self.handler)(ctx).await
    }
}
