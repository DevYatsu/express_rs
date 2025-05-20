use std::fmt;
use std::sync::Arc;

mod next;
pub mod request;
pub mod response;

pub use next::Next;
pub use request::Request;
pub use response::Response;

/// A boxed handler function type with mutable request and response.
pub(crate) type BoxedHandlerFn = dyn Fn(&mut Request, &mut Response, Next) + Send + Sync;

/// The core handler abstraction in express_rs.
#[derive(Clone)]
pub struct Handler(Arc<BoxedHandlerFn>);

impl Handler {
    /// Creates a new handler from a closure or function.
    #[inline(always)]
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&mut Request, &mut Response, Next) + Send + Sync + 'static,
    {
        Self(Arc::new(f))
    }

    /// Calls the handler function.
    #[inline(always)]
    pub fn call(&self, req: &mut Request, res: &mut Response, next: Next) {
        (self.0)(req, res, next)
    }
}

impl<F> From<F> for Handler
where
    F: Fn(&mut Request, &mut Response, Next) + Send + Sync + 'static,
{
    #[inline(always)]
    fn from(f: F) -> Self {
        Self::new(f)
    }
}

impl Default for Handler {
    #[inline(always)]
    fn default() -> Self {
        Self::new(|_, _, _| {})
    }
}

impl fmt::Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Handler { ... }")
    }
}
