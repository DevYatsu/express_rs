use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub(crate) struct Symbol(pub u32);

/// Global interner.
pub(crate) static INTERNER: Lazy<Interner> = Lazy::new(Interner::default);

/// Interning structure with fast concurrent read/write and reverse lookup.
pub(crate) struct Interner {
    /// Map from string slice to symbol.
    forward: DashMap<&'static str, Symbol>,
    /// Reverse lookup by index (Symbol.0).
    reverse: RwLock<Vec<&'static str>>,
    /// Atomic counter to assign unique symbols.
    counter: AtomicU32,
}

impl Default for Interner {
    fn default() -> Self {
        Self {
            forward: DashMap::new(),
            reverse: RwLock::new(Vec::new()),
            counter: AtomicU32::new(0),
        }
    }
}

impl Interner {
    /// Interns a string or returns its existing symbol.
    pub fn get_or_intern(&self, s: &str) -> Symbol {
        if let Some(sym) = self.forward.get(s) {
            return *sym;
        }

        let sym = Symbol(self.counter.fetch_add(1, Ordering::Relaxed));
        let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
        self.forward.insert(leaked, sym);

        let mut reverse = self.reverse.write().unwrap();
        if sym.0 as usize >= reverse.len() {
            reverse.push(leaked);
        } else {
            reverse[sym.0 as usize] = leaked;
        }

        sym
    }

    /// Gets an existing symbol for a string, if it exists.
    pub fn get(&self, s: &str) -> Option<Symbol> {
        self.forward.get(s).map(|s| *s)
    }

    /// Resolves a symbol back into a string.
    pub fn resolve(&self, sym: Symbol) -> Option<&'static str> {
        self.reverse.read().unwrap().get(sym.0 as usize).copied()
    }
}
