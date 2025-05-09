use ahash::HashMap;
use hyper::{Request as HRequest, body::Incoming};
use std::sync::Arc;

/// Aliased request type.
pub type Request = HRequest<Incoming>;

/// Wrapper type for route parameters.
#[derive(Debug, Clone, Default)]
pub struct RouteParams(HashMap<Arc<str>, Arc<str>>);

impl RouteParams {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_ref())
    }

    pub fn contains(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_ref(), v.as_ref()))
    }

    pub fn into_inner(self) -> HashMap<Arc<str>, Arc<str>> {
        self.0
    }

    pub fn to_map(&self) -> HashMap<Arc<str>, Arc<str>> {
        self.0.clone()
    }
}

pub trait RequestExt {
    fn params(&self) -> &RouteParams;
}

pub(crate) trait RequestExtInternal {
    fn set_params(&mut self, params: HashMap<Arc<str>, Arc<str>>);
}

impl RequestExt for Request {
    fn params(&self) -> &RouteParams {
        self.extensions()
            .get::<RouteParams>()
            .expect("Route parameters must be set before accessing them")
    }
}

impl RequestExtInternal for Request {
    fn set_params(&mut self, params: HashMap<Arc<str>, Arc<str>>) {
        let converted = params
            .into_iter()
            .map(|(k, v)| (Arc::from(k), Arc::from(v)))
            .collect();

        self.extensions_mut().insert(RouteParams(converted));
    }
}
