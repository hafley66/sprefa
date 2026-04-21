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
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;

pub struct StrInterner {
    table:  DashMap<Arc<str>, ()>,
    hits:   AtomicU64,
    misses: AtomicU64,
}

impl StrInterner {
    pub fn new() -> Self {
        Self {
            table:  DashMap::new(),
            hits:   AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Return the canonical `Arc<str>` for `s`. First caller allocates;
    /// subsequent callers clone the shared Arc.
    pub fn get(&self, s: &str) -> Arc<str> {
        if let Some(e) = self.table.get(s) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return e.key().clone();
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
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

    /// Observability: total hits, misses, unique entries.
    pub fn stats(&self) -> (u64, u64, usize) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
            self.table.len(),
        )
    }

    /// Swap-reset hit/miss counters. Unique entry count is not reset
    /// (the table itself persists across runs within a workspace).
    pub fn stats_snapshot(&self) -> (u64, u64, usize) {
        (
            self.hits.swap(0, Ordering::Relaxed),
            self.misses.swap(0, Ordering::Relaxed),
            self.table.len(),
        )
    }
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

    #[test]
    fn hit_miss_counters_track_dedup_ratio() {
        let i = StrInterner::new();
        i.get("a");        // miss
        i.get("b");        // miss
        i.get("a");        // hit
        i.get("a");        // hit
        i.get("c");        // miss
        let (h, m, u) = i.stats();
        assert_eq!(u, 3);
        assert_eq!(m, 3);
        assert_eq!(h, 2);
    }

    #[test]
    fn concurrent_get_dedups_under_race() {
        use std::thread;
        let i = Arc::new(StrInterner::new());
        let mut handles = vec![];
        for _ in 0..8 {
            let i2 = i.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    let _ = i2.get("shared");
                }
            }));
        }
        for h in handles { h.join().unwrap(); }
        // 8 × 1000 = 8000 calls; exactly one unique string regardless
        // of how the misses race on the first call.
        assert_eq!(i.len(), 1);
        let (h, m, _) = i.stats();
        assert_eq!(h + m, 8000);
        // At most `threads` misses if all race the empty table; more
        // commonly just 1. Tolerant upper bound.
        assert!(m <= 8, "misses should coalesce, got {}", m);
    }
}
