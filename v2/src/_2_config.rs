//! Config signal. Content-hashed. Reactive apply comes later.

use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Config {
    pub repos:        Vec<Arc<str>>,
    pub revs:         Vec<Arc<str>>,
    pub fs_exclude:   Vec<Arc<str>>,
    pub sprf_files:   Vec<Arc<str>>,
    pub shell_allow:  Vec<Arc<str>>,
    pub runtime:      RuntimeConfig,
    pub content_hash: u64,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub worker_threads:     u16,
    pub buffer_size:        u32,
    pub flush_interval_ms:  u32,
    /// When true, framework appends `OpEvidence` entries to each cursor as
    /// it flows through ops that implement `Op::witness`. LSP needs it; CLI
    /// runs can disable for zero overhead.
    pub collect_witnesses:  bool,
    /// Soft cap on distinct cross-ref tuples per cursor expansion.
    /// Hitting it drops the cursor + emits `xref/cartesian-limit` once
    /// per op. Default 10_000.
    pub xref_cartesian_limit: usize,
}

#[derive(Debug, Clone)]
pub struct ConfigDiff {
    pub changed_fields: Vec<&'static str>,
    pub old_hash:       u64,
    pub new_hash:       u64,
}

impl Config {
    /// Compute content hash. Impl lives in a later tier; signature fixed here.
    pub fn recompute_hash(&mut self) { /* impl pending */ }
}
