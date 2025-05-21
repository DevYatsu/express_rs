use crate::handler::response::error::ResponseError;
use crate::handler::{Request as ExpressRequest, Response as ExpressResponse};
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
    pub async fn handle(&self, req: &mut ExpressRequest, res: &mut ExpressResponse) {
        self.router.handle(req, res).await
    }

    pub fn use_with<M: Middleware>(&mut self, middleware: M) {
        self.router.use_with(middleware);
    }

    pub async fn listen<T, Fut>(self, port: u16, callback: T)
    where
        Self: Sized + Send + Sync + 'static,
        T: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        use hyper::service::service_fn;

        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

        let app = AppService(Arc::new(self));

        callback().await;

        let handler_factory = {
            let app = app.clone();
            move || {
                let app = app.clone();
                service_fn(move |req| {
                    let app = app.clone();
                    async move {
                        #[cfg(debug_assertions)]
                        let start = Instant::now();
                        #[cfg(debug_assertions)]
                        let method = req.method().clone();
                        #[cfg(debug_assertions)]
                        let path = req.uri().path().to_string();

                        let response = app.call(req).await;

                        #[cfg(debug_assertions)]
                        {
                            let elapsed = start.elapsed();

                            info!("{} {} ({} ms)", method, path, elapsed.as_millis());
                        }

                        response
                    }
                })
            }
        };

        if let Err(e) = Server::bind(addr, handler_factory).await {
            eprintln!("server error: {}", e);
        }
    }
}

use hyper::body::{Body, Bytes, Incoming};
use hyper::service::Service;
use hyper::{Request, Response};
use log::info;

#[derive(Clone)]
pub struct AppService(pub Arc<App>);

impl Service<Request<Incoming>> for AppService {
    type Response = Response<Pin<Box<dyn Body<Data = Bytes, Error = ResponseError> + Send>>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, mut req: Request<Incoming>) -> Self::Future {
        let app = Arc::clone(&self.0);
        Box::pin(async move {
            let mut response_builder = ExpressResponse::default();
            app.handle(&mut req, &mut response_builder).await;
            let response = response_builder.into_hyper();
            Ok(response)
        })
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
