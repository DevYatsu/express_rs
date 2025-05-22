use super::{client_prefers_json, respond_error};
use crate::handler::{
    Request, Response,
    middleware::{Middleware, MiddlewareResult, next, stop},
};
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
impl Middleware for RateLimitMiddleware {
    async fn call(&self, req: &mut Request, res: &mut Response) -> MiddlewareResult {
        let client_ip = req
            .headers()
            .get("X-Forwarded-For")
            .or_else(|| req.headers().get("X-Real-IP"))
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        if self.is_rate_limited(&client_ip) {
            let retry_after = self.window_size.as_secs().to_string();
            res.status_code(429).unwrap();
            res.header("Retry-After", HeaderValue::from_str(&retry_after).unwrap());

            let wants_json = client_prefers_json(req);

            respond_error(
                res,
                429,
                "Rate limit exceeded",
                json!({
                    "error": "Rate limit exceeded",
                    "message": "Too many requests",
                    "retry_after": retry_after
                }),
                wants_json,
            );

            return stop();
        }

        next()
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
