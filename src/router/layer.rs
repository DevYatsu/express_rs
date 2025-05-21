use super::{method_flag::MethodKind, route::Route};
use crate::handler::{Handler, Next, Request, Response};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LayerKind {
    Middleware,
    #[default]
    Route,
    ExpressInit,
}

#[derive(Debug, Clone, Default)]
pub struct Layer {
    pub method: Option<MethodKind>,
    pub handle: Handler,
    pub route: Option<Route>,
    pub kind: LayerKind,
    path: String,
}

impl Layer {
    pub fn new(path: impl Into<String>, handle: impl Into<Handler>) -> Self {
        Self {
            handle: handle.into(),
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
        self.handle.call(req, res, next).await
    }
}
