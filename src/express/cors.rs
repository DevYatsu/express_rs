use crate::handler::{
    Request, Response,
    middleware::{Middleware, MiddlewareResult, next, stop},
};
use async_trait::async_trait;
use hyper::header::HeaderValue;
use std::collections::HashSet;

/// Middleware that adds [CORS](https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS) headers to responses,
/// and handles preflight `OPTIONS` requests.
///
/// Allows fine-grained control over which origins, methods, and headers are accepted in cross-origin requests.
///
/// # Fields
/// - `allowed_origins`: List of allowed origins (`*` allows all).
/// - `allowed_methods`: List of allowed HTTP methods (e.g., `GET`, `POST`).
/// - `allowed_headers`: List of allowed headers in actual requests.
/// - `allow_credentials`: Whether to include the `Access-Control-Allow-Credentials` header.
/// - `max_age`: Optionally caches preflight responses in the browser (in seconds).
///
/// # Example
/// ```rust
/// CorsMiddleware {
///     allowed_origins: vec!["https://example.com".into()].into_iter().collect(),
///     allowed_methods: vec!["GET".into(), "POST".into()].into_iter().collect(),
///     ..Default::default()
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CorsMiddleware {
    /// List of allowed origins (e.g. `["*"]`, or specific domains).
    pub allowed_origins: HashSet<String>,

    /// HTTP methods that are allowed in cross-origin requests.
    pub allowed_methods: HashSet<String>,

    /// Headers allowed in cross-origin requests.
    pub allowed_headers: HashSet<String>,

    /// Whether to include `Access-Control-Allow-Credentials: true`.
    pub allow_credentials: bool,

    /// Maximum cache duration in seconds for preflight responses.
    pub max_age: Option<u32>,
}

impl Default for CorsMiddleware {
    fn default() -> Self {
        Self {
            allowed_origins: HashSet::new(), // No CORS allowed
            allowed_methods: vec!["GET", "HEAD", "PUT", "PATCH", "POST", "DELETE"]
                .into_iter()
                .map(String::from)
                .collect(),
            allowed_headers: HashSet::new(), // No headers predefined
            allow_credentials: false,
            max_age: None,
        }
    }
}

#[async_trait]
impl Middleware for CorsMiddleware {
    async fn call(&self, req: &mut Request, res: &mut Response) -> MiddlewareResult {
        let origin = req.headers().get("Origin").and_then(|h| h.to_str().ok());

        let is_allowed_origin = match origin {
            Some(o) => self.allowed_origins.contains("*") || self.allowed_origins.contains(o),
            None => false,
        };

        if let Some(o) = origin {
            if is_allowed_origin {
                if let Ok(val) = HeaderValue::from_str(o) {
                    res.header("Access-Control-Allow-Origin", val);
                }
            }
        }

        if self.allow_credentials {
            res.header(
                "Access-Control-Allow-Credentials",
                HeaderValue::from_static("true"),
            );
        }

        if req.method() == "OPTIONS" {
            if let Ok(val) = HeaderValue::from_str(
                &self
                    .allowed_methods
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
            ) {
                res.header("Access-Control-Allow-Methods", val);
            }

            if let Ok(val) = HeaderValue::from_str(
                &self
                    .allowed_headers
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
            ) {
                res.header("Access-Control-Allow-Headers", val);
            }

            if let Some(age) = self.max_age {
                if let Ok(val) = HeaderValue::from_str(&age.to_string()) {
                    res.header("Access-Control-Max-Age", val);
                }
            }

            res.status_code(204).unwrap();
            return stop();
        }

        next()
    }
}

impl CorsMiddleware {
    /// Create a new CORS middleware with full configuration.
    pub fn new(
        allowed_origins: impl IntoIterator<Item = impl Into<String>>,
        allowed_methods: impl IntoIterator<Item = impl Into<String>>,
        allowed_headers: impl IntoIterator<Item = impl Into<String>>,
        allow_credentials: bool,
        max_age: Option<u32>,
    ) -> Self {
        Self {
            allowed_origins: allowed_origins.into_iter().map(Into::into).collect(),
            allowed_methods: allowed_methods.into_iter().map(Into::into).collect(),
            allowed_headers: allowed_headers.into_iter().map(Into::into).collect(),
            allow_credentials,
            max_age,
        }
    }

    /// Returns a permissive CORS config (suitable for development):
    ///
    /// - Allows all origins: `*`
    /// - Allows common methods: GET, POST, PUT, DELETE, OPTIONS
    /// - Allows headers: Content-Type, Authorization, X-Requested-With
    /// - Allows credentials
    /// - Max-Age: 86400s
    pub fn permissive() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()].into_iter().collect(),
            allowed_methods: vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"]
                .into_iter()
                .map(String::from)
                .collect(),
            allowed_headers: vec!["Content-Type", "Authorization", "X-Requested-With"]
                .into_iter()
                .map(String::from)
                .collect(),
            allow_credentials: true,
            max_age: Some(86400),
        }
    }

    /// Allow a specific origin.
    pub fn allow_origin(mut self, origin: &str) -> Self {
        self.allowed_origins.insert(origin.to_string());
        self
    }

    /// Allow multiple origins.
    pub fn allow_origins<I>(mut self, origins: I) -> Self
    where
        I: IntoIterator,
        I::Item: ToString,
    {
        self.allowed_origins
            .extend(origins.into_iter().map(|s| s.to_string()));
        self
    }

    /// Allow specific methods.
    pub fn allow_methods<I>(mut self, methods: I) -> Self
    where
        I: IntoIterator,
        I::Item: ToString,
    {
        self.allowed_methods
            .extend(methods.into_iter().map(|s| s.to_string()));
        self
    }

    /// Allow specific headers.
    pub fn allow_headers<I>(mut self, headers: I) -> Self
    where
        I: IntoIterator,
        I::Item: ToString,
    {
        self.allowed_headers
            .extend(headers.into_iter().map(|s| s.to_string()));
        self
    }

    /// Enable credentials.
    pub fn allow_credentials(mut self, enabled: bool) -> Self {
        self.allow_credentials = enabled;
        self
    }

    /// Set the max age for preflight caching.
    pub fn max_age(mut self, seconds: u32) -> Self {
        self.max_age = Some(seconds);
        self
    }
}
