use core::fmt;
use std::sync::Arc;

mod next;
mod request;
pub mod response;

pub use next::Next;
pub use request::Request;
pub use response::Response;

pub type BoxedHandlerFn = dyn Fn(&Request, &mut Response, Next) + Send + Sync;

pub struct Handler {
    inner: Arc<BoxedHandlerFn>,
}

impl Handler {
    pub fn new<F>(f: F) -> Self
    where
        F: Fn(&Request, &mut Response, Next) + Send + Sync + 'static,
    {
        Self { inner: Arc::new(f) }
    }

    pub fn call(&self, req: &Request, res: &mut Response, next: Next) {
        (self.inner)(req, res, next)
    }
}

impl<F> From<F> for Handler
where
    F: Fn(&Request, &mut Response, Next) + Send + Sync + 'static,
{
    fn from(f: F) -> Self {
        Handler { inner: Arc::new(f) }
    }
}

impl Default for Handler {
    fn default() -> Self {
        (|_: &Request, _: &mut Response, _: Next| {}).into()
    }
}

impl Clone for Handler {
    fn clone(&self) -> Self {
        Handler {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl fmt::Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Handler { ... }")
    }
}
