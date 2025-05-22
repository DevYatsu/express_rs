use super::{Request, Response};
use async_trait::async_trait;

/// Trait for async handler abstraction.
#[async_trait]
pub trait FnHandler: Send + Sync + 'static {
    async fn call(&self, req: Request, res: Response) -> Response;
}

/// Blanket impl for closures or functions that match the async signature.
#[async_trait]
impl<F, Fut> FnHandler for F
where
    F: Fn(Request, Response) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    async fn call(&self, req: Request, res: Response) -> Response {
        (self)(req, res).await
    }
}
