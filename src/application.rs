use crate::handler::middleware::Middleware;
use crate::handler::response::error::ResponseError;
use crate::handler::{Request as ExpressRequest, Response as ExpressResponse};
use crate::router::Router;
use crate::server::Server;
use hyper::body::{Body, Bytes, Incoming};
use hyper::service::Service;
use hyper::{Request, Response};
use log::info;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

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
#[derive(Default)]
pub struct App<S: Clone + Sync + Send + 'static = ()> {
    /// The internal router responsible for matching and handling requests.
    ///
    /// Unlike the original Express.js (which initializes the router lazily),
    /// this is always present to keep logic simple and fast.
    pub router: Router,

    state: Arc<RwLock<S>>,
}

impl<S: Clone + Sync + Send + 'static> App<S> {
    pub fn with_state(state: S) -> Self {
        Self {
            router: Router::default(),
            state: Arc::new(RwLock::new(state)),
        }
    }

    /// Consumes self and returns a new App with a different state type.
    pub fn with_new_state<T: Clone + Sync + Send + 'static>(self, state: T) -> App<T> {
        App {
            router: self.router, // Move the router over
            state: Arc::new(RwLock::new(state)),
        }
    }

    pub async fn state(&self) -> tokio::sync::RwLockReadGuard<'_, S> {
        self.state.read().await
    }

    pub async fn state_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, S> {
        self.state.write().await
    }

    /// Creates a new `App` instance with an empty router.
    pub async fn handle(&self, mut req: ExpressRequest, res: ExpressResponse) -> ExpressResponse {
        req.extensions_mut().insert(self.state.clone());
        self.router.handle(req, res).await
    }

    pub fn use_with(&mut self, path: impl AsRef<str>, middleware: impl Middleware) {
        info!("Adding middleware to path: {}", path.as_ref());
        self.router.use_with(path, middleware);
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
                        let response = app.call(req).await;
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

#[derive(Clone)]
struct AppService<S: Clone + Sync + Send + 'static = ()>(Arc<App<S>>);

impl<S: Clone + Sync + Send + 'static> Service<Request<Incoming>> for AppService<S> {
    type Response = Response<Pin<Box<dyn Body<Data = Bytes, Error = ResponseError> + Send>>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let app = Arc::clone(&self.0);
        Box::pin(async move {
            let response = app.handle(req, ExpressResponse::default()).await;
            Ok(response.into_hyper())
        })
    }
}

use crate::handler::FnHandler;
use crate::router::MethodKind;

macro_rules! generate_methods {
    (
        methods: [$($method:ident),* $(,)?]
    ) => {
        impl<S: Clone + Sync + Send + 'static> App<S> {
            $(
                pub fn $method(&mut self, path: impl AsRef<str>, handler: impl FnHandler) -> &mut Self
                {
                    use hyper::Method;
                    info!("Adding {} handler to path: {}", stringify!($method), path.as_ref());

                    // DO NOT ABSOLUTELY REMOVE .to_uppercase call, it's needed for comparaison of Method struct
                    let method = MethodKind::from_hyper(&Method::from_str(&stringify!($method).to_uppercase()).expect("This method is not a valid Method"));

                    let route = self.router.route(path, handler, method);

                    #[cfg(debug_assertions)]
                    println!("route: {:?}", route);

                    self
                }
            )*
        }
    };
}

generate_methods! {
    methods: [get, post, put, delete, patch, head, connect, trace]
}
