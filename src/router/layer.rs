use crate::{
    express::Next,
    handler::{Handler, Request, Response},
    methods::Method,
};
use std::path::{Path, PathBuf};

use super::route::Route;

#[derive(Debug, Clone, Default)]
pub struct Layer {
    pub method: Option<Method>,
    handle: Handler,
    pub route: Option<Route>,
    path: PathBuf,
}

impl Layer {
    pub fn new(path: impl AsRef<Path>, handle: impl Into<Handler>) -> Self {
        Self {
            handle: handle.into(),
            method: None,
            route: None,
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn route_mut(&mut self) -> &mut Option<Route> {
        &mut self.route
    }

    pub fn handle_request(&self, req: Request, res: &mut Response, next: Next) {
        let f = self.handle.inner();

        f(req, res, next);
    }
}
