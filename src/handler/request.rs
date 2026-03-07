use crate::{
    handler::Response,
    router::interner::{INTERNER, Symbol},
};
use dashmap::DashMap;
use hyper::{Request as HRequest, body::Incoming};
use smallvec::SmallVec;
use std::net::SocketAddr;
use std::sync::Arc;

/// Aliased request type for the framework.
///
/// Wraps a [`hyper::Request`] using the [`Incoming`] body type for compatibility with async streams.
pub type Request = HRequest<Incoming>;

/// Wraps the client's socket address for injection into request extensions.
#[derive(Debug, Clone, Copy)]
pub struct ClientAddr(pub SocketAddr);

/// Wraps TLS connection status for injection into request extensions.
#[derive(Debug, Clone, Copy)]
pub struct TlsInfo(pub bool);

/// A collection of request-scoped data, similar to Express.js `res.locals`.
///
/// Uses a concurrent map to allow safe sharing across async boundaries if needed,
/// though typically accessed sequentially within the request lifecycle.
#[derive(Debug, Clone, Default)]
pub struct Locals(pub Arc<DashMap<String, serde_json::Value>>);

/// Stores route parameters extracted from dynamic segments of a matched route.
///
/// Internally maps interned symbols to other interned symbols for compact, efficient key-value storage.
#[derive(Debug, Clone, Default)]
pub struct RouteParams(pub(crate) SmallVec<[(Symbol, Symbol); 4]>);

impl RouteParams {
    /// Gets the parameter value associated with the given key, if present.
    pub fn get(&self, key: &str) -> Option<String> {
        let sym_key = INTERNER.get(key)?;
        let sym_val = self.0.iter().find(|(k, _)| *k == sym_key).map(|(_, v)| v)?;
        INTERNER.resolve(*sym_val).map(|s| s.to_owned())
    }

    /// Returns `true` if the given key exists in the parameter map.
    pub fn contains(&self, key: &str) -> bool {
        INTERNER
            .get(key)
            .is_some_and(|sym| self.0.iter().any(|(k, _)| *k == sym))
    }

    /// Returns the total number of parameters.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if there are no parameters.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over all parameters as resolved string key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.0.iter().filter_map(|(k, v)| {
            Some((
                INTERNER.resolve(*k)?.to_string(),
                INTERNER.resolve(*v)?.to_string(),
            ))
        })
    }
}

pub trait RequestExt {
    /// Returns the [`RouteParams`] associated with this request.
    fn params(&self) -> &RouteParams;

    /// Gets the path of the request URL (e.g., `/users/123`).
    fn path(&self) -> &str;

    /// Gets the value of a specific query parameter from the URL.
    fn query(&self, key: &str) -> Option<String>;

    /// Gets a specific HTTP header value by name.
    fn get_header(&self, key: &str) -> Option<&str>;

    /// Gets the host header or URL host.
    fn host_name(&self) -> Option<&str>;

    /// Determines whether the client prefers a JSON response based on the Accept header.
    fn prefers_json(&self) -> bool;

    /// Returns a new response object, allowing for chaining from the request.
    fn res(&self) -> Response;

    /// Gets the client IP address from the request extensions.
    fn ip(&self) -> Option<SocketAddr>;

    /// Returns `true` if the request was made via XMLHttpRequest (`X-Requested-With` header).
    fn xhr(&self) -> bool;

    /// Returns `true` if the `Content-Type` matches the specified type (case-insensitive).
    fn is(&self, content_type_to_match: &str) -> bool;

    /// Returns `true` if the connection is secure (TLS).
    fn secure(&self) -> bool;

    /// Returns the request-scoped locals map.
    fn locals(&self) -> &Locals;
}

/// Internal trait used to attach request metadata during server processing.
pub(crate) trait RequestMetadataInternal {
    fn set_params(&mut self, params: SmallVec<[(Symbol, Symbol); 4]>);
    fn set_metadata(&mut self, addr: SocketAddr, is_tls: bool);
}

impl RequestExt for Request {
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
            .or_else(|| self.uri().host())
    }

    fn prefers_json(&self) -> bool {
        self.get_header("accept")
            .is_none_or(|accept| accept.contains("application/json"))
    }

    fn res(&self) -> Response {
        Response::new()
    }

    fn ip(&self) -> Option<SocketAddr> {
        self.extensions().get::<ClientAddr>().map(|c| c.0)
    }

    fn xhr(&self) -> bool {
        self.get_header("x-requested-with")
            .is_some_and(|v| v.eq_ignore_ascii_case("xmlhttprequest"))
    }

    fn is(&self, content_type_to_match: &str) -> bool {
        self.get_header("content-type").is_some_and(|v| {
            v.to_lowercase()
                .contains(&content_type_to_match.to_lowercase())
        })
    }

    fn secure(&self) -> bool {
        self.extensions().get::<TlsInfo>().is_some_and(|t| t.0)
    }

    fn locals(&self) -> &Locals {
        if self.extensions().get::<Locals>().is_none() {
            // This is a bit tricky since we need &self and want to insert if missing.
            // Normally locals would be initialized at the start of the request handling.
            panic!("Locals must be initialized at the start of the request lifecycle");
        }
        self.extensions().get::<Locals>().unwrap()
    }
}

impl RequestMetadataInternal for Request {
    fn set_params(&mut self, params: SmallVec<[(Symbol, Symbol); 4]>) {
        self.extensions_mut().insert(RouteParams(params));
    }

    fn set_metadata(&mut self, addr: SocketAddr, is_tls: bool) {
        self.extensions_mut().insert(ClientAddr(addr));
        self.extensions_mut().insert(TlsInfo(is_tls));
    }
}

pub trait RequestState {
    fn get_state<S: Sync + Send + 'static>(&self) -> &Arc<S>;
    fn set_state<S: Sync + Send + 'static>(&mut self, state: S);
}

impl RequestState for Request {
    fn get_state<S: Sync + Send + 'static>(&self) -> &Arc<S> {
        self.extensions()
            .get::<Arc<S>>()
            .expect("State must be set before accessing it")
    }

    fn set_state<S: Sync + Send + 'static>(&mut self, state: S) {
        self.extensions_mut().insert(Arc::new(state));
    }
}
