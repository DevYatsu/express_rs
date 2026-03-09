use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub(crate) struct Symbol(pub u32);

/// Global interner.
pub(crate) static INTERNER: Lazy<Interner> = Lazy::new(Interner::default);

/// Interning structure with fast concurrent read/write.
///
/// # Memory note
/// Interned strings are leaked via `Box::leak` so they gain `'static` lifetime.
/// In practice only route-parameter *names* (e.g. `"id"`, `"slug"`) are
/// interned — a small, bounded set that is fixed at startup.
pub(crate) struct Interner {
    /// Map from `&'static str` → symbol.
    forward: DashMap<&'static str, Symbol>,
    /// Atomic counter — each new string gets a unique id.
    counter: AtomicU32,
}

impl Default for Interner {
    fn default() -> Self {
        Self {
            forward: DashMap::new(),
            counter: AtomicU32::new(0),
        }
    }
}

impl Interner {
    /// Interns a string or returns its existing symbol.
    ///
    /// ## Race-free design
    /// We use DashMap's `entry` API which holds the shard write-lock for the
    /// duration of `or_insert_with`, eliminating the TOCTOU window that existed
    /// between a bare `get` + `insert`. The fast-path `get` before `entry` is
    /// an optimistic short-circuit for the common case where the key is already
    /// present (read-locks only, cheaper).
    pub fn get_or_intern(&self, s: &str) -> Symbol {
        // Fast path: already interned — acquire only a read-shard lock.
        if let Some(sym) = self.forward.get(s) {
            return *sym;
        }

        // Slow path: atomically check-and-insert inside the shard write-lock.
        // Leaking here is intentional: route-param names are a bounded set.
        let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
        *self
            .forward
            .entry(leaked)
            .or_insert_with(|| Symbol(self.counter.fetch_add(1, Ordering::Relaxed)))
    }

    /// Gets an existing symbol for a string, if it exists.
    pub fn get(&self, s: &str) -> Option<Symbol> {
        self.forward.get(s).map(|s| *s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interner_basic() {
        let interner = Interner::default();
        let sym1 = interner.get_or_intern("test");
        let sym2 = interner.get_or_intern("test");
        let sym3 = interner.get_or_intern("other");

        assert_eq!(sym1, sym2);
        assert_ne!(sym1, sym3);
    }

    #[test]
    fn test_interner_get() {
        let interner = Interner::default();
        assert_eq!(interner.get("test"), None);

        let sym1 = interner.get_or_intern("test");
        assert_eq!(interner.get("test"), Some(sym1));
    }

    #[test]
    fn test_global_interner() {
        let sym1 = INTERNER.get_or_intern("global");
        let sym2 = INTERNER.get_or_intern("global");
        assert_eq!(sym1, sym2);
    }
}
