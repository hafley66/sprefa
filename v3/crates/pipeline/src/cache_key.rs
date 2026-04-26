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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::_0_cursor::{Capture, CaptureKind, Cursor};
use effect_runtime::Store;

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
}

impl std::fmt::Debug for OpCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpCache")
            .field("enabled", &self.enabled)
            .field("len", &self.len())
            .finish()
    }
}

impl OpCache {
    pub fn new(enabled: bool) -> Self {
        Self { enabled, map: Mutex::new(HashMap::new()) }
    }

    pub fn get(&self, key: &[u8; 32]) -> Option<Arc<[Cursor]>> {
        if !self.enabled { return None; }
        self.map.lock().unwrap().get(key).cloned()
    }

    pub fn insert(&self, key: [u8; 32], val: Arc<[Cursor]>) {
        if !self.enabled { return; }
        self.map.lock().unwrap().insert(key, val);
    }

    pub fn len(&self) -> usize {
        self.map.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }
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
