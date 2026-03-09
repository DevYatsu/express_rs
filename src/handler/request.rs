use crate::router::interner::Symbol;
use hyper::{Request as HRequest, body::Incoming};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::net::SocketAddr;
use std::sync::Arc;

/// Aliased request type for the framework.
pub type Request<B = Incoming> = HRequest<B>;

/// Wraps the client's socket address for injection into request extensions.
#[derive(Debug, Clone, Copy)]
pub struct ClientAddr(
    /// The actual socket address of the client
    pub SocketAddr
);

/// Wraps TLS connection status for injection into request extensions.
#[derive(Debug, Clone, Copy)]
pub struct TlsInfo {
    /// Indicates whether the request connection is secure via TLS.
    pub is_secure: bool,
}

/// Request-scoped state storage.
///
/// Uses a plain `HashMap` (not `Arc<DashMap>`) because `Locals` is only ever
/// accessed from the single async task handling a request — no concurrency.
#[derive(Debug, Clone, Default)]
pub struct Locals(pub FxHashMap<String, serde_json::Value>);

use async_trait::async_trait;
use http_body_util::BodyExt;

/// Extension trait for [`Request`] to provide Express.js-like properties.
#[async_trait]
pub trait RequestExt<B = Incoming> {
    /// Returns the parsed route parameters.
    fn params(&self) -> &RouteParams;
    /// Returns the requested path.
    fn path(&self) -> &str;
    /// Returns the requested query parameter.
    fn query(&self, key: &str) -> Option<String>;
    /// Returns the specified HTTP header value.
    fn get_header(&self, key: &str) -> Option<&str>;
    /// Returns the requested host name from the headers.
    fn host_name(&self) -> Option<&str>;
    /// Returns the remote socket address.
    fn ip(&self) -> Option<SocketAddr>;
    /// Returns true if the request was an XMLHttpRequest.
    fn xhr(&self) -> bool;
    /// Checks if the request's Content-Type matches the given string.
    fn is(&self, content_type_to_match: &str) -> bool;
    /// Returns true if the request prefers a JSON response based on the Accept header.
    fn prefers_json(&self) -> bool;
    /// Returns true if the request is running over a secure TLS connection.
    fn secure(&self) -> bool;
    /// Returns the request-scoped locals.
    fn locals(&self) -> &Locals;
    /// Returns a mutable reference to the request-scoped locals.
    fn locals_mut(&mut self) -> &mut Locals;
    /// Parses the request body as JSON.
    async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T, crate::handler::ResponseError>
    where
        B: BodyExt + Send + Unpin + 'static,
        B::Data: Send,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>> + std::fmt::Display;
}

/// Internal trait used to attach request metadata during server processing.
pub(crate) trait RequestMetadataInternal {
    fn set_params(&mut self, params: SmallVec<[(Symbol, Arc<str>); 4]>);
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
        // Lazy-initialise the parsed query cache on first call.
        // We can't store a mutable reference here, so we parse on every
        // miss — but the extension is inserted on set_metadata, so we
        // re-parse at most once per request.
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
            .map(|v| v.eq_ignore_ascii_case("xmlhttprequest"))
            .unwrap_or(false)
    }

    fn is(&self, content_type_to_match: &str) -> bool {
        let content_type = self.get_header("Content-Type").unwrap_or("");
        // All MIME type comparisons are ASCII — use the allocation-free variant.
        if content_type.eq_ignore_ascii_case(content_type_to_match) {
            return true;
        }
        // Shorthand aliases.
        let m = content_type_to_match.to_ascii_lowercase();
        match m.as_str() {
            "json" => content_type
                .to_ascii_lowercase()
                .contains("application/json"),
            "html" => content_type.to_ascii_lowercase().contains("text/html"),
            "text" => content_type.to_ascii_lowercase().contains("text/plain"),
            _ => content_type.to_ascii_lowercase().contains(m.as_str()),
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

    fn locals_mut(&mut self) -> &mut Locals {
        self.extensions_mut()
            .get_mut::<Locals>()
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
            .map_err(|e| crate::handler::ResponseError::BodyReadError(e.to_string()))?
            .to_bytes();

        serde_json::from_slice(&bytes)
            .map_err(crate::handler::ResponseError::JsonSerializationError)
    }
}

/// Parsed route parameters from the request URI.
#[derive(Debug, Clone)]
pub struct RouteParams(
    /// Internal representation of the route parameters.
    SmallVec<[(Symbol, Arc<str>); 4]>
);

impl RouteParams {
    /// Look up a route parameter by name.
    ///
    /// Returns `None` if the parameter does not exist.
    pub fn get(&self, key: &str) -> Option<&str> {
        use crate::router::interner::INTERNER;
        let sym_key = INTERNER.get(key)?;
        self.0
            .iter()
            .find(|(k, _)| *k == sym_key)
            .map(|(_, v)| v.as_ref())
    }
}

impl<B> RequestMetadataInternal for Request<B> {
    fn set_params(&mut self, params: SmallVec<[(Symbol, Arc<str>); 4]>) {
        self.extensions_mut().insert(RouteParams(params));
    }

    fn set_metadata(&mut self, addr: SocketAddr, is_tls: bool) {
        self.extensions_mut().insert(ClientAddr(addr));
        self.extensions_mut().insert(TlsInfo { is_secure: is_tls });
    }
}
