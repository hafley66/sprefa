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
    pub max_passes:           usize,
    pub max_claims_per_pass:  usize,
    pub max_cursors_per_root: usize,
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

    /// Fully-populated empty config for tests, bins, and smoke runs. Override
    /// fields via struct-update: `Config { repos, ..Config::test_default() }`.
    pub fn test_default() -> Config {
        Config {
            repos:        vec![],
            revs:         vec![],
            fs_exclude:   vec![],
            sprf_files:   vec![],
            shell_allow:  vec![],
            runtime:      RuntimeConfig::test_default(),
            content_hash: 0,
        }
    }
}

impl RuntimeConfig {
    /// Canonical runtime knobs for tests. Keep in sync with field adds so
    /// callsites never need to care.
    pub fn test_default() -> RuntimeConfig {
        RuntimeConfig {
            worker_threads:       1,
            buffer_size:          256,
            flush_interval_ms:    100,
            collect_witnesses:    true,
            xref_cartesian_limit: 10_000,
            max_passes:           8,
            max_claims_per_pass:  10_000,
            max_cursors_per_root: 1_000_000,
        }
    }
}
