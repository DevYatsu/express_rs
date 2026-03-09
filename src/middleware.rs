use crate::application::App;
use crate::handler::{Request, Response};
use async_trait::async_trait;
use hyper::body::Incoming;

/// The result of executing a middleware function.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MiddlewareResult {
    /// Resume the request flow by calling the next layer.
    Next,
    /// Cancel processing to respond out-of-turn immediately.
    Stop,
}

/// The base trait for all Express-like middleware components.
#[async_trait]
pub trait Middleware<B = Incoming>: Send + Sync + 'static {
    /// Executes the middleware function to mutate request and response structures inline.
    async fn call(&self, req: &mut Request<B>, res: &mut Response) -> MiddlewareResult;
}

/// Helper function to yield execution to the next layer in the router stack.
pub const fn next_res() -> MiddlewareResult {
    MiddlewareResult::Next
}
/// Helper function to halt execution across the layer stack and respond.
pub const fn stop_res() -> MiddlewareResult {
    MiddlewareResult::Stop
}

impl MiddlewareResult {
    /// Determines if the current result indicates that the stack should proceed.
    pub const fn is_next(&self) -> bool {
        matches!(self, MiddlewareResult::Next)
    }

    /// Determines if the current result indicates that processing should be halted.
    pub const fn is_stop(&self) -> bool {
        matches!(self, MiddlewareResult::Stop)
    }
}

/// Blanket impl for closures or functions that match the async signature.
#[async_trait]
impl<B, F, Fut> Middleware<B> for F
where
    B: Send + Sync + 'static,
    F: Send + Sync + 'static + for<'a> Fn(&'a mut Request<B>, &'a mut Response) -> Fut,
    Fut: Future<Output = MiddlewareResult> + Send + 'static,
{
    async fn call(&self, req: &mut Request<B>, res: &mut Response) -> MiddlewareResult {
        (self)(req, res).await
    }
}

// Submodules
/// Authentication module.
pub mod auth;
mod cache;
mod cors;
mod limit_body;
mod logging;
mod normalize_path;
mod rate_limit;
mod security_headers;
mod static_serve;

pub use auth::AuthMiddleware;
pub use cache::CacheMiddleware;
pub use cors::CorsMiddleware;
pub use logging::LoggingMiddleware;
pub use normalize_path::NormalizePathMiddleware;
pub use rate_limit::RateLimitMiddleware;
pub use security_headers::SecurityHeadersMiddleware;
pub use static_serve::StaticServeMiddleware;

/// Initializes a new `express` application.
pub fn app() -> App {
    App::default()
}
