use crate::application::App;
use crate::handler::{Request, Response};
use async_trait::async_trait;
use hyper::body::Incoming;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MiddlewareResult {
    Next,
    Stop,
}

#[async_trait]
pub trait Middleware<B = Incoming>: Send + Sync + 'static {
    async fn call(&self, req: &mut Request<B>, res: &mut Response) -> MiddlewareResult;
}

pub const fn next_res() -> MiddlewareResult {
    MiddlewareResult::Next
}
pub const fn stop_res() -> MiddlewareResult {
    MiddlewareResult::Stop
}

impl MiddlewareResult {
    pub const fn is_next(&self) -> bool {
        matches!(self, MiddlewareResult::Next)
    }

    pub const fn is_stop(&self) -> bool {
        matches!(self, MiddlewareResult::Stop)
    }
}

/// Blanket impl for closures or functions that match the async signature.
#[async_trait]
impl<F, Fut> Middleware for F
where
    F: Send + Sync + 'static + for<'a> Fn(&'a mut Request, &'a mut Response) -> Fut,
    Fut: Future<Output = MiddlewareResult> + Send + 'static,
{
    async fn call(&self, req: &mut Request, res: &mut Response) -> MiddlewareResult {
        (self)(req, res).await
    }
}

// Submodules
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

pub fn app() -> App {
    App::default()
}
