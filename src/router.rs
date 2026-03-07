use self::interner::INTERNER;
use crate::handler::{
    ExpressResponse, FnHandler, Middleware, Request, Response, request::RequestExtInternal,
};
use ahash::HashMap;
use layer::Layer;
use smallvec::{SmallVec, smallvec};

pub mod interner;
mod layer;
mod method;

pub use method::MethodKind;

pub type LayerIndices = SmallVec<[usize; 8]>;

#[derive(Debug, Default)]
pub struct MethodRouter {
    pub matcher: matchit::Router<usize>,
    pub indices: Vec<LayerIndices>,
    pub path_to_idx: HashMap<String, usize>,
}

impl MethodRouter {
    fn add_route(&mut self, path: &str, layer_index: usize) {
        if let Some(&idx) = self.path_to_idx.get(path) {
            self.indices[idx].push(layer_index);
        } else {
            let idx = self.indices.len();
            self.indices.push(smallvec![layer_index]);
            self.path_to_idx.insert(path.to_string(), idx);
            self.matcher
                .insert(path.to_string(), idx)
                .expect("Failed to insert route");
        }
    }
}

#[derive(Debug)]
pub struct MiddlewareMatcher {
    pub path: String,
    pub router: matchit::Router<()>,
    pub indices: LayerIndices,
}

pub type MethodRoutes = HashMap<MethodKind, MethodRouter>;

/// The core routing engine for `express_rs`.
///
/// `Router` is responsible for registering route and middleware handlers,
/// and efficiently dispatching them based on request paths and HTTP methods.
#[derive(Debug, Default)]
pub struct Router {
    /// All layers (routes and middleware), stored in order of registration.
    ///
    /// Each `Layer` contains metadata and a handler function.
    pub stack: Vec<Layer>,

    /// Path-based matchers for global middleware.
    ///
    /// Middlewares are evaluated against all registered patterns.
    pub middleware_matchers: Vec<MiddlewareMatcher>,

    /// HTTP method-specific route matchers.
    ///
    /// For example, `Method::GET` maps to its own matcher tree.
    pub routes: MethodRoutes,
}

impl Router {
    pub fn route(
        &mut self,
        path: impl AsRef<str>,
        handler: impl FnHandler,
        method: MethodKind,
    ) -> &mut Layer {
        let path_ref = path.as_ref();
        let layer_index = self.stack.len();

        let method_routes = self.routes.entry(method).or_default();
        method_routes.add_route(path_ref, layer_index);

        let route = Layer::route(path_ref, method, handler);
        self.stack.push(route);

        self.stack.last_mut().unwrap()
    }

    pub fn use_with(&mut self, path: impl AsRef<str>, middleware: impl Middleware) -> &mut Self {
        let path = path.as_ref();
        let layer_index = self.stack.len();

        let mut found = false;
        for matcher in &mut self.middleware_matchers {
            if matcher.path == path {
                matcher.indices.push(layer_index);
                found = true;
                break;
            }
        }

        if !found {
            let mut router = matchit::Router::new();
            router
                .insert(path.to_string(), ())
                .expect("Failed to insert middleware");
            self.middleware_matchers.push(MiddlewareMatcher {
                path: path.to_string(),
                router,
                indices: smallvec![layer_index],
            });
        }

        let middleware = Layer::middleware(path, middleware);
        self.stack.push(middleware);

        self
    }

    /// Mounts another Router at the specified prefix path.
    /// This works truly like Express (`app.use('/api', router)`), but it is
    /// highly optimized! It flattens the nested router into the parent router,
    /// avoiding runtime hierarchical traversal overhead entirely.
    pub fn use_router(&mut self, prefix: impl AsRef<str>, router: Router) -> &mut Self {
        let prefix = prefix.as_ref();
        let prefix = if prefix.ends_with('/') {
            &prefix[..prefix.len() - 1]
        } else {
            prefix
        };

        for layer in router.stack {
            match layer {
                Layer::Route {
                    path,
                    method,
                    handler,
                } => {
                    let new_path = if path == "/" {
                        prefix.to_string()
                    } else {
                        format!("{}{}", prefix, path)
                    };

                    let layer_index = self.stack.len();
                    let method_routes = self.routes.entry(method).or_default();
                    method_routes.add_route(&new_path, layer_index);

                    self.stack.push(Layer::Route {
                        path: new_path,
                        method,
                        handler,
                    });
                }
                Layer::Middleware { path, handler } => {
                    let new_path = if path.starts_with('/') {
                        format!("{}{}", prefix, path)
                    } else {
                        format!("{}/{}", prefix, path)
                    };

                    let layer_index = self.stack.len();
                    let mut found = false;
                    for matcher in &mut self.middleware_matchers {
                        if matcher.path == new_path {
                            matcher.indices.push(layer_index);
                            found = true;
                            break;
                        }
                    }

                    if !found {
                        let mut r = matchit::Router::new();
                        r.insert(new_path.clone(), ())
                            .expect("Failed to insert nested middleware");
                        self.middleware_matchers.push(MiddlewareMatcher {
                            path: new_path.clone(),
                            router: r,
                            indices: smallvec![layer_index],
                        });
                    }

                    self.stack.push(Layer::Middleware {
                        path: new_path,
                        handler,
                    });
                }
            }
        }

        self
    }

    pub async fn handle(&self, mut req: Request, mut res: Response) -> Response {
        let path = req.uri().path();
        let method = &MethodKind::from_hyper(req.method());

        let mut matched = SmallVec::<[&usize; 8]>::new();
        let mut route_params = SmallVec::<[(interner::Symbol, interner::Symbol); 4]>::new();

        for matcher in &self.middleware_matchers {
            if let Ok(matched_route) = matcher.router.at(path) {
                for (k, v) in matched_route.params.iter() {
                    let sym_k = INTERNER.get_or_intern(k);
                    let sym_v = INTERNER.get_or_intern(v);
                    route_params.push((sym_k, sym_v));
                }
                matched.extend(matcher.indices.iter());
            }
        }

        let mut path_exists = false;

        if let Some(method_routes) = self.routes.get(method) {
            if let Ok(route_match) = method_routes.matcher.at(path) {
                path_exists = true;

                if !route_match.params.is_empty() {
                    for (k, v) in route_match.params.iter() {
                        let sym_k = INTERNER.get_or_intern(k);
                        let sym_v = INTERNER.get_or_intern(v);
                        route_params.push((sym_k, sym_v));
                    }
                }

                matched.extend(method_routes.indices[*route_match.value].iter());
            }
        }

        if matched.is_empty() {
            // let's check other methods to return proper 405 or 404
            for (m, method_routes) in &self.routes {
                if m != method {
                    if method_routes.matcher.at(path).is_ok() {
                        path_exists = true;
                        break;
                    }
                }
            }

            let status = if path_exists { 405 } else { 404 };

            return res.status_code(status).unwrap().send_text(match status {
                404 => "Not Found",
                _ => "Method Not Allowed",
            });
        }

        req.set_params(route_params);

        // sort and dedup
        matched.sort_unstable();
        matched.dedup();

        #[cfg(debug_assertions)]
        println!("Matched layers: {:?}", matched);

        for i in matched {
            let layer = &self.stack[*i];

            match layer {
                Layer::Middleware { .. } => {
                    if layer
                        .handle_middleware_request(&mut req, &mut res)
                        .await
                        .is_stop()
                    {
                        return res;
                    };

                    continue;
                }
                Layer::Route {
                    method: route_method,
                    ..
                } => {
                    if route_method != method {
                        continue;
                    }

                    let res = layer.handle_fn_request(req, res).await;

                    return res;
                }
            };
        }

        // no route matched
        res.status_code(405)
            .unwrap()
            .send_text("Method Not Allowed")
    }
}

#[macro_export]
macro_rules! define_methods {
    () => {
        pub fn get(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::FnHandler,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Get)
        }
        pub fn post(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::FnHandler,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Post)
        }
        pub fn put(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::FnHandler,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Put)
        }
        pub fn delete(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::FnHandler,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Delete)
        }
        pub fn patch(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::FnHandler,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Patch)
        }
        pub fn head(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::FnHandler,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Head)
        }
        pub fn connect(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::FnHandler,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Connect)
        }
        pub fn trace(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::FnHandler,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Trace)
        }
    };
}

impl Router {
    define_methods!();

    fn add_route(
        &mut self,
        path: impl AsRef<str>,
        handler: impl FnHandler,
        method: MethodKind,
    ) -> &mut Self {
        self.route(path, handler, method);
        self
    }
}
