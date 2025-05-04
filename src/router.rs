use crate::handler::{Handler, Next, Request, Response};
use layer::Layer;
use route::Route;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

mod layer;
mod middleware;
mod route;

pub use middleware::Middleware;

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
        self.stack.last_mut().unwrap().route_mut().as_mut().unwrap()
    }

    pub fn use_with<M: Middleware>(&mut self, middleware: M) -> &mut Self {
        let mut layer = Layer::new(middleware.path(), M::new());
        layer.kind = M::layer_kind();
        self.stack.push(layer);

        return self;
    }

    pub fn handle(&self, req: Request, res: &mut Response) {
        let stack = Arc::new(self.stack.clone());
        let req = Arc::new(req);
        let res = Arc::new(Mutex::new(res));
        let index = Arc::new(AtomicUsize::new(0));

        Self::dispatch(stack, index, req, res);
    }

    fn dispatch(
        stack: Arc<Vec<Layer>>,
        index: Arc<AtomicUsize>,
        req: Arc<Request>,
        res: Arc<Mutex<&mut Response>>,
    ) {
        while let Some(i) = Self::next_index(&index, stack.len()) {
            let layer = &stack[i];

            if !layer.match_path(req.uri().path()) {
                continue;
            }

            let called_next = Arc::new(AtomicBool::new(false));
            let next_signal = {
                let called_next = Arc::clone(&called_next);
                Box::new(move || {
                    called_next.store(true, Ordering::SeqCst);
                }) as Next
            };

            if layer.match_path(req.uri().path()) && layer.route.is_none() {
                // middleware matching route
                if let Ok(mut res_guard) = res.lock() {
                    layer.handle_request(&req, &mut *res_guard, next_signal);
                }

                // If `next` was not called, stop processing
                if !called_next.load(Ordering::SeqCst) {
                    return;
                }

                continue;
            }

            let route = layer.route.as_ref().unwrap();

            if !route.methods.contains(req.method()) || route.stack.is_empty() {
                continue;
            }

            if let Ok(mut res_guard) = res.lock() {
                route.stack[0].handle_request(&req, &mut *res_guard, next_signal);
            }

            if !called_next.load(Ordering::Acquire) {
                return;
            }
        }

        // No matching middleware -> fallback
        if let Ok(mut res_guard) = res.lock() {
            if !res_guard.is_ended() {
                res_guard.status_code(404).send("Not Found");
            }
        }
    }

    #[inline]
    fn next_index(index: &AtomicUsize, max: usize) -> Option<usize> {
        let i = index.fetch_add(1, Ordering::SeqCst);
        if i < max { Some(i) } else { None }
    }
}
