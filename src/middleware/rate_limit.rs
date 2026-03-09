use crate::handler::{ExpressResponse, Request, Response, request::RequestExt};
use crate::middleware::{Middleware, MiddlewareResult, next_res, stop_res};
use async_trait::async_trait;
use dashmap::DashMap;
use hyper::header::HeaderValue;
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Middleware that limits the number of requests a client can make within a given time window.
///
/// This implementation tracks requests per IP address using an in-memory concurrent map (`DashMap`).
/// If the number of requests from a given IP exceeds the configured `requests_per_minute`
/// within the `window_size`, subsequent requests are blocked until the window resets.
///
/// Note: In a production setting, a distributed store like Redis should be preferred.
#[derive(Debug, Clone)]
pub struct RateLimitMiddleware {
    /// The maximum number of requests allowed per client within the time window.
    pub requests_per_minute: u32,

    /// The size of the rate limit window (e.g. 60 seconds).
    pub window_size: Duration,

    /// Internal in-memory store mapping IP addresses to rate limit state.
    store: SharedRateLimitStore,
}

/// A shared, concurrent map for tracking rate limit entries per client.
///
/// Maps an IP address to its request count and last access timestamp.
/// Uses `Arc<DashMap<...>>` to support thread-safe shared access across async requests.
type SharedRateLimitStore = Arc<DashMap<String, RateLimitEntry>>;

/// Represents a client's rate limit state.
///
/// Stores the time when the current window started (`timestamp`) and the number of requests made (`count`).
#[derive(Debug)]
struct RateLimitEntry {
    timestamp: std::time::Instant,
    count: u32,
}

impl Default for RateLimitMiddleware {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            window_size: Duration::from_secs(60),
            store: Arc::new(DashMap::new()),
        }
    }
}

#[async_trait]
impl<B: Send + Sync + 'static> Middleware<B> for RateLimitMiddleware {
    async fn call(&self, req: &mut Request<B>, res: &mut Response) -> MiddlewareResult {
        // Use the real socket address as the primary key — it cannot be spoofed
        // unlike X-Forwarded-For headers. Fall back to proxy headers only when
        // the socket address is unavailable (shouldn't happen in practice).
        let client_ip: String = req
            .ip()
            .map(|addr| addr.ip().to_string())
            .unwrap_or_else(|| {
                req.get_header("X-Forwarded-For")
                    .or_else(|| req.get_header("X-Real-IP"))
                    .unwrap_or("unknown")
                    .to_string()
            });

        if self.is_rate_limited(&client_ip) {
            let retry_after = self.window_size.as_secs().to_string();
            res.header("Retry-After", HeaderValue::from_str(&retry_after).unwrap());

            let wants_json = req.prefers_json();

            res.respond_error(
                429,
                "Rate limit exceeded",
                json!({
                    "error": "Rate limit exceeded",
                    "message": "Too many requests",
                    "retry_after": retry_after
                }),
                wants_json,
            );

            return stop_res();
        }

        next_res()
    }
}

impl RateLimitMiddleware {
    /// Creates a new RateLimitMiddleware with custom limits.
    pub fn new(requests_per_minute: u32, window_size: Duration) -> Self {
        Self {
            requests_per_minute,
            window_size,
            store: std::sync::Arc::new(DashMap::new()),
        }
    }

    fn is_rate_limited(&self, ip: &str) -> bool {
        let now = Instant::now();

        let mut entry = self.store.entry(ip.to_string()).or_insert(RateLimitEntry {
            timestamp: now,
            count: 0,
        });

        if now.duration_since(entry.timestamp) > self.window_size {
            entry.timestamp = now;
            entry.count = 1;
            false
        } else if entry.count >= self.requests_per_minute {
            true
        } else {
            entry.count += 1;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::Response;
    use std::time::Duration;

    #[tokio::test]
    async fn test_rate_limit_basic() {
        let mw = RateLimitMiddleware::new(2, Duration::from_secs(60));
        let mut res = Response::new();

        let mut req1 = Request::builder().uri("/").body(()).unwrap();
        assert!(mw.call(&mut req1, &mut res).await.is_next());

        let mut req2 = Request::builder().uri("/").body(()).unwrap();
        assert!(mw.call(&mut req2, &mut res).await.is_next());

        let mut req3 = Request::builder().uri("/").body(()).unwrap();
        assert!(mw.call(&mut req3, &mut res).await.is_stop());
        assert_eq!(res.get_status(), hyper::StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_rate_limit_window_reset() {
        let mw = RateLimitMiddleware::new(1, Duration::from_millis(100));
        let mut res = Response::new();

        let mut req1 = Request::builder().uri("/").body(()).unwrap();
        assert!(mw.call(&mut req1, &mut res).await.is_next());

        let mut req2 = Request::builder().uri("/").body(()).unwrap();
        assert!(mw.call(&mut req2, &mut res).await.is_stop());

        tokio::time::sleep(Duration::from_millis(150)).await;

        let mut req3 = Request::builder().uri("/").body(()).unwrap();
        assert!(mw.call(&mut req3, &mut res).await.is_next());
    }
}
