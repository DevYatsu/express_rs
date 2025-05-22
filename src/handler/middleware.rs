use super::{Request, Response};
use futures_core::future::BoxFuture;

pub type MiddlewareFuture<'a> = BoxFuture<'a, MiddlewareResult>;

#[derive(Debug)]
pub enum MiddlewareResult {
    Next,
    Stop,
}

/// Trait for middleware handlers.
pub trait Middleware: Send + Sync + 'static {
    /// The type of the handler that will be invoked if this middleware matches.
    fn call<'a, 'b>(&'a self, req: &'b mut Request, res: &'b mut Response) -> MiddlewareFuture<'b>;
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
impl<F, Fut> Middleware for F
where
    F: for<'a> Fn(&'a mut Request, &'a mut Response) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = MiddlewareResult> + Send + 'static,
{
    fn call<'a, 'b>(&'a self, req: &'b mut Request, res: &'b mut Response) -> MiddlewareFuture<'b> {
        Box::pin((self)(req, res))
    }
}
