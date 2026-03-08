use crate::handler::{ExpressResponse, Request, Response};
use crate::middleware::{Middleware, MiddlewareResult, next_res, stop_res};
use async_trait::async_trait;
use hyper::header::HeaderValue;
use std::collections::HashSet;

/// Middleware that adds [CORS](https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS) headers to responses,
/// and handles preflight `OPTIONS` requests.
#[derive(Debug, Clone)]
pub struct CorsMiddleware {
    pub allowed_origins: HashSet<String>,
    pub allowed_methods: HashSet<String>,
    pub allowed_headers: HashSet<String>,
    pub allow_credentials: bool,
    pub max_age: Option<u32>,
}

impl Default for CorsMiddleware {
    fn default() -> Self {
        Self {
            allowed_origins: HashSet::new(),
            allowed_methods: vec!["GET", "HEAD", "PUT", "PATCH", "POST", "DELETE"]
                .into_iter()
                .map(String::from)
                .collect(),
            allowed_headers: HashSet::new(),
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

        if let Some(o) = origin
            && is_allowed_origin
        {
            if let Ok(val) = HeaderValue::from_str(o) {
                res.header("Access-Control-Allow-Origin", val);
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

            if let Some(age) = self.max_age
                && let Ok(val) = HeaderValue::from_str(&age.to_string())
            {
                res.header("Access-Control-Max-Age", val);
            }

            res.status_code(204);
            return stop_res();
        }

        next_res()
    }
}

impl CorsMiddleware {
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
}
