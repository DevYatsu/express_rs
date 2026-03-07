use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, body::Incoming};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::signal;

use std::sync::Arc;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;

pub(crate) type ServerResponse = hyper::Response<
    http_body_util::combinators::BoxBody<hyper::body::Bytes, std::convert::Infallible>,
>;

pub(crate) struct Server;

impl Server {
    pub async fn bind<F, S>(
        addr: SocketAddr,
        make_service: F,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        F: Fn() -> S + Send + Sync + 'static + Clone,
        S: Service<Request<Incoming>, Response = ServerResponse, Error = Infallible>
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        let listener = TcpListener::bind(addr).await?;
        Self::run(listener, make_service, |stream| async move {
            Ok(TokioIo::new(stream))
        })
        .await
    }

    pub async fn bind_tls<F, S>(
        addr: SocketAddr,
        tls_config: Arc<ServerConfig>,
        make_service: F,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        F: Fn() -> S + Send + Sync + 'static + Clone,
        S: Service<Request<Incoming>, Response = ServerResponse, Error = Infallible>
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        let listener = TcpListener::bind(addr).await?;
        let tls_acceptor = TlsAcceptor::from(tls_config);
        Self::run(listener, make_service, move |stream| {
            let tls_acceptor = tls_acceptor.clone();
            async move {
                let tls_stream = tls_acceptor.accept(stream).await?;
                Ok(TokioIo::new(tls_stream))
            }
        })
        .await
    }

    async fn run<F, S, A, Fut, I>(
        listener: TcpListener,
        make_service: F,
        acceptor: A,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        F: Fn() -> S + Send + Sync + 'static + Clone,
        S: Service<Request<Incoming>, Response = ServerResponse, Error = Infallible>
            + Send
            + 'static,
        S::Future: Send + 'static,
        A: Fn(tokio::net::TcpStream) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<I, Box<dyn std::error::Error + Send + Sync>>>
            + Send
            + 'static,
        I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
    {
        let mut shutdown = tokio::spawn(async {
            signal::ctrl_c().await.expect("failed to listen for ctrl_c");
            log::info!("🛑 Received Ctrl+C, shutting down server...");
        });

        loop {
            tokio::select! {
                Ok((stream, _)) = listener.accept() => {
                    let service = make_service();
                    let fut = acceptor(stream);

                    tokio::spawn(async move {
                        match fut.await {
                            Ok(io) => {
                                if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                                    log::error!("Connection error: {}", err);
                                }
                            }
                            Err(err) => {
                                log::error!("Handshake failed: {}", err);
                            }
                        }
                    });
                }
                _ = &mut shutdown => {
                    break;
                }
            }
        }

        Ok(())
    }
}
