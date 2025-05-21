use self::interner::INTERNER;
use crate::handler::{Handler, Next, Request, Response, request::RequestExtInternal};
use ahash::{HashMap, HashMapExt};
use hyper::Method;
use layer::Layer;
use method_flag::MethodKind;
use route::Route;
use smallvec::{SmallVec, smallvec};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub mod interner;
mod layer;
mod method_flag;
mod middleware;
mod route;

pub use middleware::Middleware;

type LayerIndices = SmallVec<[usize; 8]>;
type MethodRoutes = HashMap<MethodKind, matchthem::Router<LayerIndices>>;

/// The core routing engine for `express_rs`.
///
/// `Router` is responsible for registering route and middleware handlers,
/// and efficiently dispatching them based on request paths and HTTP methods.
#[derive(Debug, Clone, Default)]
pub struct Router {
    /// All layers (routes and middleware), stored in order of registration.
    ///
    /// Each `Layer` contains metadata and a handler function.
    stack: Vec<Layer>,

    /// Path-based matcher for global middleware.
    ///
    /// Middleware is matched and executed before route-specific handlers.
    pub middleware_matcher: matchthem::Router<LayerIndices>,

    /// HTTP method-specific route matchers.
    ///
    /// For example, `Method::GET` maps to its own matcher tree.
    pub routes: MethodRoutes,
}

impl Router {
    pub fn route(
        &mut self,
        path: impl AsRef<str>,
        handle: impl Into<Handler>,
        method: &Method,
    ) -> &mut Route {
        let path_ref = path.as_ref();
        let layer_index = self.stack.len();

        let method_flag = MethodKind::from_hyper(method);

        let method_routes = self.routes.entry(method_flag).or_default();

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
        *layer.route_mut() = Some(route);
        self.stack.push(layer);

        self.stack.last_mut().unwrap().route_mut().as_mut().unwrap()
    }

    pub fn use_with<M: Middleware>(&mut self, middleware: M) -> &mut Self {
        let path = middleware.target_path();
        let path: &str = path.as_ref();
        let layer_index = self.stack.len();

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

        let mut layer = Layer::new(path, middleware.create_handler());
        layer.kind = M::layer_kind();
        self.stack.push(layer);

        self
    }

    pub async fn handle<'a>(&'a self, req: &'a mut Request, res: &'a mut Response) -> () {
        let path = req.uri().path();
        let method = &MethodKind::from_hyper(req.method());

        // Gather middleware handlers
        let mut matched = self
            .middleware_matcher
            .all_matches(path)
            .into_iter()
            .flat_map(|m| m.value)
            .collect::<SmallVec<[&usize; 8]>>();

        let mut path_exists = false;

        let router = match self.routes.get(method) {
            Some(router) => router,
            None => {
                req.set_params(Default::default());
                if matched.is_empty() {
                    res.status_code(404).unwrap().send("Not Found");
                } else {
                    res.status_code(405).unwrap().send("Method Not Allowed");
                }
                return ();
            }
        };

        if let Ok(route_match) = router.at(path) {
            path_exists = true;

            let mut params = HashMap::with_capacity(route_match.params.len());

            if !route_match.params.is_empty() {
                for (k, v) in route_match.params.iter() {
                    let sym_k = INTERNER.get_or_intern(k);
                    let sym_v = INTERNER.get_or_intern(v);
                    params.insert(sym_k, sym_v);
                }
            }

            let value = route_match.value;
            req.set_params(params);
            matched.extend(value);
        } else {
            req.set_params(Default::default());
        }

        if matched.is_empty() {
            let status = if path_exists { 405 } else { 404 };
            res.status_code(status).unwrap().send(match status {
                404 => "Not Found",
                _ => "Method Not Allowed",
            });
            return ();
        }

        // sort and dedup
        matched.sort_unstable();
        matched.dedup();

        let mut path_method_matched = false;

        for i in matched {
            let layer = &self.stack[*i];

            if layer.route.is_none() {
                let (called_next, next_signal) = Self::create_next();
                layer.handle_request(req, res, next_signal).await;

                if !called_next.load(Ordering::Relaxed) {
                    return;
                }
                continue;
            }

            let route = layer.route.as_ref().unwrap();

            if route.stack.is_empty() || !route.methods.iter().any(|m| m == method) {
                continue;
            }

            path_method_matched = true;

            for route_layer in &route.stack {
                let (called_next, next_signal) = Self::create_next();
                route_layer.handle_request(req, res, next_signal).await;

                if !called_next.load(Ordering::Relaxed) {
                    return;
                }
            }
        }

        if !path_method_matched {
            res.status_code(405).unwrap().send("Method Not Allowed");
        }

        return;
    }

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
