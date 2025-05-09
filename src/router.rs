use crate::handler::{Handler, Next, Request, Response, request::RequestExtInternal};
use ahash::{HashMap, HashMapExt};
use hyper::Method;
use layer::Layer;
use route::Route;
use smallvec::{SmallVec, smallvec};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

mod layer;
mod middleware;
mod route;

pub use middleware::Middleware;

// Fast small collection for handlers
type LayerIndices = SmallVec<[usize; 8]>;
// Fast map for method -> route trie
type MethodRoutes = HashMap<Method, matchthem::Router<LayerIndices>>;

#[derive(Debug, Clone, Default)]
pub struct Router {
    /// All registered layers (routes and middleware)
    stack: Vec<Layer>,
    /// Middleware matcher using path trie
    pub middleware_matcher: matchthem::Router<LayerIndices>,
    /// Method-specific route matchers, e.g., GET → router for GET routes
    pub routes: MethodRoutes,
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
                    .insert(path_ref, smallvec![layer_index])
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
        let path = middleware.target_path();
        let path: &str = path.as_ref();
        let layer_index = self.stack.len();

        // Insert the middleware index into the matcher
        match self.middleware_matcher.at_mut(path) {
            Ok(entry) => entry.value.push(layer_index),
            Err(_) => {
                let mut indices = SmallVec::with_capacity(2);
                indices.push(layer_index);
                self.middleware_matcher
                    .insert(path, indices)
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
                // todo! replace with a string interner in the future
                // Pre-allocate a hashmap for route parameters with known capacity
                let mut params = HashMap::with_capacity(route_match.params.len());

                // Convert param keys and values to `Arc<str>` for shared, cheap-to-clone ownership
                for (k, v) in route_match.params.iter() {
                    params.insert(Arc::<str>::from(k), Arc::<str>::from(v));
                }

                // Store parsed route parameters into the request for downstream access
                let value = route_match.value;
                req.set_params(params);

                // Extend the matched handler list with all handler indices for this route
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
        matched.dedup();
        matched.sort_unstable();

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

            if route.stack.is_empty() || !route.methods.iter().any(|m| m == req_method) {
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
