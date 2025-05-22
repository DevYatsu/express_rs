use crate::handler::{
    Request, Response,
    middleware::{Middleware, MiddlewareResult, next},
};
use async_trait::async_trait;
use log::info;

/// Middleware that logs each incoming HTTP request to the console or logger.
///
/// Logs the method, path, and user agent at the beginning of the request lifecycle.
/// This is typically used for debugging and observability in both development and production.
///
/// Example log output:
/// ```text
/// GET /api/items - User-Agent: Mozilla/5.0
/// ```
#[derive(Debug, Clone)]
pub struct LoggingMiddleware;

#[async_trait]
impl Middleware for LoggingMiddleware {
    async fn call(&self, req: &mut Request, _res: &mut Response) -> MiddlewareResult {
        info!(
            "{} {} - User-Agent: {}",
            req.method(),
            req.uri().path(),
            req.headers()
                .get("User-Agent")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("Unknown")
        );
        next()
    }
}
