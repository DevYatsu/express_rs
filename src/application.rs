use crate::handler::Response as ExpressResponse;
use crate::handler::response::error::ResponseError;
use crate::router::{Middleware, Router};
use crate::server::Server;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

/// The main application structure for `express_rs`.
///
/// This struct represents the entry point of your web application. It holds
/// the internal router and provides methods to register middleware, define
/// routes, and start the server.
///
/// # Example
///
/// ```rust
/// let mut app = App::default();
/// app.get("/", |req, res, next| {
///     res.send("Hello, world!");
/// });
/// app.listen(3000, || println!("Server started on port 3000")).await;
/// ```
#[derive(Clone, Default)]
pub struct App {
    /// The internal router responsible for matching and handling requests.
    ///
    /// Unlike the original Express.js (which initializes the router lazily),
    /// this is always present to keep logic simple and fast.
    pub router: Router,
}

impl App {
    async fn handle<'a>(&'a self, req: &'a mut Request<Incoming>, res: &'a mut ExpressResponse) {
        self.router.handle(req, res).await
    }

    fn owned_or_cloned(self: Arc<Self>) -> Self {
        Arc::try_unwrap(self).unwrap_or_else(|arc| (*arc).clone())
    }

    pub fn use_with<M: Middleware>(&mut self, middleware: M) {
        self.router.use_with(middleware);
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

                            let app = App::owned_or_cloned(app);
                            let response = app.call(req).await;

                            let elapsed = start.elapsed();

                            info!("{} {} ({} ms)", method, path, elapsed.as_millis());

                            response
                        } else {
                            let app = App::owned_or_cloned(app);
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

use crate::handler::Handler;
use hyper::body::{Body, Bytes, Incoming};
use hyper::service::Service;
use hyper::{Request, Response};
use log::info;

impl Service<Request<Incoming>> for App {
    type Response = Response<Pin<Box<dyn Body<Data = Bytes, Error = ResponseError> + Send>>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, mut req: Request<Incoming>) -> Self::Future {
        let handler = self.clone();

        let fut = async move {
            let mut res = ExpressResponse::default();
            handler.handle(&mut req, &mut res).await;
            Ok(res.into_hyper())
        };

        Box::pin(fut)
    }
}

macro_rules! generate_methods {
    (
        methods: [$($method:ident),* $(,)?]
    ) => {
        impl App {
            $(
                pub fn $method(&mut self, path: impl AsRef<str>, handle: impl Into<Handler>) -> &mut Self {
                    use hyper::Method;
                    let handler = handle.into();
                    // DO NOT ABSOLUTELY REMOVE .to_uppercase call, it's needed for comparaison of Method struct
                    let route = self.router.route(path, handler.clone(), &Method::from_str(&stringify!($method).to_uppercase()).unwrap());
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
