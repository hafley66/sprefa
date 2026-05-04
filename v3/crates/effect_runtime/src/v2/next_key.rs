//! `NextKey` — content-derived identity of a queue row.
//!
//! Composed from row metadata + carrier content hash. The composition
//! gives every in-flight value a structurally-stable name: the same
//! input cursor at the same `(pipe, instance, depth)` produces the same
//! key on a restart, which is what makes the `ResponseStore` cache
//! and the future `Memoize<C>` work without explicit hydration.
//!
//! 32 bytes (blake3 truncated to its native digest size). Used as a
//! `HashMap` key, a sqlite blob column, and an `EventBus` topic.

use super::next::Next;
use super::queue::QueueRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NextKey(pub [u8; 32]);

impl NextKey {
    pub fn from_bytes(b: [u8; 32]) -> Self { Self(b) }

    pub fn as_bytes(&self) -> &[u8; 32] { &self.0 }
}

/// Compute the canonical key for an in-flight queue row. Stable across
/// runs given the same row metadata + carrier content.
pub fn compute_key<N: Next>(row: &QueueRow<N>) -> NextKey {
    let mut h = blake3::Hasher::new();
    h.update(&row.parent_id.unwrap_or(0).to_le_bytes());
    h.update(&row.pipe_hash.to_le_bytes());
    h.update(&row.instance_id.to_le_bytes());
    h.update(&row.depth.to_le_bytes());
    h.update(&row.batch_idx.to_le_bytes());
    h.update(&row.drive_tick.to_le_bytes());
    for seg in &row.path {
        h.update(&seg.to_le_bytes());
    }
    h.update(&row.value.content_hash());
    NextKey(*h.finalize().as_bytes())
}
