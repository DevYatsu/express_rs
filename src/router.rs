use self::interner::INTERNER;
use crate::{
    handler::{ExpressResponse, Handler, Request, Response, request::RequestMetadataInternal},
    prelude::Middleware,
};
use hyper::body::Incoming;
use layer::Layer;
use rustc_hash::FxHashMap;
use smallvec::{SmallVec, smallvec};
use std::sync::Arc;

/// Tools for interning symbols used heavily throughout routing.
pub mod interner;
mod layer;
mod method;

pub use method::MethodKind;

/// Total number of HTTP methods tracked.
const METHOD_COUNT: usize = 9;

/// A list of layer indices representing handlers resolving to a method.
pub type LayerIndices = SmallVec<[usize; 8]>;

/// Internal router for a specific HTTP method.
#[derive(Debug, Default)]
pub struct MethodRouter {
    /// Underlying matchit router used to match path variables.
    pub matcher: matchit::Router<usize>,
    /// Lists of layers matching the specific path indices.
    pub indices: Vec<LayerIndices>,
    /// Map from string paths to their indices in the `matcher`.
    pub path_to_idx: FxHashMap<Arc<str>, usize>,
}

/// Fixed-size array of per-method routers, indexed by `MethodKind as usize`.
/// Replaces `HashMap<MethodKind, MethodRouter>` for O(1) dispatch.
#[derive(Default)]
pub struct MethodRoutes([Option<MethodRouter>; METHOD_COUNT]);

impl MethodRoutes {
    /// Gets the router for a given standard method.
    #[inline]
    pub fn get(&self, method: MethodKind) -> Option<&MethodRouter> {
        self.0[method as usize].as_ref()
    }

    /// Gets the mutable router for a given standard method.
    #[inline]
    pub fn get_mut(&mut self, method: MethodKind) -> Option<&mut MethodRouter> {
        self.0[method as usize].as_mut()
    }

    /// Gets or creates the router for a given standard method.
    #[inline]
    pub fn entry_or_default(&mut self, method: MethodKind) -> &mut MethodRouter {
        self.0[method as usize].get_or_insert_with(MethodRouter::default)
    }

    /// Iterate over all (MethodKind, &MethodRouter) pairs that are present.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (MethodKind, &MethodRouter)> {
        self.0
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| slot.as_ref().map(|r| (MethodKind::from_index(i), r)))
    }

    /// Check if the method is present.
    pub fn contains_method(&self, method: MethodKind) -> bool {
        self.0[method as usize].is_some()
    }
}

impl MethodRouter {
    fn add_route(&mut self, path: &Arc<str>, layer_index: usize) {
        if let Some(&idx) = self.path_to_idx.get(path) {
            self.indices[idx].push(layer_index);
        } else {
            let idx = self.indices.len();
            self.indices.push(smallvec![layer_index]);
            self.path_to_idx.insert(Arc::clone(path), idx);
            self.matcher
                .insert(path.as_ref(), idx)
                .expect("Failed to insert route");
        }
    }
}

/// Component used to match midlewares to path prefixes.
#[derive(Debug)]
pub struct MiddlewareMatcher {
    /// Path prefix to match.
    pub path: Arc<str>,
    /// The matching engine.
    pub router: matchit::Router<()>,
    /// Indices of matching middleware layers.
    pub indices: LayerIndices,
}

/// The core routing engine for `expressjs`.
pub struct Router<B = Incoming> {
    /// The linear list of layers attached to the router.
    pub stack: Vec<Layer<B>>,
    /// Pre-compiled matchers for middleware lookup.
    pub middleware_matchers: Vec<MiddlewareMatcher>,
    /// Side-index: path string → position in `middleware_matchers`.
    /// Kept in sync so `use_with` and `use_router` run in O(1) for path dedup.
    middleware_path_index: FxHashMap<Arc<str>, usize>,
    /// Registered standard HTTP method route endpoints.
    pub routes: MethodRoutes,
    /// Fallback handler executed if no match is found.
    pub not_found_handler: Option<Arc<dyn Handler<B>>>,
}

impl<B> Default for Router<B> {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            middleware_matchers: Vec::new(),
            middleware_path_index: FxHashMap::default(),
            routes: MethodRoutes::default(),
            not_found_handler: None,
        }
    }
}

impl<B: Send + 'static> Router<B> {
    /// Attaches a custom handler to a specific path and HTTP method.
    pub fn route(
        &mut self,
        path: impl AsRef<str>,
        handler: impl Handler<B>,
        method: MethodKind,
    ) -> &mut Layer<B> {
        let mut p = path.as_ref();
        if p.len() > 1 && p.ends_with('/') {
            p = &p[..p.len() - 1];
        }
        let path: Arc<str> = p.into();
        let layer_index = self.stack.len();

        let method_routes = self.routes.entry_or_default(method);
        method_routes.add_route(&path, layer_index);

        let layer = Layer::route(Arc::clone(&path), method, vec![], Arc::new(handler));
        self.stack.push(layer);

        self.stack.last_mut().unwrap()
    }

    /// Registers a handler for all HTTP methods on the specified path.
    pub fn all<F, Fut>(&mut self, path: impl AsRef<str>, handler: F) -> &mut Self
    where
        F: Fn(Request<B>, Response) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        let path: Arc<str> = path.as_ref().into();
        for &method in &MethodKind::ALL {
            self.route(path.as_ref(), handler.clone(), method);
        }
        self
    }

    /// Creates a fluid route builder for a given path.
    pub fn route_builder(&mut self, path: impl AsRef<str>) -> Route<'_, B> {
        Route {
            router: self,
            path: path.as_ref().into(),
        }
    }

    /// Mounts a middleware function at the specified path prefix.
    pub fn use_with(&mut self, path: impl AsRef<str>, middleware: impl Middleware<B>) -> &mut Self {
        let mut p = path.as_ref();
        if p.len() > 1 && p.ends_with('/') {
            p = &p[..p.len() - 1];
        }
        let path: Arc<str> = p.into();
        let layer_index = self.stack.len();

        // O(1) lookup using the side-index instead of a linear scan.
        if let Some(&idx) = self.middleware_path_index.get(&path) {
            self.middleware_matchers[idx].indices.push(layer_index);
        } else {
            let mut router = matchit::Router::new();
            let p_str = path.as_ref();
            // Match the path itself exactly
            router.insert(p_str, ()).ok();

            // Express-style prefix matching: /path should match /path, /path/, and /path/sub
            // matchit 0.9+ requires the {*param} wildcard syntax (not the old /*param).
            if p_str == "/" {
                // Root: /{*path} matches every sub-path; exact "/" was already inserted above.
                router.insert("/{*path}", ()).ok();
            } else {
                let prefix_path = format!("{}/{{*path}}", p_str.trim_end_matches('/'));
                router.insert(prefix_path, ()).ok();
            }

            let new_idx = self.middleware_matchers.len();
            self.middleware_matchers.push(MiddlewareMatcher {
                path: Arc::clone(&path),
                router,
                indices: smallvec![layer_index],
            });
            self.middleware_path_index
                .insert(Arc::clone(&path), new_idx);
        }

        let layer = Layer::middleware(Arc::clone(&path), vec![Arc::new(middleware)]);
        self.stack.push(layer);

        self
    }

    /// Sets a catch-all handler for 404 Not Found scenarios.
    pub fn not_found<F, Fut>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(Request<B>, Response) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        self.not_found_handler = Some(Arc::new(handler));
        self
    }

    /// Responds to an incoming HTTP request by dispatching to the configured handlers.
    pub async fn handle(&self, mut req: Request<B>, res: Response) -> Response {
        let raw_path = req.uri().path();
        let path = if raw_path.len() > 1 && raw_path.ends_with('/') {
            &raw_path[..raw_path.len() - 1]
        } else {
            raw_path
        };
        let method = MethodKind::from_hyper(req.method());

        let mut matched = SmallVec::<[usize; 8]>::new();
        let mut route_params = SmallVec::<[(interner::Symbol, Arc<str>); 4]>::new();

        for matcher in &self.middleware_matchers {
            if let Ok(matched_route) = matcher.router.at(path) {
                for (k, v) in matched_route.params.iter() {
                    let sym_k = INTERNER.get_or_intern(k);
                    route_params.push((sym_k, v.into()));
                }
                matched.extend(matcher.indices.iter().copied());
            }
        }

        let mut path_exists = false;

        if let Some(method_routes) = self.routes.get(method)
            && let Ok(route_match) = method_routes.matcher.at(path) {
                path_exists = true;

                if !route_match.params.is_empty() {
                    for (k, v) in route_match.params.iter() {
                        let sym_k = INTERNER.get_or_intern(k);
                        route_params.push((sym_k, v.into()));
                    }
                }

                matched.extend(method_routes.indices[*route_match.value].iter().copied());
            }

        if !path_exists {
            // Check if path exists under a different method (=> 405 vs 404).
            // O(methods) matchit lookups only on cache misses — acceptable.
            for (m, method_routes) in self.routes.iter() {
                if m != method && method_routes.matcher.at(path).is_ok() {
                    path_exists = true;
                    break;
                }
            }
        }

        if matched.is_empty() {
            let status = if path_exists { 405 } else { 404 };

            if status == 404
                && let Some(h) = &self.not_found_handler {
                    return h.call(req, res).await;
                }

            return res.status_code(status).send_text(match status {
                404 => "Not Found",
                _ => "Method Not Allowed",
            });
        }

        req.set_params(route_params);

        // Sort by index to maintain registration order across all layers.
        // This is necessary because middlewares and routes are collected separately.
        if matched.len() > 1 {
            matched.sort_unstable();
            matched.dedup();
        }

        let mut req_opt = Some(req);
        let mut res_opt = Some(res);

        for i in matched {
            let layer = &self.stack[i];

            if let Some(m) = &layer.method
                && *m != method {
                    continue;
                }

            for mw in &layer.middlewares {
                let req_mut = req_opt.as_mut().unwrap();
                let res_mut = res_opt.as_mut().unwrap();
                if mw.call(req_mut, res_mut).await.is_stop() {
                    // A middleware signalled Stop — halt the entire chain.
                    return res_opt.unwrap();
                }
            }

            if let Some(h) = &layer.handler {
                return h
                    .call(req_opt.take().unwrap(), res_opt.take().unwrap())
                    .await;
            }
        }

        let status = if path_exists { 405 } else { 404 };

        if status == 404
            && let Some(h) = &self.not_found_handler {
                return h
                    .call(req_opt.take().unwrap(), res_opt.take().unwrap())
                    .await;
            }

        res_opt
            .unwrap()
            .status_code(status)
            .send_text(match status {
                404 => "Not Found",
                _ => "Method Not Allowed",
            })
    }

    /// Mounts a child router into the current router at the given path prefix.
    pub fn use_router(&mut self, prefix: impl AsRef<str>, router: Router<B>) -> &mut Self {
        let prefix = prefix.as_ref().trim_end_matches('/');

        for layer in router.stack {
            let new_path: Arc<str> = if layer.path.as_ref() == "/" {
                prefix.into()
            } else if layer.path.starts_with('/') {
                format!("{}{}", prefix, layer.path.as_ref()).into()
            } else {
                format!("{}/{}", prefix, layer.path.as_ref()).into()
            };

            let layer_index = self.stack.len();

            if let Some(method) = layer.method {
                let method_routes = self.routes.entry_or_default(method);
                method_routes.add_route(&new_path, layer_index);
            } else {
                // O(1) lookup via side-index.
                if let Some(&idx) = self.middleware_path_index.get(&new_path) {
                    self.middleware_matchers[idx].indices.push(layer_index);
                } else {
                    let p = new_path.as_ref();
                    let mut r = matchit::Router::new();
                    r.insert(p, ()).expect("Failed to insert nested middleware");
                    // Include wildcard sub-path so mounted middleware also matches /prefix/sub/paths
                    // matchit 0.9+ requires the {*param} wildcard syntax.
                    if p == "/" {
                        r.insert("/{*path}", ()).ok();
                    } else {
                        let wildcard = format!("{}/{{*path}}", p);
                        r.insert(&wildcard, ()).ok();
                    }
                    let new_idx = self.middleware_matchers.len();
                    self.middleware_matchers.push(MiddlewareMatcher {
                        path: Arc::clone(&new_path),
                        router: r,
                        indices: smallvec![layer_index],
                    });
                    self.middleware_path_index
                        .insert(Arc::clone(&new_path), new_idx);
                }
            }

            self.stack.push(Layer {
                path: Arc::clone(&new_path),
                method: layer.method,
                middlewares: layer.middlewares,
                handler: layer.handler,
            });
        }

        self
    }
}

/// Helper macro that generates convenient fluid HTTP method builder routines on the Router syntax.
#[macro_export]
macro_rules! define_methods {
    () => {
        /// Registers a handler for GET requests.
        pub fn get<F, Fut>(&mut self, path: impl AsRef<str>, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(path, handler, $crate::router::MethodKind::Get)
        }
        /// Registers a handler for POST requests.
        pub fn post<F, Fut>(&mut self, path: impl AsRef<str>, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(path, handler, $crate::router::MethodKind::Post)
        }
        /// Registers a handler for PUT requests.
        pub fn put<F, Fut>(&mut self, path: impl AsRef<str>, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(path, handler, $crate::router::MethodKind::Put)
        }
        /// Registers a handler for DELETE requests.
        pub fn delete<F, Fut>(&mut self, path: impl AsRef<str>, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(path, handler, $crate::router::MethodKind::Delete)
        }
        /// Registers a handler for PATCH requests.
        pub fn patch<F, Fut>(&mut self, path: impl AsRef<str>, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(path, handler, $crate::router::MethodKind::Patch)
        }
        /// Registers a handler for HEAD requests.
        pub fn head<F, Fut>(&mut self, path: impl AsRef<str>, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(path, handler, $crate::router::MethodKind::Head)
        }
        /// Registers a handler for CONNECT requests.
        pub fn connect<F, Fut>(&mut self, path: impl AsRef<str>, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(path, handler, $crate::router::MethodKind::Connect)
        }
        /// Registers a handler for TRACE requests.
        pub fn trace<F, Fut>(&mut self, path: impl AsRef<str>, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(path, handler, $crate::router::MethodKind::Trace)
        }
    };
    (route) => {
        /// Registers a handler for GET requests.
        pub fn get<F, Fut>(&mut self, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(handler, $crate::router::MethodKind::Get)
        }
        /// Registers a handler for POST requests.
        pub fn post<F, Fut>(&mut self, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(handler, $crate::router::MethodKind::Post)
        }
        /// Registers a handler for PUT requests.
        pub fn put<F, Fut>(&mut self, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(handler, $crate::router::MethodKind::Put)
        }
        /// Registers a handler for DELETE requests.
        pub fn delete<F, Fut>(&mut self, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(handler, $crate::router::MethodKind::Delete)
        }
        /// Registers a handler for PATCH requests.
        pub fn patch<F, Fut>(&mut self, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(handler, $crate::router::MethodKind::Patch)
        }
        /// Registers a handler for HEAD requests.
        pub fn head<F, Fut>(&mut self, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(handler, $crate::router::MethodKind::Head)
        }
        /// Registers a handler for CONNECT requests.
        pub fn connect<F, Fut>(&mut self, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(handler, $crate::router::MethodKind::Connect)
        }
        /// Registers a handler for TRACE requests.
        pub fn trace<F, Fut>(&mut self, handler: F) -> &mut Self
        where
            F: Fn($crate::handler::Request<B>, $crate::handler::Response) -> Fut
                + Send
                + Sync
                + 'static,
            Fut: std::future::Future<Output = $crate::handler::Response> + Send + 'static,
        {
            self.add_route(handler, $crate::router::MethodKind::Trace)
        }
    };
}

impl<B: Send + 'static> Router<B> {
    define_methods!();

    fn add_route(
        &mut self,
        path: impl AsRef<str>,
        handler: impl Handler<B>,
        method: MethodKind,
    ) -> &mut Self {
        self.route(path, handler, method);
        self
    }
}

impl std::fmt::Debug for MethodRoutes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        for (m, r) in self.iter() {
            map.entry(&m, r);
        }
        map.finish()
    }
}

/// A route builder for a specific path, allowing for method chaining.
pub struct Route<'a, B = Incoming> {
    router: &'a mut Router<B>,
    path: Arc<str>,
}

impl<'a, B: Send + 'static> Route<'a, B> {
    /// Registers a handler for all HTTP methods on this route.
    pub fn all<F, Fut>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(Request<B>, Response) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        for &method in &MethodKind::ALL {
            self.router
                .route(self.path.as_ref(), handler.clone(), method);
        }
        self
    }

    fn add_route(&mut self, handler: impl Handler<B>, method: MethodKind) -> &mut Self {
        self.router.route(self.path.as_ref(), handler, method);
        self
    }

    define_methods!(route);
}

impl<B> std::fmt::Debug for Router<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Router")
            .field("stack_len", &self.stack.len())
            .finish()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::Response;

    async fn mock_handler<B: Send + 'static>(_req: Request<B>, res: Response) -> Response {
        res.send_text("ok")
    }

    #[test]
    fn test_router_add_route() {
        let mut router = Router::<()>::default();
        router.get("/", mock_handler);

        assert_eq!(router.stack.len(), 1);
        assert!(router.routes.contains_method(MethodKind::Get));
    }

    #[test]
    fn test_router_all_methods() {
        let mut router = Router::<()>::default();
        router.all("/all", mock_handler);

        // MethodKind::ALL has several methods (GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS, CONNECT, TRACE)
        assert!(router.stack.len() >= 5);
        for &method in &MethodKind::ALL {
            assert!(router.routes.contains_method(method));
        }
    }

    #[test]
    fn test_router_trailing_slash_normalization() {
        let mut router = Router::<()>::default();
        router.get("/test/", mock_handler);

        let method_router = router.routes.get(MethodKind::Get).unwrap();
        assert!(method_router.path_to_idx.contains_key("/test"));
        assert!(!method_router.path_to_idx.contains_key("/test/"));
    }

    #[test]
    fn test_router_use_with() {
        let mut router = Router::<()>::default();
        router.use_with("/api", |_: &mut Request<()>, _: &mut Response| async {
            crate::middleware::next_res()
        });

        assert_eq!(router.stack.len(), 1);
        assert_eq!(router.middleware_matchers.len(), 1);
        assert_eq!(router.middleware_matchers[0].path.as_ref(), "/api");
    }
}
