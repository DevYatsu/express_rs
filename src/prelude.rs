//! One-liner imports for `expressjs`.
//!
//! Add the following to the top of your file to bring the most commonly used
//! types, traits, and macros into scope:
//!
//! ```rust
//! use expressjs::prelude::*;
//! ```
//!
//! This covers:
//! - [`App`] and the [`express`] / [`app`](crate::app) factory functions
//! - [`Request`] and [`Response`] builder API ([`ExpressResponse`], [`RequestExt`])
//! - All built-in middleware types and the [`Middleware`] trait
//! - [`Router`], [`MethodKind`]
//! - [`StatusCode`] from `hyper`
//! - The [`async_trait`] attribute macro (for custom middleware impls)
//! - The [`Serialize`] / [`Deserialize`] derive macros from `serde`

pub use crate::application::App;
pub use crate::express;
pub use crate::handler::request::{Locals, RequestExt};
pub use crate::handler::response::{ExpressResponse, ResponseError};
pub use crate::handler::{Handler, Request, Response};
pub use crate::middleware::{
    AuthMiddleware, CacheMiddleware, CorsMiddleware, LoggingMiddleware, Middleware,
    MiddlewareResult, NormalizePathMiddleware, RateLimitMiddleware, SecurityHeadersMiddleware,
    StaticServeMiddleware, next_res, stop_res,
};
pub use crate::router::{MethodKind, Router};

// Proc-macros and common derives — re-exported so users need zero extra deps.
pub use crate::async_trait;
pub use crate::{Deserialize, Serialize};
pub use hyper::StatusCode;
