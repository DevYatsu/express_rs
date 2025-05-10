use ahash::HashMap;
use hyper::{Request as HRequest, body::Incoming};
use crate::router::{Symbol, INTERNER};

/// Aliased request type.
pub type Request = HRequest<Incoming>;

/// Wrapper type for route parameters.
#[derive(Debug, Clone, Default)]
pub struct RouteParams(HashMap<Symbol, Symbol>);

impl RouteParams {
    /// Get the parameter value by string key, if it exists.
    pub fn get(&self, key: &str) -> Option<String> {
        let interner = INTERNER.read().unwrap();
        let sym = interner.get(key)?;
        let val = self.0.get(&sym)?;
        interner.resolve(*val).map(|s| s.to_owned())
    }

    /// Check if a parameter with the given key exists.
    pub fn contains(&self, key: &str) -> bool {
        let interner = INTERNER.read().unwrap();
        interner.get(key).map_or(false, |sym| self.0.contains_key(&sym))
    }

    /// Return the number of parameters.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if there are no parameters.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over all parameters resolved to strings.
    pub fn iter(&self) -> impl Iterator<Item = (String, String)> + '_ {
        let interner = INTERNER.read().unwrap();
        self.0.iter().filter_map(move |(k, v)| {
            Some((
                interner.resolve(*k)?.to_owned(),
                interner.resolve(*v)?.to_owned(),
            ))
        })
    }

    /// Take ownership of the underlying symbol map.
    pub fn into_inner(self) -> HashMap<Symbol, Symbol> {
        self.0
    }

    /// Clone the internal symbol map.
    pub fn to_map(&self) -> HashMap<Symbol, Symbol> {
        self.0.clone()
    }
}

pub trait RequestExt {
    fn params(&self) -> &RouteParams;
}

pub(crate) trait RequestExtInternal {
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