use crate::{
    handler::Response,
    router::interner::{INTERNER, Symbol},
};
// use ahash::HashMap;
use hyper::{Request as HRequest, body::Incoming};
use smallvec::SmallVec;
use std::sync::Arc;

/// Aliased request type for the framework.
///
/// Wraps a [`hyper::Request`] using the [`Incoming`] body type for compatibility with async streams.
pub type Request = HRequest<Incoming>;

/// Stores route parameters extracted from dynamic segments of a matched route.
///
/// Internally maps interned symbols to other interned symbols for compact, efficient key-value storage.
#[derive(Debug, Clone, Default)]
pub struct RouteParams(pub(crate) SmallVec<[(Symbol, Symbol); 4]>);

impl RouteParams {
    /// Gets the parameter value associated with the given key, if present.
    ///
    /// Resolves both the key and the value using the global symbol interner.
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
}

/// Internal trait used to attach route parameters to a request during routing.
pub(crate) trait RequestExtInternal {
    /// Sets the [`RouteParams`] using a small sequence of parameters.
    fn set_params(&mut self, params: SmallVec<[(Symbol, Symbol); 4]>);
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
        self.headers()
            .get(hyper::header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|accept| accept.contains("application/json"))
    }

    fn res(&self) -> Response {
        Response::new()
    }
}

impl RequestExtInternal for Request {
    fn set_params(&mut self, params: SmallVec<[(Symbol, Symbol); 4]>) {
        self.extensions_mut().insert(RouteParams(params));
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
