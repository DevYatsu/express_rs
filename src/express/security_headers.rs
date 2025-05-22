use crate::handler::{
    Request, Response,
    middleware::{Middleware, MiddlewareResult, next},
};
use async_trait::async_trait;
use hyper::header::HeaderValue;

/// Middleware that injects common HTTP security headers into the response.
///
/// This middleware enhances basic security by setting the following headers:
/// - `Content-Security-Policy`
/// - `X-XSS-Protection`
/// - `X-Content-Type-Options`
/// - `X-Frame-Options`
/// - `Referrer-Policy`
/// - `Strict-Transport-Security`
///
/// These headers help mitigate common browser-based attacks like XSS, MIME sniffing,
/// clickjacking, and downgrade attacks.
#[derive(Debug, Clone)]
pub struct SecurityHeadersMiddleware;

#[async_trait]
impl Middleware for SecurityHeadersMiddleware {
    /// Injects security headers into the response.
    ///
    /// This middleware does **not** inspect the request or block it based on policy.
    /// It simply adds defensive headers for the response.
    async fn call(&self, _req: &mut Request, res: &mut Response) -> MiddlewareResult {
        // Content Security Policy
        res.header(
            "Content-Security-Policy",
            HeaderValue::from_static("default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline';")
        );

        // XSS Protection
        res.header(
            "X-XSS-Protection",
            HeaderValue::from_static("1; mode=block"),
        );

        // Prevent MIME type sniffing
        res.header(
            "X-Content-Type-Options",
            HeaderValue::from_static("nosniff"),
        );

        // Disallow embedding in frames
        res.header("X-Frame-Options", HeaderValue::from_static("DENY"));

        // Control referrer information
        res.header(
            "Referrer-Policy",
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        );

        // Enforce HTTPS for future requests
        res.header(
            "Strict-Transport-Security",
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );

        next()
    }
}
