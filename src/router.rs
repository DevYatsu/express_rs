use crate::handler::{Handler, Next, Request, Response, request::RequestExtInternal};
use ahash::HashMap;
use hyper::Method;
use layer::Layer;
use matchthem::Router as MatchitRouter;
use route::Route;
use smallvec::SmallVec;
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
    pub middleware_matcher: MatchitRouter<SmallVec<[usize; 8]>>,
    pub routes: HashMap<Method, MatchitRouter<Vec<usize>>>,
}

impl Router {
    /// Creates a new route at the specified path with the given handler
    pub fn route(
        &mut self,
        path: impl AsRef<str>,
        handle: impl Into<Handler>,
        method: Method,
    ) -> &mut Route {
        let path_ref = path.as_ref();
        let layer_index = self.stack.len();

        let method_routes = self.routes.entry(method.clone()).or_default();

        // Insert the new route index into the matcher
        match method_routes.at_mut(path_ref) {
            Ok(entry) => entry.value.push(layer_index),
            Err(_) => {
                method_routes
                    .insert(path_ref, vec![layer_index])
                    .expect("Failed to insert route");
            }
        }

        let route = Route::new(path_ref);
        let mut layer = Layer::new(path_ref, handle);
        *layer.route_mut() = Some(route.clone());
        self.stack.push(layer);

        // Return mutable reference to the new route
        self.stack.last_mut().unwrap().route_mut().as_mut().unwrap()
    }

    /// Adds middleware to the router
    pub fn use_with<M: Middleware>(&mut self, middleware: M) -> &mut Self {
        let path = middleware.target_path().into();
        let layer_index = self.stack.len();

        // Insert the middleware index into the matcher
        match self.middleware_matcher.at_mut(&path) {
            Ok(entry) => entry.value.push(layer_index),
            Err(_) => {
                let mut indices = SmallVec::with_capacity(2);
                indices.push(layer_index);
                self.middleware_matcher
                    .insert(path.to_owned(), indices)
                    .expect("Failed to insert middleware");
            }
        }

        // Create and add the new middleware layer
        let mut layer = Layer::new(path, middleware.create_handler());
        layer.kind = M::layer_kind();
        self.stack.push(layer);

        self
    }

    /// Handles a request by matching middleware and route handlers, then executing them.
    pub fn handle(&self, req: &mut Request, res: &mut Response) {
        let path = req.uri().path();
        let method = req.method();

        // Collect all middleware matches (including overlapping ones)
        let mut matched = self
            .middleware_matcher
            .all_matches(path)
            .into_iter()
            .flat_map(|m| m.value)
            .collect::<SmallVec<[&usize; 8]>>();

        // Try to match a route and extract params
        if let Some(routes) = self.routes.get(method) {
            if let Ok(route_match) = routes.at(path) {
                let params = route_match
                    .params
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let value = route_match.value;

                req.set_params(params);
                matched.extend(value);
            } else {
                req.set_params(Default::default());
            }
        } else {
            req.set_params(Default::default());
        }

        if matched.is_empty() {
            res.status_code(404).unwrap().send("Not Found");
            return;
        }

        // Deduplicate handler indices for execution
        matched.sort_unstable();
        matched.dedup();

        let mut path_method_matched = false;
        let req_method = req.method();

        for i in matched {
            let layer = &self.stack[*i];

            // Handle middleware (no associated route)
            if layer.route.is_none() {
                let (called_next, next_signal) = Self::create_next();
                layer.handle_request(req, res, next_signal);

                if !called_next.load(Ordering::Relaxed) {
                    return;
                }
                continue;
            }

            // Handle route
            let route = layer.route.as_ref().unwrap();

            if !route.methods.contains(req_method) || route.stack.is_empty() {
                continue;
            }

            path_method_matched = true;

            for route_layer in &route.stack {
                let (called_next, next_signal) = Self::create_next();
                route_layer.handle_request(req, res, next_signal);

                if !called_next.load(Ordering::Relaxed) {
                    return;
                }
            }
        }

        if !path_method_matched {
            res.status_code(405).unwrap().send("Method Not Allowed");
        }
    }

    /// Creates a next signal with a flag to track if it was called
    #[inline(always)]
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
