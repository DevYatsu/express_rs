#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

mod application;
mod handler;
mod middleware;
mod router;
mod server;

pub mod prelude;

// ─── Primary entry-points ─────────────────────────────────────────────────────

/// Creates a new Express-style application instance.
///
/// This is the main entry-point for the framework, equivalent to calling
/// `express()` in Node.js.
///
/// # Example
///
/// ```rust,no_run
/// use expressjs::prelude::*;
///
/// #[expressjs::main]
/// async fn main() {
///     let mut app = express();
///     app.get("/", async |_req, res| res.send_text("Hello, world!"));
///     app.listen(3000, async |port| println!("Listening on port {port}")).await;
/// }
/// ```
pub use middleware::app;

/// Alias for [`app`] — mirrors the `express()` call familiar from Express.js.
///
/// # Example
///
/// ```rust,no_run
/// use expressjs::prelude::*;
///
/// #[expressjs::main]
/// async fn main() {
///     let mut app = express();
///     app.get("/", async |_req, res| res.send_text("Hello, world!"));
///     app.listen(3000, async |port| println!("Listening on port {port}")).await;
/// }
/// ```
pub use middleware::app as express;

// Proc-macro re-exports

/// Marks an `async fn main` as the Tokio async runtime entry-point.
///
/// This is a direct re-export of [`tokio::main`] so that users do **not** need
/// to add `tokio` as a direct dependency in their `Cargo.toml`.
///
/// # Example
///
/// ```rust,no_run
/// #[expressjs::main]
/// async fn main() {
///     let mut app = expressjs::express();
///     app.listen(3000, async |p| println!("Listening on {p}")).await;
/// }
/// ```
pub use tokio::main;

/// Derive-able attribute for implementing the [`Middleware`](prelude::Middleware)
/// trait on async functions.
///
/// This is a direct re-export of [`async_trait::async_trait`] so that users
/// implementing custom middleware do **not** need to list `async-trait` as a
/// direct dependency.
///
/// # Example
///
/// ```rust,no_run
/// use expressjs::prelude::*;
/// use expressjs::async_trait;
///
/// #[derive(Clone)]
/// struct MyMiddleware;
///
/// #[async_trait]
/// impl<B: Send + Sync + 'static> Middleware<B> for MyMiddleware {
///     async fn call(&self, req: &mut Request<B>, res: &mut Response) -> MiddlewareResult {
///         // … your logic …
///         next_res()
///     }
/// }
/// ```
pub use async_trait::async_trait;

/// Derive macro for `serde::Serialize`.
///
/// Re-exported so users do **not** need to add `serde` as a direct dependency
/// when working with JSON request/response bodies.
///
/// # Example
///
/// ```rust
/// use expressjs::Serialize;
///
/// #[derive(Serialize)]
/// struct User { name: String }
/// ```
pub use serde::Serialize;

/// Derive macro for `serde::Deserialize`.
///
/// Re-exported so users do **not** need to add `serde` as a direct dependency
/// when working with JSON request/response bodies.
///
/// # Example
///
/// ```rust
/// use expressjs::Deserialize;
///
/// #[derive(Deserialize)]
/// struct CreateUser { name: String }
/// ```
pub use serde::Deserialize;

// ─── Helper macro ─────────────────────────────────────────────────────────────

/// Declares a lazily-initialized, globally-shared application state.
///
/// Uses [`once_cell::sync::Lazy`] internally for thread-safe, one-time
/// initialization with minimal overhead. The value is initialized with
/// [`Default::default()`] the first time it is accessed.
///
/// # Example
///
/// ```rust
/// use expressjs::express_state;
/// use std::sync::atomic::{AtomicU32, Ordering};
///
/// #[derive(Debug, Default)]
/// struct AppState {
///     request_count: AtomicU32,
/// }
///
/// express_state!(STATE, AppState);
///
/// // Then inside a handler:
/// // let count = STATE.request_count.fetch_add(1, Ordering::Relaxed);
/// ```
#[macro_export]
macro_rules! express_state {
    ($name:ident, $type:ty) => {
        static $name: $crate::__private::Lazy<$type> =
            $crate::__private::Lazy::new(<$type>::default);
    };
}

// ─── Internal / macro support ─────────────────────────────────────────────────

/// Private re-exports used internally by the [`express_state!`] macro.
///
/// **Do not use this module directly.** It is an implementation detail and
/// may change without notice.
#[doc(hidden)]
pub mod __private {
    pub use once_cell::sync::Lazy;
}
