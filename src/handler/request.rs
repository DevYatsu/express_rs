use crate::router::interner::{INTERNER, Symbol};
use ahash::HashMap;
use async_trait::async_trait;
use hyper::{Request as HRequest, body::Incoming};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Aliased request type for the framework.
///
/// Wraps a [`hyper::Request`] using the [`Incoming`] body type for compatibility with async streams.
pub type Request = HRequest<Incoming>;

/// Stores route parameters extracted from dynamic segments of a matched route.
///
/// Internally maps interned symbols to other interned symbols for compact, efficient key-value storage.
#[derive(Debug, Clone, Default)]
pub struct RouteParams(HashMap<Symbol, Symbol>);

impl RouteParams {
    /// Gets the parameter value associated with the given key, if present.
    ///
    /// Resolves both the key and the value using the global symbol interner.
    pub fn get(&self, key: &str) -> Option<String> {
        let sym_key = INTERNER.get(key)?;
        let sym_val = self.0.get(&sym_key)?;
        INTERNER.resolve(*sym_val).map(|s| s.to_owned())
    }

    /// Returns `true` if the given key exists in the parameter map.
    pub fn contains(&self, key: &str) -> bool {
        INTERNER
            .get(key)
            .map_or(false, |sym| self.0.contains_key(&sym))
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

/// Extension trait to access route parameters from a [`Request`] object.
pub trait RequestExt {
    /// Returns the [`RouteParams`] associated with this request.
    fn params(&self) -> &RouteParams;
}

/// Internal trait used to attach route parameters to a request during routing.
pub(crate) trait RequestExtInternal {
    /// Sets the [`RouteParams`] using a raw `HashMap<Symbol, Symbol>`.
    fn set_params(&mut self, params: HashMap<Symbol, Symbol>);
}

impl RequestExt for Request {
    fn params(&self) -> &RouteParams {
        self.extensions()
            .get::<RouteParams>()
            .expect("Route parameters must be set before accessing them")
    }
}

impl RequestExtInternal for Request {
    fn set_params(&mut self, params: HashMap<Symbol, Symbol>) {
        self.extensions_mut().insert(RouteParams(params));
    }
}

#[async_trait]
pub trait RequestState {
    async fn get_state<S: Sync + Send + 'static>(&self) -> &Arc<S>;
    fn set_state<S: Sync + Send + 'static>(&mut self, state: S);
}

#[async_trait]
impl RequestState for Request {
    async fn get_state<S: Sync + Send + 'static>(&self) -> &Arc<S> {
        let arc = self
            .extensions()
            .get::<Arc<S>>()
            .expect("State must be set before accessing it");

        arc
    }

    fn set_state<S: Sync + Send + 'static>(&mut self, state: S) {
        self.extensions_mut().insert(Arc::new(RwLock::new(state)));
    }
}
