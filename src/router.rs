use self::interner::INTERNER;
use crate::{handler::{ExpressResponse, Handler, Request, Response, request::RequestMetadataInternal}, prelude::Middleware};
use ahash::HashMap;
use hyper::body::Incoming;
use layer::Layer;
use smallvec::{SmallVec, smallvec};
use std::sync::Arc;

pub mod interner;
mod layer;
mod method;

pub use method::MethodKind;

pub type LayerIndices = SmallVec<[usize; 8]>;

#[derive(Debug, Default)]
pub struct MethodRouter {
    pub matcher: matchit::Router<usize>,
    pub indices: Vec<LayerIndices>,
    pub path_to_idx: HashMap<Arc<str>, usize>,
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

#[derive(Debug)]
pub struct MiddlewareMatcher {
    pub path: Arc<str>,
    pub router: matchit::Router<()>,
    pub indices: LayerIndices,
}

pub type MethodRoutes = HashMap<MethodKind, MethodRouter>;

/// The core routing engine for `express_rs`.
pub struct Router<B = Incoming> {
    pub stack: Vec<Layer<B>>,
    pub middleware_matchers: Vec<MiddlewareMatcher>,
    pub routes: MethodRoutes,
    pub not_found_handler: Option<Arc<dyn Handler<B>>>,
}

impl<B> Default for Router<B> {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            middleware_matchers: Vec::new(),
            routes: HashMap::default(),
            not_found_handler: None,
        }
    }
}

impl<B: Send + 'static> Router<B> {
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

        let method_routes = self.routes.entry(method).or_default();
        method_routes.add_route(&path, layer_index);

        let layer = Layer::route(Arc::clone(&path), method, vec![], Arc::new(handler));
        self.stack.push(layer);

        self.stack.last_mut().unwrap()
    }

    pub fn all(&mut self, path: impl AsRef<str>, handler: impl Handler<B> + Clone) -> &mut Self {
        let path: Arc<str> = path.as_ref().into();
        for &method in &MethodKind::ALL {
            self.route(path.as_ref(), handler.clone(), method);
        }
        self
    }

    pub fn route_builder(&mut self, path: impl AsRef<str>) -> Route<'_, B> {
        Route {
            router: self,
            path: path.as_ref().into(),
        }
    }

    pub fn use_with(&mut self, path: impl AsRef<str>, middleware: impl Middleware<B>) -> &mut Self {
        let path: Arc<str> = path.as_ref().into();
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
            let p = path.as_ref();
            router.insert(p, ()).ok();
            
            // Express-style prefix matching: /path should match /path, /path/, and /path/sub
            if p == "/" {
                router.insert("/*middleware_wildcard", ()).ok();
            } else {
                let prefix_path = if p.ends_with('/') {
                    format!("{}*middleware_wildcard", p)
                } else {
                    format!("{}/*middleware_wildcard", p)
                };
                router.insert(prefix_path, ()).ok();
            }

            self.middleware_matchers.push(MiddlewareMatcher {
                path: Arc::clone(&path),
                router,
                indices: smallvec![layer_index],
            });
        }

        let layer = Layer::middleware(Arc::clone(&path), vec![Arc::new(middleware)]);
        self.stack.push(layer);

        self
    }

    pub fn not_found(&mut self, handler: impl Handler<B>) -> &mut Self {
        self.not_found_handler = Some(Arc::new(handler));
        self
    }

    pub async fn handle(&self, mut req: Request<B>, res: Response) -> Response {
        let raw_path = req.uri().path();
        let path = if raw_path.len() > 1 && raw_path.ends_with('/') {
            &raw_path[..raw_path.len() - 1]
        } else {
            raw_path
        };
        let method = &MethodKind::from_hyper(req.method());

        let mut matched = SmallVec::<[&usize; 8]>::new();
        let mut route_params = SmallVec::<[(interner::Symbol, Arc<str>); 4]>::new();

        for matcher in &self.middleware_matchers {
            if let Ok(matched_route) = matcher.router.at(path) {
                for (k, v) in matched_route.params.iter() {
                    let sym_k = INTERNER.get_or_intern(k);
                    route_params.push((sym_k, v.into()));
                }
                matched.extend(matcher.indices.iter());
            }
        }

        let mut path_exists = false;

        if let Some(method_routes) = self.routes.get(method)
            && let Ok(route_match) = method_routes.matcher.at(path)
        {
            path_exists = true;

            if !route_match.params.is_empty() {
                for (k, v) in route_match.params.iter() {
                    let sym_k = INTERNER.get_or_intern(k);
                    route_params.push((sym_k, v.into()));
                }
            }

            matched.extend(method_routes.indices[*route_match.value].iter());
        }

        if !path_exists {
            for (m, method_routes) in &self.routes {
                if m != method && method_routes.matcher.at(path).is_ok() {
                    path_exists = true;
                    break;
                }
            }
        }

        if matched.is_empty() {
            let status = if path_exists { 405 } else { 404 };

            if status == 404 {
                if let Some(h) = &self.not_found_handler {
                    return h.call(req, res).await;
                }
            }

            return res.status_code(status).send_text(match status {
                404 => "Not Found",
                _ => "Method Not Allowed",
            });
        }

        req.set_params(route_params);

        if matched.len() > 1 {
            matched.sort_unstable();
            matched.dedup();
        }

        let mut req_opt = Some(req);
        let mut res_opt = Some(res);

        for i in matched {
            let layer = &self.stack[*i];

            if let Some(m) = &layer.method {
                if m != method {
                    continue;
                }
            }

            let mut stopped = false;
            for mw in &layer.middlewares {
                let req_mut = req_opt.as_mut().unwrap();
                let res_mut = res_opt.as_mut().unwrap();
                if mw.call(req_mut, res_mut).await.is_stop() {
                    stopped = true;
                    break;
                }
            }

            if stopped {
                return res_opt.unwrap();
            }

            if let Some(h) = &layer.handler {
                return h.call(req_opt.take().unwrap(), res_opt.take().unwrap()).await;
            }
        }

        let status = if path_exists { 405 } else { 404 };

        if status == 404 {
            if let Some(h) = &self.not_found_handler {
                return h.call(req_opt.take().unwrap(), res_opt.take().unwrap()).await;
            }
        }

        res_opt.unwrap().status_code(status).send_text(match status {
            404 => "Not Found",
            _ => "Method Not Allowed",
        })
    }

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
                let method_routes = self.routes.entry(method).or_default();
                method_routes.add_route(&new_path, layer_index);
            } else {
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
                    r.insert(new_path.as_ref(), ())
                        .expect("Failed to insert nested middleware");
                    self.middleware_matchers.push(MiddlewareMatcher {
                        path: Arc::clone(&new_path),
                        router: r,
                        indices: smallvec![layer_index],
                    });
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

#[macro_export]
macro_rules! define_methods {
    () => {
        pub fn get(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::Handler<B>,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Get)
        }
        pub fn post(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::Handler<B>,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Post)
        }
        pub fn put(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::Handler<B>,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Put)
        }
        pub fn delete(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::Handler<B>,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Delete)
        }
        pub fn patch(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::Handler<B>,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Patch)
        }
        pub fn head(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::Handler<B>,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Head)
        }
        pub fn connect(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::Handler<B>,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Connect)
        }
        pub fn trace(
            &mut self,
            path: impl AsRef<str>,
            handler: impl $crate::handler::Handler<B>,
        ) -> &mut Self {
            self.add_route(path, handler, $crate::router::MethodKind::Trace)
        }
    };
    (route) => {
        pub fn get(&mut self, handler: impl $crate::handler::Handler<B>) -> &mut Self {
            self.add_route(handler, $crate::router::MethodKind::Get)
        }
        pub fn post(&mut self, handler: impl $crate::handler::Handler<B>) -> &mut Self {
            self.add_route(handler, $crate::router::MethodKind::Post)
        }
        pub fn put(&mut self, handler: impl $crate::handler::Handler<B>) -> &mut Self {
            self.add_route(handler, $crate::router::MethodKind::Put)
        }
        pub fn delete(&mut self, handler: impl $crate::handler::Handler<B>) -> &mut Self {
            self.add_route(handler, $crate::router::MethodKind::Delete)
        }
        pub fn patch(&mut self, handler: impl $crate::handler::Handler<B>) -> &mut Self {
            self.add_route(handler, $crate::router::MethodKind::Patch)
        }
        pub fn head(&mut self, handler: impl $crate::handler::Handler<B>) -> &mut Self {
            self.add_route(handler, $crate::router::MethodKind::Head)
        }
        pub fn connect(&mut self, handler: impl $crate::handler::Handler<B>) -> &mut Self {
            self.add_route(handler, $crate::router::MethodKind::Connect)
        }
        pub fn trace(&mut self, handler: impl $crate::handler::Handler<B>) -> &mut Self {
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

/// A route builder for a specific path, allowing for method chaining.
pub struct Route<'a, B = Incoming> {
    router: &'a mut Router<B>,
    path: Arc<str>,
}

impl<'a, B: Send + 'static> Route<'a, B> {
    pub fn all(&mut self, handler: impl Handler<B> + Clone) -> &mut Self {
        for &method in &MethodKind::ALL {
            self.router.route(self.path.as_ref(), handler.clone(), method);
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
