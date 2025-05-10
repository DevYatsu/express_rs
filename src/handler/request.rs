use crate::router::interner::{INTERNER, Symbol};
use ahash::HashMap;
use hyper::{Request as HRequest, body::Incoming};

/// Aliased request type.
pub type Request = HRequest<Incoming>;

/// Wrapper type for route parameters.
#[derive(Debug, Clone, Default)]
pub struct RouteParams(HashMap<Symbol, Symbol>);

impl RouteParams {
    /// Get the parameter value by string key, if it exists.
    pub fn get(&self, key: &str) -> Option<String> {
        let sym_key = INTERNER.get(key)?;
        let sym_val = self.0.get(&sym_key)?;
        INTERNER.resolve(*sym_val).map(|s| s.to_owned())
    }

    /// Check if a parameter with the given key exists.
    pub fn contains(&self, key: &str) -> bool {
        INTERNER
            .get(key)
            .map_or(false, |sym| self.0.contains_key(&sym))
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
        self.0.iter().filter_map(|(k, v)| {
            Some((
                INTERNER.resolve(*k)?.to_string(),
                INTERNER.resolve(*v)?.to_string(),
            ))
        })
    }
}

/// Public trait to access route parameters.
pub trait RequestExt {
    fn params(&self) -> &RouteParams;
}

/// Internal trait to set route parameters from a router.
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
