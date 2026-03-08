use crate::handler::{Request, Response};
use crate::middleware::{next_res, Middleware, MiddlewareResult};
use async_trait::async_trait;
use hyper::Uri;

/// Middleware to normalize the request path.
/// It collapses consecutive slashes into a single slash and removes trailing slashes.
#[derive(Debug, Clone)]
pub struct NormalizePathMiddleware {
    strip_trailing_slash: bool,
    merge_slashes: bool,
}

impl Default for NormalizePathMiddleware {
    fn default() -> Self {
        Self {
            strip_trailing_slash: true,
            merge_slashes: true,
        }
    }
}

impl NormalizePathMiddleware {
    /// Create a new NormalizePathMiddleware with default settings (all normalizations enabled).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether trailing slashes should be removed.
    pub fn strip_trailing_slash(mut self, strip: bool) -> Self {
        self.strip_trailing_slash = strip;
        self
    }

    /// Set whether consecutive slashes should be merged into a single slash.
    pub fn merge_slashes(mut self, merge: bool) -> Self {
        self.merge_slashes = merge;
        self
    }
}

#[async_trait]
impl<B: Send + Sync + 'static> Middleware<B> for NormalizePathMiddleware {
    async fn call(&self, req: &mut Request<B>, _res: &mut Response) -> MiddlewareResult {
        let path = req.uri().path();

        // Fast path: Determine if any allocation/modification is actually required
        let needs_merge = self.merge_slashes && path.contains("//");
        let needs_strip = self.strip_trailing_slash && path.len() > 1 && path.ends_with('/');

        if !needs_merge && !needs_strip {
            // Hot path: the path is already normalized, do zero allocations
            return next_res();
        }

        let mut normalized = if needs_merge {
            let mut buf = String::with_capacity(path.len());
            let mut last_was_slash = false;
            for c in path.chars() {
                if c == '/' {
                    if !last_was_slash {
                        buf.push(c);
                    }
                    last_was_slash = true;
                } else {
                    buf.push(c);
                    last_was_slash = false;
                }
            }
            buf
        } else {
            path.to_string()
        };

        if self.strip_trailing_slash && normalized.len() > 1 && normalized.ends_with('/') {
            normalized.pop();
        }

        let mut parts = req.uri().clone().into_parts();
        let new_path_and_query = match parts.path_and_query {
            Some(pq) => {
                if let Some(query) = pq.query() {
                    format!("{}?{}", normalized, query).parse().ok()
                } else {
                    normalized.parse().ok()
                }
            }
            None => normalized.parse().ok(),
        };

        if let Some(pq) = new_path_and_query {
            parts.path_and_query = Some(pq);
            if let Ok(new_uri) = Uri::from_parts(parts) {
                *req.uri_mut() = new_uri;
            }
        }

        next_res()
    }
}
