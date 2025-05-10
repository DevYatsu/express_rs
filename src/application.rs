use crate::handler::response::error::ResponseError;
use crate::handler::{Request as ExpressRequest, Response as ExpressResponse};
use crate::router::{Middleware, Router};
use crate::server::Server;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct App {
    pub cache: HashMap<String, u32>,
    pub engines: Vec<u32>,
    pub settings: HashMap<String, u32>,
    pub router: Option<Router>,
}

impl App {
    pub fn handle(&self, req: &mut ExpressRequest, res: &mut ExpressResponse) {
        self.router.as_ref().unwrap().handle(req, res);
    }

    pub fn lazyrouter(&mut self) {
        if self.router.is_none() {
            let router = Router::default();

            self.router = Some(router);
        }
    }

    pub fn use_with<M: Middleware>(&mut self, middleware: M) {
        self.lazyrouter();
        self.router.as_mut().unwrap().use_with(middleware);
    }

    pub async fn listen<T: FnOnce()>(self, port: u16, callback: T) {
        use hyper::service::service_fn;

        let addr: SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();

        let app = std::sync::Arc::new(self);

        let handler_factory = {
            let app = app.clone();
            move || {
                let app = app.clone();
                service_fn(move |req| {
                    let app = app.clone();
                    async move {
                        if cfg!(debug_assertions) {
                            let start = Instant::now();

                            let method = req.method().clone();
                            let path = req.uri().path().to_string();

                            let app = Arc::try_unwrap(app).unwrap_or_else(|arc| (*arc).clone());
                            let response = app.call(req).await;

                            let elapsed = start.elapsed();

                            info!("{} {} ({} ms)", method, path, elapsed.as_millis());

                            response
                        } else {
                            let app = Arc::try_unwrap(app).unwrap_or_else(|arc| (*arc).clone());
                            app.call(req).await
                        }
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

use hyper::body::{Body, Bytes, Incoming};
use hyper::service::Service;
use hyper::{Request, Response};
use log::info;

impl Service<Request<Incoming>> for App {
    type Response = Response<Pin<Box<dyn Body<Data = Bytes, Error = ResponseError> + Send>>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, mut req: Request<Incoming>) -> Self::Future {
        let mut response_builder = ExpressResponse::default();
        self.handle(&mut req, &mut response_builder);

        let response = response_builder.into_hyper();

        Box::pin(async move { Ok(response) })
    }
}

use crate::handler::Handler;

macro_rules! generate_methods {
    (
        methods: [$($method:ident),* $(,)?]
    ) => {
        impl App {
            $(
                pub fn $method(&mut self, path: impl AsRef<str>, handle: impl Into<Handler>) -> &mut Self {
                    use hyper::Method;
                    self.lazyrouter();
                    let handler = handle.into();
                    // DO NOT ABSOLUTELY REMOVE .to_uppercase call, it's needed for comparaison of Method struct
                    let route = self.router.as_mut().unwrap().route(path, handler.clone(), &Method::from_str(&stringify!($method).to_uppercase()).unwrap());
                    route.$method(handler);

                    if cfg!(debug_assertions) {
                        println!("route: {:?}", route);
                    }

                    self
                }
            )*
        }
    };
}

generate_methods! {
    methods: [get, post, put, delete, patch, head, connect, trace]
}
