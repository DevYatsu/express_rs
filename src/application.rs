use crate::handler::{Request, Response, Handler};
use crate::middleware::Middleware;
use crate::router::{MethodKind, Route, Router};
use crate::server::Server;
use hyper::body::Incoming;
use log::info;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_rustls::rustls::ServerConfig;

/// The main application structure for `express_rs`.
pub struct App<S: Sync + Send + 'static = (), B: Send + 'static = Incoming> {
    pub router: Router<B>,
    state: Arc<S>,
}

impl<S: Sync + Send + 'static + Default, B: Send + 'static> Default for App<S, B> {
    fn default() -> Self {
        Self {
            router: Router::default(),
            state: Arc::new(S::default()),
        }
    }
}

impl<S: Sync + Send + 'static, B: Send + 'static> App<S, B> {
    pub fn with_state(state: S) -> Self {
        Self {
            router: Router::default(),
            state: Arc::new(state),
        }
    }

    pub fn with_new_state<T: Sync + Send + 'static>(self, state: T) -> App<T, B> {
        App {
            router: self.router,
            state: Arc::new(state),
        }
    }

    pub fn state(&self) -> &S {
        &self.state
    }

    pub async fn handle(&self, req: Request<B>, res: Response) -> Response {
        let mut req = req;
        req.extensions_mut().insert(self.state.clone());
        req.extensions_mut()
            .insert(crate::handler::request::Locals::default());
        self.router.handle(req, res).await
    }

    pub fn use_with(&mut self, path: impl AsRef<str>, middleware: impl Middleware<B>) -> &mut Self {
        info!("Adding middleware to path: {}", path.as_ref());
        self.router.use_with(path, middleware);
        self
    }

    pub fn use_router(&mut self, path: impl AsRef<str>, router: Router<B>) -> &mut Self {
        self.router.use_router(path, router);
        self
    }

    pub fn not_found(&mut self, handler: impl Handler<B>) -> &mut Self {
        self.router.not_found(handler);
        self
    }

    pub fn all(&mut self, path: impl AsRef<str>, handler: impl Handler<B> + Clone) -> &mut Self {
        self.router.all(path, handler);
        self
    }

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
impl<S: Sync + Send + 'static> App<S, Incoming> {
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

impl<S: Sync + Send + 'static, B: Send + 'static> std::fmt::Debug for App<S, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App").field("router", &self.router).finish()
    }
}
