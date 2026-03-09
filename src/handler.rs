/// Provides request parsing and extraction utilities.
pub mod request;
/// Provides response creation and formatting utilities.
pub mod response;

use async_trait::async_trait;
use hyper::body::Incoming;
pub use request::Request;
pub use response::{ExpressResponse, Response, ResponseError};

/// Trait for async handler abstraction.
#[async_trait]
pub trait Handler<B = Incoming>: Send + Sync + 'static {
    /// Invokes the handler asynchronously.
    async fn call(&self, req: Request<B>, res: Response) -> Response;
}

/// Blanket impl for closures or functions that match the async signature.
#[async_trait]
impl<F, Fut, B> Handler<B> for F
where
    F: Fn(Request<B>, Response) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
    B: Send + 'static,
{
    async fn call(&self, req: Request<B>, res: Response) -> Response {
        (self)(req, res).await
    }
}
