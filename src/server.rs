use hyper::body::{Body, Bytes};
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, body::Incoming};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use tokio::net::TcpListener;
use tokio::signal;

use crate::handler::response::error::ResponseError;

pub(crate) struct Server;

impl Server {
    pub async fn bind<F, S>(
        addr: SocketAddr,
        make_service: F,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        F: Fn() -> S + Send + Sync + 'static + Clone,
        S: Service<
                Request<Incoming>,
                Response = Response<Pin<Box<dyn Body<Data = Bytes, Error = ResponseError> + Send>>>,
                Error = Infallible,
            > + Send
            + 'static,
        S::Future: Send + 'static,
    {
        let listener = TcpListener::bind(addr).await?;

        let mut shutdown = tokio::spawn(async {
            signal::ctrl_c().await.expect("failed to listen for ctrl_c");
            log::info!("🛑 Received Ctrl+C, shutting down server...");
        });

        loop {
            tokio::select! {
                Ok((stream, _)) = listener.accept() => {
                    let service = make_service();
                    let io = TokioIo::new(stream);

                    tokio::spawn(async move {
                        if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                            log::error!("Connection error: {}", err);
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
