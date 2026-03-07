use crate::handler::FnHandler;
use crate::handler::{Middleware, Request as ExpressRequest, Response as ExpressResponse};
use crate::router::{MethodKind, Route, Router};
use crate::server::Server;
use log::info;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_rustls::rustls::ServerConfig;

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
pub struct App<S: Sync + Send + 'static = ()> {
    /// The internal router responsible for matching and handling requests.
    ///
    /// Unlike the original Express.js (which initializes the router lazily),
    /// this is always present to keep logic simple and fast.
    pub router: Router,

    state: Arc<S>,
}

impl<S: Sync + Send + 'static> App<S> {
    pub fn with_state(state: S) -> Self {
        Self {
            router: Router::default(),
            state: Arc::new(state),
        }
    }

    /// Consumes self and returns a new App with a different state type.
    pub fn with_new_state<T: Sync + Send + 'static>(self, state: T) -> App<T> {
        App {
            router: self.router, // Move the router over
            state: Arc::new(state),
        }
    }

    pub async fn state(&self) -> &S {
        &self.state
    }

    /// Creates a new `App` instance with an empty router.
    pub async fn handle(&self, mut req: ExpressRequest, res: ExpressResponse) -> ExpressResponse {
        req.extensions_mut().insert(self.state.clone());
        req.extensions_mut()
            .insert(crate::handler::request::Locals::default());
        self.router.handle(req, res).await
    }

    pub fn use_with(&mut self, path: impl AsRef<str>, middleware: impl Middleware) -> &mut Self {
        info!("Adding middleware to path: {}", path.as_ref());
        self.router.use_with(path, middleware);
        self
    }

    pub fn all(&mut self, path: impl AsRef<str>, handler: impl FnHandler + Clone) -> &mut Self {
        self.router.all(path, handler);
        self
    }

    pub fn route(&mut self, path: impl Into<String>) -> Route<'_> {
        self.router.route_builder(path)
    }

    crate::define_methods!();

    fn add_route(
        &mut self,
        path: impl AsRef<str>,
        handler: impl FnHandler,
        method: MethodKind,
    ) -> &mut Self {
        self.router.route(path, handler, method);
        self
    }

    pub async fn listen<T, Fut>(self, port: u16, callback: T)
    where
        Self: Sized + Send + Sync + 'static,
        T: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let app = Arc::new(self);
        callback().await;

        let factory = move |addr: SocketAddr| {
            let app = app.clone();
            hyper::service::service_fn(move |mut req| {
                let app = app.clone();
                use crate::handler::request::RequestMetadataInternal;
                req.set_metadata(addr, false);
                async move {
                    let response = app.handle(req, ExpressResponse::default()).await;
                    Ok::<_, std::convert::Infallible>(response.into_hyper())
                }
            })
        };

        if let Err(e) = Server::bind(addr, factory).await {
            eprintln!("server error: {}", e);
        }
    }

    pub async fn listen_https<T, Fut>(self, port: u16, tls_config: ServerConfig, callback: T)
    where
        Self: Sized + Send + Sync + 'static,
        T: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let app = Arc::new(self);
        callback().await;

        let factory = move |addr: SocketAddr| {
            let app = app.clone();
            hyper::service::service_fn(move |mut req| {
                let app = app.clone();
                use crate::handler::request::RequestMetadataInternal;
                req.set_metadata(addr, true);
                async move {
                    let response = app.handle(req, ExpressResponse::default()).await;
                    Ok::<_, std::convert::Infallible>(response.into_hyper())
                }
            })
        };

        if let Err(e) = Server::bind_tls(addr, Arc::new(tls_config), factory).await {
            eprintln!("https server error: {}", e);
        }
    }
}
