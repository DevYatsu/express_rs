use crate::{
    router::interner::Symbol,
};
use dashmap::DashMap;
use hyper::{Request as HRequest, body::Incoming};
use smallvec::SmallVec;
use std::net::SocketAddr;
use std::sync::Arc;

/// Aliased request type for the framework.
pub type Request<B = Incoming> = HRequest<B>;

/// Wraps the client's socket address for injection into request extensions.
#[derive(Debug, Clone, Copy)]
pub struct ClientAddr(pub SocketAddr);

/// Wraps TLS connection status for injection into request extensions.
#[derive(Debug, Clone, Copy)]
pub struct TlsInfo {
    pub is_secure: bool,
}

/// Request-scoped state storage.
#[derive(Debug, Clone, Default)]
pub struct Locals(pub Arc<DashMap<String, serde_json::Value>>);

use async_trait::async_trait;
use http_body_util::BodyExt;

/// Extension trait for [`Request`] to provide Express.js-like properties.
#[async_trait]
pub trait RequestExt<B = Incoming> {
    fn params(&self) -> &RouteParams;
    fn path(&self) -> &str;
    fn query(&self, key: &str) -> Option<String>;
    fn get_header(&self, key: &str) -> Option<&str>;
    fn host_name(&self) -> Option<&str>;
    fn ip(&self) -> Option<SocketAddr>;
    fn xhr(&self) -> bool;
    fn is(&self, content_type_to_match: &str) -> bool;
    fn prefers_json(&self) -> bool;
    fn secure(&self) -> bool;
    fn locals(&self) -> &Locals;
    async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T, crate::handler::ResponseError>
    where
        B: BodyExt + Send + Unpin + 'static,
        B::Data: Send,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>> + std::fmt::Display;
}

/// Internal trait used to attach request metadata during server processing.
pub(crate) trait RequestMetadataInternal {
    fn set_params(&mut self, params: SmallVec<[(Symbol, Symbol); 4]>);
    fn set_metadata(&mut self, addr: SocketAddr, is_tls: bool);
}

#[async_trait]
impl<B> RequestExt<B> for Request<B> {
    fn params(&self) -> &RouteParams {
        self.extensions()
            .get::<RouteParams>()
            .expect("Route parameters must be set before accessing them")
    }

    fn path(&self) -> &str {
        self.uri().path()
    }

    fn query(&self, key: &str) -> Option<String> {
        self.uri().query().and_then(|q| {
            form_urlencoded::parse(q.as_bytes())
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.into_owned())
        })
    }

    fn get_header(&self, key: &str) -> Option<&str> {
        self.headers().get(key).and_then(|v| v.to_str().ok())
    }

    fn host_name(&self) -> Option<&str> {
        self.headers()
            .get(hyper::header::HOST)
            .and_then(|v| v.to_str().ok())
    }

    fn ip(&self) -> Option<SocketAddr> {
        self.extensions().get::<ClientAddr>().map(|addr| addr.0)
    }

    fn xhr(&self) -> bool {
        self.get_header("X-Requested-With")
            .map(|v| v.to_lowercase() == "xmlhttprequest")
            .unwrap_or(false)
    }

    fn is(&self, content_type_to_match: &str) -> bool {
        let content_type = self.get_header("Content-Type").unwrap_or("");
        if content_type
            .to_lowercase()
            .contains(&content_type_to_match.to_lowercase())
        {
            return true;
        }
        match content_type_to_match.to_lowercase().as_str() {
            "json" => content_type.to_lowercase().contains("application/json"),
            "html" => content_type.to_lowercase().contains("text/html"),
            "text" => content_type.to_lowercase().contains("text/plain"),
            _ => false,
        }
    }

    fn prefers_json(&self) -> bool {
        self.get_header("Accept")
            .map(|v| v.contains("application/json"))
            .unwrap_or(false)
    }

    fn secure(&self) -> bool {
        self.extensions()
            .get::<TlsInfo>()
            .map(|info| info.is_secure)
            .unwrap_or(false)
    }

    fn locals(&self) -> &Locals {
        self.extensions()
            .get::<Locals>()
            .expect("Locals must be initialized in App::handle")
    }

    async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T, crate::handler::ResponseError>
    where
        B: BodyExt + Send + Unpin + 'static,
        B::Data: Send,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>> + std::fmt::Display,
    {
        let body = self.into_body();
        let bytes = body
            .collect()
            .await
            .map_err(|e| {
                crate::handler::ResponseError::BodyReadError(e.to_string())
            })?
            .to_bytes();

        serde_json::from_slice(&bytes).map_err(crate::handler::ResponseError::JsonSerializationError)
    }
}

#[derive(Debug, Clone)]
pub struct RouteParams(SmallVec<[(Symbol, Symbol); 4]>);

impl RouteParams {
    /// Look up a route parameter by name.
    ///
    /// Returns `None` if the parameter does not exist.
    pub fn get(&self, key: &str) -> Option<&'static str> {
        use crate::router::interner::INTERNER;
        let sym_key = INTERNER.get(key)?;
        self.0
            .iter()
            .find(|(k, _)| *k == sym_key)
            .and_then(|(_, v)| INTERNER.resolve(*v))
    }
}

pub trait RequestState<B = Incoming> {
    fn get_state<S: Sync + Send + 'static>(&self) -> &Arc<S>;
    fn set_state<S: Sync + Send + 'static>(&mut self, state: S);
}

impl<B> RequestState<B> for Request<B> {
    fn get_state<S: Sync + Send + 'static>(&self) -> &Arc<S> {
        self.extensions()
            .get::<Arc<S>>()
            .expect("State must be registered before accessing it")
    }

    fn set_state<S: Sync + Send + 'static>(&mut self, state: S) {
        self.extensions_mut().insert(Arc::new(state));
    }
}

impl<B> RequestMetadataInternal for Request<B> {
    fn set_params(&mut self, params: SmallVec<[(Symbol, Symbol); 4]>) {
        self.extensions_mut().insert(RouteParams(params));
    }

    fn set_metadata(&mut self, addr: SocketAddr, is_tls: bool) {
        self.extensions_mut().insert(ClientAddr(addr));
        self.extensions_mut().insert(TlsInfo { is_secure: is_tls });
    }
}
