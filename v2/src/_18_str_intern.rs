//! StrInterner — per-workspace `Arc<str>` dedup table.
//!
//! Lives on `WorkspaceCtx`. Readers and ops that build keys from
//! `&str` values (repo, rev, path) look up here first; repeat strings
//! return the same `Arc<str>`, so downstream:
//!   - cursor clone = refcount bump, no string copy
//!   - hashmap keys compare by pointer first (if hashed consistently)
//!   - RAM growth is bounded by unique string cardinality, not call count
//!
//! DashMap for lock-free concurrent reads; write-on-miss takes a
//! short lock on one shard. Default shard count (64) is enough for
//! the expected workload — hundreds to low thousands of unique
//! strings per workspace.

use std::sync::Arc;

use dashmap::DashMap;

pub struct StrInterner {
    table: DashMap<Arc<str>, ()>,
}

impl StrInterner {
    pub fn new() -> Self {
        Self { table: DashMap::new() }
    }

    /// Return the canonical `Arc<str>` for `s`. First caller allocates;
    /// subsequent callers clone the shared Arc.
    pub fn get(&self, s: &str) -> Arc<str> {
        if let Some(e) = self.table.get(s) {
            return e.key().clone();
        }
        // Miss: allocate once and insert. Another thread may race and
        // insert first; `entry` coalesces.
        let arc: Arc<str> = Arc::from(s);
        self.table
            .entry(arc.clone())
            .or_insert(());
        arc
    }

    pub fn len(&self) -> usize { self.table.len() }
    pub fn is_empty(&self) -> bool { self.table.is_empty() }
}

impl Default for StrInterner {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_returns_same_arc_ptr() {
        let i = StrInterner::new();
        let a = i.get("hello");
        let b = i.get("hello");
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(i.len(), 1);
    }

    #[test]
    fn distinct_strings_distinct_arcs() {
        let i = StrInterner::new();
        let a = i.get("a");
        let b = i.get("b");
        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(i.len(), 2);
    }
}
