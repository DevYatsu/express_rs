use super::route::Route;
use crate::handler::{Handler, Next, Request, Response};
use hyper::Method;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LayerKind {
    Middleware,
    #[default]
    Route,
    ExpressInit,
}

#[derive(Debug, Clone, Default)]
pub struct Layer {
    pub method: Option<Method>,
    pub handle: Handler,
    pub route: Option<Route>,
    pub kind: LayerKind,
    path: PathBuf,
}

impl Layer {
    pub fn new(path: impl AsRef<Path>, handle: impl Into<Handler>) -> Self {
        Self {
            handle: handle.into(),
            method: None,
            route: None,
            path: path.as_ref().to_path_buf(),
            kind: LayerKind::default(),
        }
    }

    pub fn match_path(&self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();

        if let Some(route) = &self.route {
            route.path == path
        } else if self.path == path && self.kind == LayerKind::Middleware {
            true
        } else {
            false
        }
    }

    pub fn route_mut(&mut self) -> &mut Option<Route> {
        &mut self.route
    }

    pub fn handle_request(&self, req: &Request, res: &mut Response, next: Next) {
        let f = self.handle.inner();

        f(req, res, next);
    }
}
