use core::fmt;
use std::sync::Arc;

mod next;
mod request;
mod response;

pub use next::Next;
pub use request::Request;
pub use response::Response;

pub type BoxedHandlerFn = dyn Fn(&Request, &mut Response, Next) + Send + Sync;

pub struct Handler {
    inner: Arc<BoxedHandlerFn>,
}

impl Handler {
    pub fn new(f: impl Fn(&Request, &mut Response, Next) + Send + Sync + 'static) -> Self {
        Self { inner: Arc::new(f) }
    }

    pub fn inner(&self) -> &(dyn Fn(&Request, &mut Response, Next) + Send + Sync) {
        &*self.inner
    }
}

impl<F> From<F> for Handler
where
    F: Fn(&Request, &mut Response, Next) + Send + Sync + 'static,
{
    fn from(value: F) -> Self {
        Handler::new(value)
    }
}

impl Default for Handler {
    fn default() -> Self {
        Self {
            inner: Arc::new(|_, _, _| {}),
        }
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
