use crate::{
    express::respond_error,
    handler::{
        Request, Response,
        middleware::{Middleware, MiddlewareResult, next, stop},
    },
};
use async_trait::async_trait;
use log::warn;
use serde_json::json;

use super::client_prefers_json;

/// Middleware that rejects requests with a `Content-Length` exceeding the allowed limit.
/// Can respond in either JSON or plain text depending on the `Accept` header.
#[derive(Debug, Clone)]
pub struct BodySizeLimitMiddleware {
    /// Max body size in bytes.
    pub max_size_bytes: usize,
    /// If `true`, reject requests that do not have a Content-Length header.
    pub strict: bool,
}

impl Default for BodySizeLimitMiddleware {
    fn default() -> Self {
        Self {
            max_size_bytes: 10 * 1024 * 1024, // 10 MB
            strict: false,
        }
    }
}

#[async_trait]
impl Middleware for BodySizeLimitMiddleware {
    async fn call(&self, req: &mut Request, res: &mut Response) -> MiddlewareResult {
        let wants_json = client_prefers_json(req);

        // Handle missing Content-Length
        let Some(header) = req.headers().get("Content-Length") else {
            if self.strict {
                warn!("Strict mode: Content-Length header is missing.");
                res.status_code(411).unwrap();

                respond_error(
                    res,
                    411,
                    "Content-Length header required",
                    json!({
                        "error": "Content-Length header required",
                        "max_size_bytes": self.max_size_bytes
                    }),
                    wants_json,
                );

                return stop();
            }
            return next();
        };

        let Ok(length_str) = header.to_str() else {
            warn!("Invalid Content-Length header format.");
            return next();
        };

        let Ok(length) = length_str.parse::<usize>() else {
            warn!("Unable to parse Content-Length as usize.");
            return next();
        };

        if length > self.max_size_bytes {
            warn!(
                "Rejected request: content-length {} exceeds limit {}",
                length, self.max_size_bytes
            );

            res.status_code(413).unwrap();

            respond_error(
                res,
                413,
                "Payload too large",
                json!({
                    "error": "Payload too large",
                    "max_size_bytes": self.max_size_bytes,
                    "actual_size": length
                }),
                wants_json,
            );

            return stop();
        }

        next()
    }
}
