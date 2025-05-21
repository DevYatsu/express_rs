use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

mod next;
pub mod request;
pub mod response;

use futures_util::FutureExt;
pub use next::Next;
pub use request::Request;
pub use response::Response;

pub type HandlerResult<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Trait for async handler abstraction.
pub trait Handler: Send + Sync + 'static {
    fn call<'a>(
        &'a self,
        req: &'a mut Request,
        res: &'a mut Response,
        next: Next,
    ) -> HandlerResult<'a> {
        async {}.boxed()
    }
}

/// Handler struct that wraps any compatible async function or closure.
#[derive(Clone)]
pub struct FnHandler(Arc<dyn Handler>);

impl FnHandler {
    #[inline(always)]
    pub fn new<H>(handler: H) -> Self
    where
        H: Handler,
    {
        Self(Arc::new(handler))
    }

    #[inline(always)]
    pub fn call<'a>(
        &'a self,
        req: &'a mut Request,
        res: &'a mut Response,
        next: Next,
    ) -> HandlerResult<'a> {
        self.0.call(req, res, next)
    }
}

impl<H> From<H> for FnHandler
where
    H: Handler,
{
    #[inline(always)]
    fn from(handler: H) -> Self {
        Self::new(handler)
    }
}

impl fmt::Debug for FnHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Handler { ... }")
    }
}

/// Blanket impl for closures or functions that match the async signature.
impl<F, Fut> Handler for F
where
    F: Fn(&mut Request, &mut Response, Next) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn call<'a>(
        &'a self,
        req: &'a mut Request,
        res: &'a mut Response,
        next: Next,
    ) -> HandlerResult<'a> {
        (self)(req, res, next).boxed()
    }
}
