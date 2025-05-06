use hyper::{Request as HRequest, body::Incoming};
use std::collections::HashMap;

/// Aliased request type.
pub type Request = HRequest<Incoming>;

/// Wrapper type for route parameters.
#[derive(Debug, Clone, Default)]
pub struct RouteParams(HashMap<String, String>);

impl RouteParams {
    /// Returns a parameter by key as `Option<&str>`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// Returns `true` if the specified key exists.
    pub fn contains(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Returns the number of stored parameters.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if there are no parameters.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over `(key, value)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }

    /// Converts to the underlying `HashMap`.
    pub fn into_inner(self) -> HashMap<String, String> {
        self.0
    }

    /// Returns a clone of the internal `HashMap`.
    pub fn to_map(&self) -> HashMap<String, String> {
        self.0.clone()
    }
}

/// Public extension trait for accessing route params on a request.
pub trait RequestExt {
    /// Gets route parameters.  
    ///
    /// # Panics
    /// Panics if params haven't been set yet—this is a programming error in the framework.
    fn params(&self) -> &RouteParams;
}

/// Internal-only trait for setting route parameters.
pub(crate) trait RequestExtInternal {
    /// Sets route parameters on a request.
    fn set_params(&mut self, params: HashMap<String, String>);
}

impl RequestExt for Request {
    fn params(&self) -> &RouteParams {
        self.extensions()
            .get::<RouteParams>()
            .expect("Route parameters must be set before accessing them")
    }
}

impl RequestExtInternal for Request {
    fn set_params(&mut self, params: HashMap<String, String>) {
        self.extensions_mut().insert(RouteParams(params));
    }
}
