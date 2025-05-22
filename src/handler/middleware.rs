use super::{Request, Response};
use async_trait::async_trait;
use futures_core::future::BoxFuture;

pub type MiddlewareFuture<'a> = BoxFuture<'a, MiddlewareResult>;

#[derive(Debug)]
pub enum MiddlewareResult {
    Next,
    Stop,
}

/// Trait for middleware handlers.
#[async_trait]
pub trait Middleware: Send + Sync + 'static {
    /// The type of the handler that will be invoked if this middleware matches.
    async fn call(&self, req: &mut Request, res: &mut Response) -> MiddlewareResult;
}

pub fn next() -> MiddlewareFuture<'static> {
    MiddlewareResult::Next.boxed()
}
pub fn stop() -> MiddlewareFuture<'static> {
    MiddlewareResult::Stop.boxed()
}

impl MiddlewareResult {
    pub fn is_next(&self) -> bool {
        matches!(self, MiddlewareResult::Next)
    }

    pub fn is_stop(&self) -> bool {
        matches!(self, MiddlewareResult::Stop)
    }

    pub fn boxed(self) -> MiddlewareFuture<'static> {
        Box::pin(async move { self })
    }
}

/// Blanket impl for closures or functions that match the async signature.
#[async_trait]
impl<F, Fut> Middleware for F
where
    F: Send + Sync + 'static + for<'a> Fn(&'a mut Request, &'a mut Response) -> Fut,
    Fut: std::future::Future<Output = MiddlewareResult> + Send + 'static,
{
    async fn call(&self, req: &mut Request, res: &mut Response) -> MiddlewareResult {
        (self)(req, res).await
    }
}
