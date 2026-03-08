use crate::handler::{ExpressResponse, Request, Response};
use crate::middleware::{Middleware, MiddlewareResult, next_res};
use async_trait::async_trait;
use hyper::header::{CACHE_CONTROL, HeaderValue};

/// Middleware to set Cache-Control headers on responses.
#[derive(Debug, Clone)]
pub struct CacheMiddleware {
    value: String,
}

impl CacheMiddleware {
    /// Create a new CacheMiddleware with a custom Cache-Control value.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Create a new CacheMiddleware that sets public, max-age in seconds.
    pub fn public(max_age: u64) -> Self {
        Self::new(format!("public, max-age={}", max_age))
    }

    /// Create a new CacheMiddleware that sets private, max-age in seconds.
    pub fn private(max_age: u64) -> Self {
        Self::new(format!("private, max-age={}", max_age))
    }

    /// Create a new CacheMiddleware that disables caching.
    pub fn no_store() -> Self {
        Self::new("no-store, no-cache, must-revalidate, proxy-revalidate")
    }
}

#[async_trait]
impl<B: Send + Sync + 'static> Middleware<B> for CacheMiddleware {
    async fn call(&self, _req: &mut Request<B>, res: &mut Response) -> MiddlewareResult {
        if let Ok(val) = HeaderValue::from_str(&self.value) {
            res.header(CACHE_CONTROL, val);
        }
        next_res()
    }
}
