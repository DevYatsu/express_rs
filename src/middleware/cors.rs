use crate::handler::{ExpressResponse, Request, Response};
use crate::middleware::{Middleware, MiddlewareResult, next_res, stop_res};
use async_trait::async_trait;
use hyper::header::HeaderValue;
use rustc_hash::FxHashSet;

/// Middleware that adds [CORS](https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS) headers to responses,
/// and handles preflight `OPTIONS` requests.
///
/// ## Performance
/// The `Allow-Methods` and `Allow-Headers` header values are pre-computed at
/// construction time so no allocation happens on the hot path.
#[derive(Debug, Clone)]
pub struct CorsMiddleware {
    /// Standard origins authorized by CORS.
    pub allowed_origins: FxHashSet<String>,
    /// HTTP methods allowed by CORS.
    pub allowed_methods: FxHashSet<String>,
    /// Headers allowed by CORS.
    pub allowed_headers: FxHashSet<String>,
    /// Indicates whether the response can be shared when credentials flag is true.
    pub allow_credentials: bool,
    /// How long the results of a preflight request can be cached.
    pub max_age: Option<u32>,
    // Pre-computed header values — built once in `new_inner`.
    methods_header: Option<HeaderValue>,
    headers_header: Option<HeaderValue>,
    max_age_header: Option<HeaderValue>,
}

impl CorsMiddleware {
    fn new_inner(
        allowed_origins: FxHashSet<String>,
        allowed_methods: FxHashSet<String>,
        allowed_headers: FxHashSet<String>,
        allow_credentials: bool,
        max_age: Option<u32>,
    ) -> Self {
        let methods_header = HeaderValue::from_str(
            &allowed_methods
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        )
        .ok();

        let headers_header = if allowed_headers.is_empty() {
            None
        } else {
            HeaderValue::from_str(
                &allowed_headers
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
            )
            .ok()
        };

        let max_age_header = max_age.and_then(|age| HeaderValue::from_str(&age.to_string()).ok());

        Self {
            allowed_origins,
            allowed_methods,
            allowed_headers,
            allow_credentials,
            max_age,
            methods_header,
            headers_header,
            max_age_header,
        }
    }

    /// Construct a permissive CORS config that allows all origins.
    pub fn permissive() -> Self {
        Self::new_inner(
            ["*".to_string()].into_iter().collect(),
            ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
                .into_iter()
                .map(String::from)
                .collect(),
            ["Content-Type", "Authorization", "X-Requested-With"]
                .into_iter()
                .map(String::from)
                .collect(),
            true,
            Some(86400),
        )
    }
}

impl Default for CorsMiddleware {
    fn default() -> Self {
        Self::new_inner(
            FxHashSet::default(),
            ["GET", "HEAD", "PUT", "PATCH", "POST", "DELETE"]
                .into_iter()
                .map(String::from)
                .collect(),
            FxHashSet::default(),
            false,
            None,
        )
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

        if let Some(o) = origin
            && is_allowed_origin
            && let Ok(val) = HeaderValue::from_str(o) {
                res.header("Access-Control-Allow-Origin", val);
            }

        if self.allow_credentials {
            res.header(
                "Access-Control-Allow-Credentials",
                HeaderValue::from_static("true"),
            );
        }

        if req.method() == "OPTIONS" {
            // Use the pre-computed header values — zero allocation on this path.
            if let Some(val) = &self.methods_header {
                res.header("Access-Control-Allow-Methods", val.clone());
            }
            if let Some(val) = &self.headers_header {
                res.header("Access-Control-Allow-Headers", val.clone());
            }
            if let Some(val) = &self.max_age_header {
                res.header("Access-Control-Max-Age", val.clone());
            }

            res.status_code(204);
            return stop_res();
        }

        next_res()
    }
}
