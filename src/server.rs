use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, body::Incoming};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub struct Server;

impl Server {
    pub async fn bind<F, S>(
        addr: SocketAddr,
        make_service: F,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        F: Fn() -> S + Send + Sync + 'static + Clone,
        S: Service<Request<Incoming>, Response = Response<Full<Bytes>>, Error = Infallible>
            + Send
            + 'static,
        S::Future: Send + 'static,
    {
        let listener = TcpListener::bind(addr).await?;

        loop {
            let (stream, _) = listener.accept().await?;
            let service = make_service();
            let io = TokioIo::new(stream);

            tokio::spawn(async move {
                if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                    eprintln!("Connection error: {}", err);
                }
            });
        }
    }
}
