//! Config — the reactive signal that shapes every pipeline run.
//!
//! Two structs: `Config` (user-facing, loaded from `.sprefa.toml` or
//! programmatic) and `RuntimeConfig` (runner tuning knobs). `ConfigDiff`
//! surfaces what changed between two snapshots so the runner can decide
//! what to invalidate on live reload.

use std::sync::Arc;

/// User-facing configuration for a sprefa run.
///
/// **Content-hashed:** `content_hash` is a stable digest over all user
/// fields. Reparse short-circuits when the hash matches the prior run
/// — nothing to re-scan.
/// **Cardinality:** one per run. Held inside `Arc<Config>` on every
/// `OpCtx` so ops can read without sharing mutability.
/// **Allocation:** small; every field is an `Arc`-holding Vec. Clone
/// is vector-copy of Arc handles, not string copy.
#[derive(Debug, Clone)]
pub struct Config {
    /// Repo identifiers in the scan universe. Empty = "every repo under
    /// the configured workspace root".
    pub repos:        Vec<Arc<str>>,
    /// Rev identifiers (branches, tags, commits). Empty = "HEAD of each
    /// repo".
    pub revs:         Vec<Arc<str>>,
    /// Path globs to exclude from scanning. Applied at the Reader layer.
    pub fs_exclude:   Vec<Arc<str>>,
    /// .sprf source files to load for this run.
    pub sprf_files:   Vec<Arc<str>>,
    /// Allowlist for shell effects. Only commands matching a pattern
    /// here can be queued by the `sh` op family.
    pub shell_allow:  Vec<Arc<str>>,
    /// Runner tuning. See `RuntimeConfig`.
    pub runtime:      RuntimeConfig,
    /// Stable digest of the above fields. Bumps when anything material
    /// changes; stays identical for cosmetic changes.
    pub content_hash: u64,
}

/// Runner-facing tuning knobs. Changes here don't invalidate results
/// (they adjust scheduling / caps, not what gets computed).
///
/// **Cardinality:** one per run, inside `Config`.
/// **When to bump a field:** when ops start hitting a soft cap
/// regularly, surface it as a diagnostic and raise the cap with user
/// consent rather than hide.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Tokio worker thread count. Default 1 for CLI determinism; scale
    /// up for the daemon.
    pub worker_threads:     u16,
    /// Bounded channel size between op stages. Back-pressure signal for
    /// fast upstream / slow downstream.
    pub buffer_size:        u32,
    /// How often to flush accumulated rows to the Store in batch mode.
    pub flush_interval_ms:  u32,
    /// When true, framework appends `OpEvidence` entries to each cursor
    /// as it flows through ops that implement `Op::witness`. LSP needs
    /// it; CLI runs can disable for zero overhead.
    pub collect_witnesses:  bool,
    /// Soft cap on distinct cross-ref tuples per cursor expansion.
    /// Hitting it drops the cursor + emits `xref/cartesian-limit` once
    /// per op. Default 10_000.
    pub xref_cartesian_limit: usize,
    /// Outer-loop iteration cap for the scan-check fix-point.
    pub max_passes:           usize,
    /// Per-pass cap on new scan-pointer claims considered.
    pub max_claims_per_pass:  usize,
    /// Cursor fan-out cap per root — dropped with a diagnostic on
    /// overflow so pathological rules don't OOM.
    pub max_cursors_per_root: usize,
    /// Per-file byte cap for byte-reading ops (ast, json, md, line).
    /// Files over this size are skipped with a `file/size-cap`
    /// diagnostic so the user sees which files were dropped and what
    /// the cap was. Default 1 MiB — picks off generated visitor files
    /// (e.g. swc_visit) that drown debug-mode ast-grep walks.
    pub max_file_bytes:       u64,
}

/// What changed between two `Config` snapshots. The reactive layer
/// uses this to decide which rules / which cached results to
/// invalidate on a live reload.
///
/// **Cardinality:** one per config change. Held transiently in the
/// daemon's reload machinery.
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
            max_file_bytes:       1_048_576,
        }
    }
}
