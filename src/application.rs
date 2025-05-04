use crate::express::Next;
use crate::handler::{Request as ExpressRequest, Response as ExpressResponse};
use crate::router::Router;
use crate::server::Server;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct App {
    pub cache: HashMap<String, u32>,
    pub engines: Vec<u32>,
    pub settings: HashMap<String, u32>,
    pub _router: Option<Router>,
}

impl App {
    pub fn handle(&self, req: ExpressRequest, res: &mut ExpressResponse, next: Next) {
        self._router.as_ref().unwrap().handle(req, res, next);
    }

    pub fn lazy_router(&mut self) {
        if self._router.is_none() {
            self._router = Some(Router::default());
        }
    }

    pub async fn listen<T: FnOnce()>(self, port: u16, callback: T) {
        use hyper::service::service_fn;

        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

        let app = std::sync::Arc::new(self);

        let handler_factory = {
            let app = app.clone();
            move || {
                let app = app.clone();
                service_fn(move |req| {
                    let app = app.clone();
                    async move {
                        let app = Arc::try_unwrap(app).unwrap_or_else(|arc| (*arc).clone());
                        app.call(req).await
                    }
                })
            }
        };

        callback();

        if let Err(e) = Server::bind(addr, handler_factory).await {
            eprintln!("server error: {}", e);
        }
    }
}

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::Service;
use hyper::{Request, Response};

impl Service<Request<Incoming>> for App {
    type Response = Response<Full<Bytes>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let mut response_builder = ExpressResponse::default();

        self.handle(req, &mut response_builder, Next);

        let response = response_builder.into_hyper();

        Box::pin(async move { Ok(response) })
    }
}

use crate::handler::Handler;
use std::path::Path;

macro_rules! generate_methods {
    (
        methods: [$($method:ident),* $(,)?]
    ) => {
        impl App {
            $(
                pub fn $method(&mut self, path: impl AsRef<Path>, handle: impl Into<Handler>) -> &mut Self {
                    self.lazy_router();
                    let handler = handle.into();
                    let route = self._router.as_mut().unwrap().route(path, handler.clone());
                    route.$method(handler);

                    self
                }
            )*
        }
    };
}

generate_methods! {
    methods: [get, post, patch, delete]
}
