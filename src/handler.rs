use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;

mod next;
pub mod request;
pub mod response;

use futures_core::future::BoxFuture;
pub use next::Next;
pub use request::Request;
pub use response::Response;

pub trait Handler: Send + Sync + 'static {
    fn handle<'a >(
        &self,
        req: &'a mut Request,
        res: &'a mut Response,
        next: Next,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

// HandlerFn wrapper for closures
#[derive(Clone)]
pub struct HandlerFn {
    pub f: Arc<dyn Fn(&mut Request, &mut Response, Next) -> BoxFuture<'static, ()>> ,
}

impl Handler for HandlerFn {
    fn handle(
        &self,
        req: &mut Request,
        res: &mut Response,
        next: Next,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        (self.f)(req, res, next)
    }
}

pub fn from_async_handler<F, Fut>(func: F) -> HandlerFn
where
    F: Fn(&mut Request, &mut Response, Next) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    HandlerFn {
        f: Arc::new(move |req, res, next| Box::pin(func(req, res, next))),
    }
}

