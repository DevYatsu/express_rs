use crate::handler::{Handler, Request, Response};
use crate::middleware::Middleware;
use crate::router::{MethodKind, Route, Router};
use crate::server::Server;
use hyper::body::Incoming;

use std::net::SocketAddr;
use std::sync::Arc;
use tokio_rustls::rustls::ServerConfig;

/// The main application structure for `express_rs`.
pub struct App<B: Send + 'static = Incoming> {
    /// The inner router used by the application to match and handle paths.
    pub router: Router<B>,
}

impl<B: Send + 'static> Default for App<B> {
    fn default() -> Self {
        Self {
            router: Router::default(),
        }
    }
}

impl<B: Send + 'static> App<B> {
    /// Handles an incoming request and returns a response.
    /// This method is typically called internally but is exposed for custom integrations.
    pub async fn handle(&self, req: Request<B>, res: Response) -> Response {
        let mut req = req;
        req.extensions_mut()
            .insert(crate::handler::request::Locals::default());
        self.router.handle(req, res).await
    }

    /// Attaches a middleware to a specific path prefix.
    pub fn use_with(&mut self, path: impl AsRef<str>, middleware: impl Middleware<B>) -> &mut Self {
        self.router.use_with(path, middleware);
        self
    }

    /// Mounts another `Router` at the specified path prefix.
    pub fn use_router(&mut self, path: impl AsRef<str>, router: Router<B>) -> &mut Self {
        self.router.use_router(path, router);
        self
    }

    /// Sets a generic handler when no route matches the requested path.
    pub fn not_found<F, Fut>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(Request<B>, Response) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        self.router.not_found(handler);
        self
    }

    /// Registers a handler for all HTTP methods on the specified path.
    pub fn all<F, Fut>(&mut self, path: impl AsRef<str>, handler: F) -> &mut Self
    where
        F: Fn(Request<B>, Response) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        self.router.all(path, handler);
        self
    }

    /// Creates a route builder for the specified path, allowing chainable handler registrations.
    pub fn route(&mut self, path: impl AsRef<str>) -> Route<'_, B> {
        self.router.route_builder(path)
    }

    crate::define_methods!();

    fn add_route(
        &mut self,
        path: impl AsRef<str>,
        handler: impl Handler<B>,
        method: MethodKind,
    ) -> &mut Self {
        self.router.route(path, handler, method);
        self
    }
}

// listen only for Incoming
impl App<Incoming> {
    /// Binds the HTTP server to the given port and invokes the callback once ready.
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
                    let response = app.handle(req, Response::new()).await;
                    Ok::<_, std::convert::Infallible>(response.into_hyper())
                }
            })
        };

        if let Err(e) = Server::bind(addr, factory).await {
            eprintln!("server error: {}", e);
        }
    }

    /// Binds the HTTPS server to the given port using a provided TLS configuration, and invokes the callback once ready.
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
                    let response = app.handle(req, Response::new()).await;
                    Ok::<_, std::convert::Infallible>(response.into_hyper())
                }
            })
        };

        if let Err(e) = Server::bind_tls(addr, Arc::new(tls_config), factory).await {
            eprintln!("https server error: {}", e);
        }
    }
}

impl<B: Send + 'static> std::fmt::Debug for App<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App").field("router", &self.router).finish()
    }
}
