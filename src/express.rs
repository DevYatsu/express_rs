use crate::{
    application::App,
    handler::{Request, Response},
};

pub mod auth;
mod cors;
mod limit_body;
mod logging;
mod rate_limit;
mod security_headers;
mod static_serve;

pub use auth::middleware::AuthMiddleware;
pub use cors::CorsMiddleware;
use hyper::header::{ACCEPT, CONTENT_TYPE, HeaderValue};
pub use logging::LoggingMiddleware;
pub use rate_limit::RateLimitMiddleware;
pub use security_headers::SecurityHeadersMiddleware;
pub use static_serve::StaticServeMiddleware;

pub fn app() -> App {
    App::default()
}

/// Determines whether the client prefers a JSON response based on the Accept header.
fn client_prefers_json(req: &Request) -> bool {
    req.headers()
        .get(ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map_or(true, |accept| accept.contains("application/json"))
}

/// Writes a formatted error response in JSON or plain text.
fn respond_error(
    res: &mut Response,
    status: u16,
    message: &str,
    json_body: serde_json::Value,
    json: bool,
) {
    if json {
        res.header(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        res.json(&json_body).ok();
    } else {
        res.header(
            CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        res.text(message);
    }

    res.status_code(status).ok();
}

// pub struct MiddlewareChain;

// impl MiddlewareChain {
//     pub fn new() -> Vec<Box<dyn Middleware + Send + Sync>> {
//         vec![
//             Box::new(LoggingMiddleware),
//             Box::new(SecurityHeadersMiddleware),
//             Box::new(CorsMiddleware::default()),
//             Box::new(RateLimitMiddleware::default()),
//             Box::new(BodySizeLimitMiddleware::default()),
//             Box::new(CookieAuthMiddleware::default()),
//         ]
//     }

//     pub fn basic() -> Vec<Box<dyn Middleware + Send + Sync>> {
//         vec![
//             Box::new(LoggingMiddleware),
//             Box::new(SecurityHeadersMiddleware),
//             Box::new(CorsMiddleware::default()),
//         ]
//     }
// }
