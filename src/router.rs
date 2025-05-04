use layer::Layer;
use route::Route;
use std::path::Path;

use crate::{
    express::Next,
    handler::{Handler, Request, Response},
};

mod layer;
mod route;

#[derive(Debug, Clone, Default)]
pub struct Router {
    stack: Vec<Layer>,
}

impl Router {
    pub fn route(&mut self, path: impl AsRef<Path>, handle: impl Into<Handler>) -> &mut Route {
        let route = Route::new(&path);
        let mut layer = Layer::new(&path, handle);
        *layer.route_mut() = Some(route.clone());

        self.stack.push(layer);
        let last = self.stack.len() - 1;

        self.stack[last].route_mut().as_mut().unwrap()
    }

    pub fn handle(&self, req: Request, res: &mut Response, next: Next) {
        self.stack[0].handle_request(req, res, next);
    }
}
