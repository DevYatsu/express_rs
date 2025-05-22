use futures_core::future::BoxFuture;
use futures_util::FutureExt;
use std::future::Future;

use super::{Request, Response};

pub type HandlerFuture = BoxFuture<'static, Response>;

/// Trait for async handler abstraction.
pub trait FnHandler: Send + Sync + 'static {
    fn call(&self, req: Request, response: Response) -> HandlerFuture;
}

/// Blanket impl for closures or functions that match the async signature.
impl<F, Fut> FnHandler for F
where
    F: Fn(Request, Response) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    fn call(&self, req: Request, res: Response) -> HandlerFuture {
        (self)(req, res).boxed()
    }
}
