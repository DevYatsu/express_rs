use crate::handler::{Handler, Next, Request, Response};
use layer::Layer;
use matchit::Router as MatchitRouter;
use route::Route;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

mod layer;
mod middleware;
mod route;

pub use middleware::Middleware;

#[derive(Debug, Clone, Default)]
pub struct Router {
    stack: Vec<Layer>,
    pub route_matcher: MatchitRouter<Vec<usize>>,
    pub middleware_matcher: MatchitRouter<Vec<usize>>,
}

impl Router {
    pub fn route(&mut self, path: impl AsRef<str>, handle: impl Into<Handler>) -> &mut Route {
        if let Ok(entry) = self.route_matcher.at_mut(path.as_ref()) {
            entry.value.push(self.stack.len());
        } else {
            self.route_matcher
                .insert(path.as_ref(), vec![self.stack.len()])
                .unwrap();
        }

        let route = Route::new(path.as_ref());
        let mut layer = Layer::new(path.as_ref(), handle);
        *layer.route_mut() = Some(route.clone());
        self.stack.push(layer);
        self.stack.last_mut().unwrap().route_mut().as_mut().unwrap()
    }

    pub fn use_with<M: Middleware>(&mut self, middleware: M) -> &mut Self {
        let path = &middleware.target_path().into();
        if let Ok(entry) = self.middleware_matcher.at_mut(path) {
            entry.value.push(self.stack.len());
        } else {
            self.middleware_matcher
                .insert(path, vec![self.stack.len()])
                .unwrap();
        }

        let mut layer = Layer::new(middleware.target_path(), middleware.create_handler());
        layer.kind = M::layer_kind();
        self.stack.push(layer);

        return self;
    }

    pub fn handle(&self, req: &Request, res: &mut Response) {
        // gather matched middlewares
        let mut matched = self
            .middleware_matcher
            .at(req.uri().path())
            .map(|m| m.value.iter().copied().collect::<Vec<_>>())
            .unwrap_or_default();

        // add matched routes
        if let Ok(route_matches) = self.route_matcher.at(req.uri().path()) {
            matched.extend(route_matches.value.iter().copied());
        }

        if matched.is_empty() {
            // No matching middleware -> fallback
            res.status_code(404).unwrap().send("Not Found");
            return;
        }

        let mut path_method_matched = false;

        for &i in &matched {
            let layer = &self.stack[i];

            // case for middleware
            if layer.route.is_none() {
                let (called_next, next_signal) = Self::create_next();
                layer.handle_request(&req, res, next_signal);

                // If `next` was not called, stop processing
                if !called_next.load(Ordering::Relaxed) {
                    return;
                }

                continue;
            }

            let route = layer.route.as_ref().unwrap();

            if !route.methods.contains(req.method()) || route.stack.is_empty() {
                continue;
            }

            path_method_matched = true;

            for route_layer in &route.stack {
                let (called_next, next_signal) = Self::create_next();

                route_layer.handle_request(&req, res, next_signal);

                if !called_next.load(Ordering::Relaxed) {
                    return;
                }
            }
        }

        // matching path but not method
        if !path_method_matched {
            res.status_code(405).unwrap().send("Method Not Allowed");
            return;
        }
    }

    fn create_next() -> (Arc<AtomicBool>, Next) {
        let called_next = Arc::new(AtomicBool::new(false));
        let next_signal = Box::new({
            let called_next = Arc::clone(&called_next);
            move || {
                called_next.store(true, Ordering::Relaxed);
            }
        });

        (called_next, next_signal)
    }
}
