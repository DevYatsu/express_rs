use super::method::MethodKind;
use crate::handler::Handler;
use crate::middleware::Middleware;
use hyper::body::Incoming;
use std::{fmt::Debug, sync::Arc};

/// Represents a single routing or middleware layer in the application.
pub struct Layer<B = Incoming> {
    pub path: Arc<str>,
    pub method: Option<MethodKind>,
    pub middlewares: Vec<Arc<dyn Middleware<B>>>,
    pub handler: Option<Arc<dyn Handler<B>>>,
}

impl<B: Send + 'static> Layer<B> {
    pub fn route(
        path: Arc<str>,
        method: MethodKind,
        middlewares: Vec<Arc<dyn Middleware<B>>>,
        handler: Arc<dyn Handler<B>>,
    ) -> Self {
        Self {
            path,
            method: Some(method),
            middlewares,
            handler: Some(handler),
        }
    }

    pub fn middleware(path: Arc<str>, middlewares: Vec<Arc<dyn Middleware<B>>>) -> Self {
        Self {
            path,
            method: None,
            middlewares,
            handler: None,
        }
    }
}

impl<B> Debug for Layer<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layer")
            .field("path", &self.path)
            .field("method", &self.method)
            .field("middlewares_count", &self.middlewares.len())
            .field("has_handler", &self.handler.is_some())
            .finish()
    }
}
