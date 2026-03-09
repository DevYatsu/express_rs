#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

mod application;
/// Handlers and utilities for incoming HTTP requests and responses.
pub mod handler;
/// Middleware traits and implementations for request processing.
pub mod middleware;
/// Routing logic, method dispatchers, and route definitions.
pub mod router;
mod server;

/// Re-exported items necessary for expressjs functionality.
pub mod prelude;

/// Helper macro to initialize a high-performance global state.
///
/// This uses `once_cell::sync::Lazy` internally for thread-safe,
/// one-time initialization with minimal overhead.
#[macro_export]
macro_rules! express_state {
    ($name:ident, $type:ty) => {
        static $name: $crate::reexports::Lazy<$type> =
            $crate::reexports::Lazy::new(<$type>::default);
    };
}

/// Helper re-exports used by the macro.
pub mod reexports {
    pub use once_cell::sync::Lazy;
}

pub use middleware::app as express;
