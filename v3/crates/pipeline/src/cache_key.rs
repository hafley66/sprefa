//! L1 in-memory cache for cacheable ops (sprefa-upb Phase E).
//!
//! Two pieces:
//!   - `batch_fingerprint`: stable hash over an input cursor batch
//!     (content_hash + byte_range + sorted captures per cursor).
//!   - `OpCache`: typed `Store` holding a `Mutex<HashMap<[u8;32],
//!     Arc<[Cursor]>>>`. The runner composes
//!     `blake3(batch_fingerprint || op.cache_key(h))` and consults the
//!     map for an entry before invoking `op.pipe`.
//!
//! The cache lives off `RtCtx::store` (see `effect_runtime::Store`) so
//! no change to the framework core is required. Drivers (sprefa-run,
//! LSP) attach an `OpCache` via `RtCtxBuilder::with_store(...)` when
//! `cfg.run.cache` is true.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::_0_cursor::{Capture, CaptureKind, Cursor};
use crate::disk_cache::DiskCache;
use effect_runtime::Store;

/// Leaf coordinate used to index cache entries by file. `rev` is kept
/// as the cursor's `Arc<str>` value (e.g. "wt", "HEAD", a SHA, a tag).
/// `path` is the cursor's `fs` path normalized into a [`PathBuf`].
pub type LeafCoord = (Arc<str>, Arc<str>, PathBuf);

/// Stable fingerprint over a batch of cursors. Hashes per cursor:
///   content_hash (32 bytes; zeros if absent)
///   byte_range start/end as u64 LE
///   captures sorted by name, each:
///     name bytes || u64 LE start || u64 LE end || kind tag || synth bytes (if any)
pub fn batch_fingerprint(batch: &[Cursor]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    let zeros = [0u8; 32];
    for c in batch {
        let ch: &[u8; 32] = c
            .content_hash
            .as_deref()
            .unwrap_or(&zeros);
        h.update(ch);
        h.update(&(c.byte_range.start as u64).to_le_bytes());
        h.update(&(c.byte_range.end   as u64).to_le_bytes());

        // Repo/rev/fs are part of the cursor's logical address. Include
        // them so the same content under two repo slugs hashes apart.
        h.update(c.repo.as_bytes());
        h.update(&[0u8]);
        h.update(c.rev.as_bytes());
        h.update(&[0u8]);
        if let Some(p) = c.fs.as_deref() {
            h.update(p.to_string_lossy().as_bytes());
        }
        h.update(&[0u8]);

        let mut caps: Vec<&Capture> = c.captures.iter().collect();
        caps.sort_by(|a, b| a.name.cmp(&b.name));
        for cap in caps {
            h.update(cap.name.as_bytes());
            h.update(&[0u8]);
            h.update(&(cap.byte_range.start as u64).to_le_bytes());
            h.update(&(cap.byte_range.end   as u64).to_le_bytes());
            match &cap.kind {
                CaptureKind::SpanBacked => { h.update(&[0u8]); }
                CaptureKind::Synthesized { value } => {
                    h.update(&[1u8]);
                    h.update(value.as_bytes());
                    h.update(&[0u8]);
                }
            }
        }
        // Per-cursor terminator so two cursors don't collide with one
        // longer cursor with concatenated capture lists.
        h.update(&[0xff_u8]);
    }
    *h.finalize().as_bytes()
}

/// Typed `Store` payload registered on `RtCtx`. Pipeline::Op consults
/// this if the runtime has one bound *and* the op declares itself
/// cacheable via `Op::cache_key`.
pub struct OpCache {
    pub enabled: bool,
    map: Mutex<HashMap<[u8; 32], Arc<[Cursor]>>>,
    /// Reverse index: leaf coordinate → cache keys whose batch carried
    /// any cursor at that coordinate (sprefa-3n6 Phase F). Populated by
    /// [`OpCache::insert`] from the batch's cursor `(repo, rev, fs)`
    /// triples. Drives [`OpCache::evict_paths`] without a per-batch
    /// cascade walk: every entry rooted at the changed file is in the
    /// set directly, so eviction is a single index lookup per path.
    paths: Mutex<HashMap<LeafCoord, HashSet<[u8; 32]>>>,
    /// L2 sqlite-backed persistence (sprefa-upb Phase D). When present
    /// and `enabled`, get falls through to L2 on L1 miss and promotes
    /// the hit into L1; insert writes through to both layers.
    pub l2: Option<Arc<DiskCache>>,
}

impl std::fmt::Debug for OpCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpCache")
            .field("enabled", &self.enabled)
            .field("len", &self.len())
            .field("l2", &self.l2.as_ref().map(|_| "<DiskCache>"))
            .finish()
    }
}

impl OpCache {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            map: Mutex::new(HashMap::new()),
            paths: Mutex::new(HashMap::new()),
            l2: None,
        }
    }

    /// Attach an L2 sqlite-backed [`DiskCache`]. Calls on a disabled
    /// cache still no-op via `enabled`. Returns `Self` for chaining.
    pub fn with_l2(mut self, l2: Arc<DiskCache>) -> Self {
        self.l2 = Some(l2);
        self
    }

    pub fn get(&self, key: &[u8; 32]) -> Option<Arc<[Cursor]>> {
        if !self.enabled { return None; }
        if let Some(hit) = self.map.lock().unwrap().get(key).cloned() {
            return Some(hit);
        }
        if let Some(l2) = self.l2.as_ref() {
            if let Some(hit) = l2.get(key) {
                // Promote into L1 so subsequent calls in this process
                // skip the bincode + sqlite round trip.
                self.map.lock().unwrap().insert(*key, hit.clone());
                return Some(hit);
            }
        }
        None
    }

    pub fn insert(&self, key: [u8; 32], val: Arc<[Cursor]>) {
        if !self.enabled { return; }
        if let Some(l2) = self.l2.as_ref() {
            l2.insert(key, &val);
        }
        // Index leaf coordinates for Phase F invalidation. Every cursor
        // in the batch that has a fs path contributes one (repo, rev,
        // path) → key edge. Cursors without `fs` (pre-fs stages like
        // bare repo()/rev()) record nothing here; those entries are
        // re-keyed naturally on rev-hop via batch_fingerprint changes.
        let pairs = batch_leaf_coords(&val);
        if !pairs.is_empty() {
            let mut idx = self.paths.lock().unwrap();
            for coord in pairs {
                idx.entry(coord).or_default().insert(key);
            }
        }
        self.map.lock().unwrap().insert(key, val);
    }

    /// Phase F: evict every entry whose batch contained a cursor at
    /// any of the listed leaf coordinates. L1 + L2 + path index all
    /// scrubbed in one pass. Returns the count of evicted L1 entries.
    pub fn evict_paths(&self, coords: &[LeafCoord]) -> usize {
        if !self.enabled || coords.is_empty() { return 0; }
        let to_evict: HashSet<[u8; 32]> = {
            let mut idx = self.paths.lock().unwrap();
            let mut acc = HashSet::new();
            for coord in coords {
                if let Some(keys) = idx.remove(coord) {
                    acc.extend(keys);
                }
            }
            acc
        };
        if to_evict.is_empty() { return 0; }
        if let Some(l2) = self.l2.as_ref() {
            let v: Vec<[u8; 32]> = to_evict.iter().copied().collect();
            l2.delete_many(&v);
        }
        let mut map = self.map.lock().unwrap();
        let mut n = 0;
        for k in &to_evict {
            if map.remove(k).is_some() { n += 1; }
        }
        // Sweep stale entries from other coords' index buckets.
        let mut idx = self.paths.lock().unwrap();
        idx.retain(|_, set| {
            set.retain(|k| !to_evict.contains(k));
            !set.is_empty()
        });
        n
    }

    /// Phase F: evict every entry rooted at any path under `(repo, rev)`.
    /// Used when `RevHead` switches the worktree to a different rev:
    /// every cached batch tagged with that rev becomes stale because
    /// `cursor.rev` flowed into batch_fingerprint.
    pub fn evict_rev(&self, repo: &str, rev: &str) -> usize {
        if !self.enabled { return 0; }
        let coords: Vec<LeafCoord> = {
            let idx = self.paths.lock().unwrap();
            idx.keys()
                .filter(|(r, v, _)| r.as_ref() == repo && v.as_ref() == rev)
                .cloned()
                .collect()
        };
        self.evict_paths(&coords)
    }

    pub fn len(&self) -> usize {
        self.map.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

/// Walk a batch's cursors and collect distinct leaf coordinates. A
/// cursor without `fs` contributes nothing; downstream callers handle
/// the empty case.
fn batch_leaf_coords(batch: &[Cursor]) -> Vec<LeafCoord> {
    let mut seen: HashSet<LeafCoord> = HashSet::new();
    for c in batch {
        if let Some(p) = c.fs.as_deref() {
            seen.insert((c.repo.clone(), c.rev.clone(), p.to_path_buf()));
        }
    }
    seen.into_iter().collect()
}

impl Store for OpCache {}

/// Convenience: compute the composite cache key for a (batch, op).
/// Returns `Some(key)` when the op is cacheable.
pub fn compose_key(batch: &[Cursor], op: &dyn crate::_1_op::Op) -> Option<[u8; 32]> {
    let mut h = blake3::Hasher::new();
    h.update(&batch_fingerprint(batch));
    if !op.cache_key(&mut h) {
        return None;
    }
    Some(*h.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{FsOp, RepoOp, RevOp};
    use std::sync::Arc;

    fn cursor_with_hash(hash: [u8; 32]) -> Cursor {
        let mut c = Cursor::default();
        c.content_hash = Some(Arc::new(hash));
        c
    }

    #[test]
    fn batch_fingerprint_deterministic() {
        let a = [cursor_with_hash([7u8; 32])];
        let b = [cursor_with_hash([7u8; 32])];
        assert_eq!(batch_fingerprint(&a), batch_fingerprint(&b));
    }

    #[test]
    fn batch_fingerprint_differs_on_content_hash() {
        let a = [cursor_with_hash([7u8; 32])];
        let b = [cursor_with_hash([8u8; 32])];
        assert_ne!(batch_fingerprint(&a), batch_fingerprint(&b));
    }

    fn op_key(op: &dyn crate::_1_op::Op) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        let cacheable = op.cache_key(&mut h);
        assert!(cacheable, "op {} must be cacheable", op.name());
        *h.finalize().as_bytes()
    }

    #[test]
    fn op_cache_keys_distinguish_repo_sources() {
        let a = RepoOp::from_source("myorg/*").unwrap();
        let b = RepoOp::from_source("myorg/*").unwrap();
        let c = RepoOp::from_source("other/*").unwrap();
        assert_eq!(op_key(&a), op_key(&b));
        assert_ne!(op_key(&a), op_key(&c));
    }

    #[test]
    fn op_cache_keys_distinguish_rev_sources() {
        let a = RevOp::from_source("v1.*").unwrap();
        let b = RevOp::from_source("v1.*").unwrap();
        let c = RevOp::from_source("main").unwrap();
        assert_eq!(op_key(&a), op_key(&b));
        assert_ne!(op_key(&a), op_key(&c));
    }

    #[test]
    fn op_cache_keys_distinguish_fs_sources() {
        let a = FsOp::from_source("**/*.rs").unwrap();
        let b = FsOp::from_source("**/*.rs").unwrap();
        let c = FsOp::from_source("**/*.toml").unwrap();
        assert_eq!(op_key(&a), op_key(&b));
        assert_ne!(op_key(&a), op_key(&c));
    }

    #[test]
    fn op_cache_keys_distinguish_glob_inside_repo_filter() {
        // GlobOp + ReOp compile via tree-sitter and aren't directly
        // constructible without the host parser. Their identity is
        // exercised transitively via repo()/fs() filter sources, which
        // wrap a `compile_str(...)` glob and recurse into its cache_key.
        let a = RepoOp::from_source("a/*").unwrap();
        let b = RepoOp::from_source("a/*").unwrap();
        let c = RepoOp::from_source("b/*").unwrap();
        assert_eq!(op_key(&a), op_key(&b));
        assert_ne!(op_key(&a), op_key(&c));
    }

    #[test]
    fn op_cache_get_insert_roundtrip() {
        let cache = OpCache::new(true);
        let key = [1u8; 32];
        let v: Arc<[Cursor]> = Arc::from(vec![Cursor::default()]);
        assert!(cache.get(&key).is_none());
        cache.insert(key, v.clone());
        let got = cache.get(&key).expect("hit");
        assert_eq!(got.len(), 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn op_cache_disabled_skips_inserts() {
        let cache = OpCache::new(false);
        cache.insert([2u8; 32], Arc::from(vec![Cursor::default()]));
        assert_eq!(cache.len(), 0);
        assert!(cache.get(&[2u8; 32]).is_none());
    }
}
