use std::{fmt::Debug, sync::Arc};

use super::{method_flag::MethodKind, route::Route};
use crate::handler::{Handler, Next, Request, Response};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LayerKind {
    Middleware,
    #[default]
    Route,
    ExpressInit,
}

#[derive(Clone)]
pub struct Layer {
    pub method: Option<MethodKind>,
    pub handler: Arc<dyn Handler>,
    pub route: Option<Route>,
    pub kind: LayerKind,
    path: String,
}

impl Debug for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layer")
            .field("method", &self.method)
            .field("handler", &"Handler")
            .field("route", &self.route)
            .field("kind", &self.kind)
            .field("path", &self.path)
            .finish()
    }
}


impl Layer {
    pub fn new(path: impl Into<String>, handler: impl Handler) -> Self {
        Self {
            handler: Arc::new(handler),
            method: None,
            route: None,
            path: path.into(),
            kind: LayerKind::default(),
        }
    }

    pub fn match_path(&self, path: impl Into<String>) -> bool {
        let path = path.into();

        if let Some(route) = &self.route {
            route.path == path
        } else if (self.path == "*" && self.kind == LayerKind::Middleware)
            || (self.path == path && self.kind == LayerKind::Middleware)
        {
            true
        } else {
            false
        }
    }

    pub fn route_mut(&mut self) -> &mut Option<Route> {
        &mut self.route
    }

    pub async fn handle_request(&self, req: &mut Request, res: &mut Response, next: Next) {
        self.handler.handle(req, res, next).await
    }
}
