use crate::handler::{ExpressResponse, Request, Response};
use crate::middleware::{Middleware, MiddlewareResult, next_res, stop_res};
use crate::prelude::RequestExt;
use async_trait::async_trait;
use hyper::header::{CACHE_CONTROL, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, HeaderValue};
use std::path::{Component, Path};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct StaticServeMiddleware {
    root: String,
    max_age: Option<u64>,
}

impl Default for StaticServeMiddleware {
    fn default() -> Self {
        Self {
            root: ".".to_string(),
            max_age: Some(3600), // Default to 1 hour
        }
    }
}

impl StaticServeMiddleware {
    /// Create a new StaticServeMiddleware that serves files from the given root directory.
    pub fn new(root: impl Into<String>) -> Self {
        let mut root_str = root.into();
        // ensure no trailing slash in root for predictable concatenation
        if root_str.ends_with('/') {
            root_str.pop();
        }
        Self {
            root: root_str,
            max_age: Some(3600),
        }
    }

    /// Set the Max-Age for the Cache-Control header in seconds.
    pub fn max_age(mut self, seconds: u64) -> Self {
        self.max_age = Some(seconds);
        self
    }

    /// Disable caching for this middleware.
    pub fn no_cache(mut self) -> Self {
        self.max_age = None;
        self
    }
}

#[async_trait]
impl<B: Send + Sync + 'static> Middleware<B> for StaticServeMiddleware {
    async fn call(&self, req: &mut Request<B>, res: &mut Response) -> MiddlewareResult {
        // Extract the relative path if the middleware was mounted with a wildcard
        // e.g., app.use_with("/src/{*p}", ...) -> parameter is "p"
        let raw_path = req.params().get("p")
            .or_else(|| req.params().get("path"))
            .or_else(|| req.params().get("file"))
            .unwrap_or_else(|| req.uri().path());

        // Prevent path traversal vulnerabilities by parsing path components safely
        let mut clean_path = String::new();
        for component in Path::new(raw_path).components() {
            if let Component::Normal(c) = component {
                clean_path.push('/');
                clean_path.push_str(&c.to_string_lossy());
            }
        }

        // If path is empty (just "/"), default to "/index.html" (or let send_file fail, we will fallback)
        if clean_path.is_empty() {
            clean_path.push_str("/index.html");
        }

        let file_path = format!("{}{}", self.root, clean_path);
        let path = Path::new(&file_path);

        if !path.exists() {
            return next_res();
        }

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return next_res(),
        };

        if metadata.is_dir() {
            return next_res();
        }

        // Generate a weak ETag based on modified time and file size
        let last_modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let timestamp = last_modified.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
        let etag_val = format!("W/\"{:x}-{:x}\"", metadata.len(), timestamp);

        // Check conditional match (ETag)
        if let Some(if_none_match) = req.headers().get(IF_NONE_MATCH) {
            if if_none_match.to_str().unwrap_or_default() == etag_val {
                *res = Response::new().status(hyper::StatusCode::NOT_MODIFIED);
                return stop_res();
            }
        }

        // Check conditional match (Last-Modified)
        if let Some(if_modified_since) = req.headers().get(IF_MODIFIED_SINCE) {
            if let Ok(since) = httpdate::parse_http_date(if_modified_since.to_str().unwrap_or_default()) {
                if last_modified <= since {
                    *res = Response::new().status(hyper::StatusCode::NOT_MODIFIED);
                    return stop_res();
                }
            }
        }

        let temp_res = std::mem::take(res);
        let mut new_res = temp_res.send_file(&file_path).await;

        if new_res.error.is_some() {
            *res = new_res;
            return next_res();
        }

        // Apply caching headers
        if let Some(max_age) = self.max_age {
            if let Ok(val) = HeaderValue::from_str(&format!("public, max-age={}", max_age)) {
                new_res = new_res.header(CACHE_CONTROL, val);
            }
        }

        // Add ETag and Last-Modified
        if let Ok(val) = HeaderValue::from_str(&etag_val) {
            new_res = new_res.header(ETAG, val);
        }
        if let Ok(date) = httpdate::fmt_http_date(last_modified).parse::<HeaderValue>() {
            new_res = new_res.header(LAST_MODIFIED, date);
        }

        *res = new_res.status_code(200);
        stop_res()
    }
}
