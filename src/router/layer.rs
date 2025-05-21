use super::method::MethodKind;
use crate::handler::{Handler, Next, Request, Response};
use std::{fmt::Debug, sync::Arc};

/// Represents a single routing or middleware layer in the application.
/// Similar to an Express.js layer, it holds a path matcher, an optional HTTP method,
/// a handler, and optional route metadata.
pub enum Layer {
    /// A middleware layer that intercepts and processes requests.
    Middleware {
        /// The path prefix that this layer matches against.
        path: String,
        /// The handler that will be invoked if this layer matches.
        handler: Arc<dyn Handler>,
    },

    /// A route layer that matches a method and path to a specific handler.
    Route {
        /// The path prefix that this layer matches against.
        path: String,
        /// The HTTP method this layer responds to (None for middleware).
        method: MethodKind,
        /// The handler that will be invoked if this layer matches.
        handler: Arc<dyn Handler>,
    },
}

impl Layer {
    pub fn route(path: impl AsRef<str>, method: MethodKind, handle: impl Handler) -> Self {
        Self::Route {
            path: path.as_ref().into(),
            method,
            handler: Arc::new(handle),
        }
    }

    pub fn middleware(path: impl AsRef<str>, handle: impl Handler) -> Self {
        Self::Middleware {
            path: path.as_ref().into(),
            handler: Arc::new(handle),
        }
    }

    pub fn path(&self) -> &str {
        match self {
            Layer::Middleware { path, .. } => path,
            Layer::Route { path, .. } => path,
        }
    }

    fn handler(&self) -> &Arc<dyn Handler> {
        match self {
            Layer::Middleware { handler, .. } => handler,
            Layer::Route { handler, .. } => handler,
        }
    }

    pub async fn handle_request(&self, req: &mut Request, res: &mut Response, next: Next) {
        self.handler().call(req, res, next).await
    }
}

impl Debug for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Layer::Middleware { path, .. } => {
                f.debug_struct("Middleware").field("path", path).finish()
            }
            Layer::Route { path, method, .. } => f
                .debug_struct("Route")
                .field("path", path)
                .field("method", method)
                .finish(),
        }
    }
}
