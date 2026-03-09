mod application;
pub mod handler;
pub mod middleware;
pub mod router;
mod server;

pub mod prelude;

/// Helper macro to initialize a high-performance global state.
///
/// This uses `once_cell::sync::Lazy` internally for thread-safe, 
/// one-time initialization with minimal overhead.
#[macro_export]
macro_rules! express_state {
    ($name:ident, $type:ty) => {
        static $name: $crate::reexports::Lazy<$type> = $crate::reexports::Lazy::new(<$type>::default);
    };
}

pub mod reexports {
    pub use once_cell::sync::Lazy;
}

pub use middleware::app as express;
