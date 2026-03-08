use crate::handler::{Request, Response};
use crate::middleware::{Middleware, MiddlewareResult, next_res};
use async_trait::async_trait;
use log::info;

/// Middleware that logs each incoming HTTP request to the console or logger.
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
        next_res()
    }
}
