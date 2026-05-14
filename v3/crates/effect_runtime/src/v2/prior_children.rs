//! `PriorChildIndex` — per-position record of children that flowed
//! through a render boundary, keyed by parent's `path`.
//!
//! Identity here is positional, not value-based. Two renders of the
//! same Memoize boundary at the same `row.path` are "the same source"
//! even if the input value changed. This is the React keyed-reconcile
//! shape: position carries identity, content carries value.
//!
//! Used by `Memoize::dispatch` to multiset-diff a re-render's child
//! set against the prior set:
//!
//!   doomed = prior \ new   ─── cascade_delete each
//!   kept   = prior ∩ new   ─── survive untouched (downstream subtrees stay)
//!   added  = new \ prior   ─── enqueue fresh
//!
//! In-RAM only. Sqlite-backed variant lands when the index needs to
//! survive process restart; today it rebuilds organically as the pipe
//! flows.

use std::collections::HashMap;
use std::sync::Mutex;

use super::queue::QueueId;

pub type ContentHash = [u8; 32];

#[derive(Default)]
pub struct PriorChildIndex {
    by_path: Mutex<HashMap<Vec<u32>, Vec<(ContentHash, QueueId)>>>,
}

impl PriorChildIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the prior child list for `path`, leaving the slot empty.
    /// The caller computes the diff and calls `put` with the merged
    /// (kept ++ added) list.
    pub fn take(&self, path: &[u32]) -> Vec<(ContentHash, QueueId)> {
        self.by_path
            .lock()
            .unwrap()
            .remove(path)
            .unwrap_or_default()
    }

    pub fn put(&self, path: Vec<u32>, children: Vec<(ContentHash, QueueId)>) {
        if children.is_empty() {
            // Don't grow the map with empty slots.
            self.by_path.lock().unwrap().remove(&path);
        } else {
            self.by_path.lock().unwrap().insert(path, children);
        }
    }

    pub fn get(&self, path: &[u32]) -> Vec<(ContentHash, QueueId)> {
        self.by_path
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.by_path.lock().unwrap().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Entries whose path starts with `prefix`, including `prefix`
    /// itself. Used by future cascade-by-position invalidation. Today
    /// not wired into Memoize.
    pub fn drain_prefix(&self, prefix: &[u32]) -> Vec<(Vec<u32>, Vec<(ContentHash, QueueId)>)> {
        let mut by = self.by_path.lock().unwrap();
        let keys: Vec<Vec<u32>> = by
            .keys()
            .filter(|p| p.starts_with(prefix))
            .cloned()
            .collect();
        keys.into_iter()
            .filter_map(|k| by.remove(&k).map(|v| (k, v)))
            .collect()
    }
}

/// Diff result from comparing a prior child list against a new one
/// (matched by content hash, multiset semantics — two children with
/// the same hash count as two slots).
pub struct ChildDiff {
    /// Prior entries whose hash does not appear in `new`. Their
    /// QueueIds should be cascade_delete'd.
    pub doomed_ids: Vec<QueueId>,
    /// Prior entries whose hash matched a new child. These survive.
    pub kept: Vec<(ContentHash, QueueId)>,
    /// Indexes into the `new` slice for children that have no prior
    /// match and must be enqueued fresh.
    pub added_idx: Vec<usize>,
}

/// Multiset diff of `prior` against `new_hashes`. Order of `new_hashes`
/// is preserved in `added_idx`.
pub fn diff_children(prior: &[(ContentHash, QueueId)], new_hashes: &[ContentHash]) -> ChildDiff {
    // Build a multiset of prior by hash → Vec<QueueId> stack (pop on match).
    let mut pool: HashMap<ContentHash, Vec<QueueId>> = HashMap::new();
    for (h, id) in prior {
        pool.entry(*h).or_default().push(*id);
    }

    let mut kept = Vec::new();
    let mut added_idx = Vec::new();
    for (i, h) in new_hashes.iter().enumerate() {
        if let Some(stack) = pool.get_mut(h) {
            if let Some(id) = stack.pop() {
                kept.push((*h, id));
                continue;
            }
        }
        added_idx.push(i);
    }

    let doomed_ids: Vec<QueueId> = pool.into_values().flatten().collect();
    ChildDiff {
        doomed_ids,
        kept,
        added_idx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(byte: u8) -> ContentHash {
        [byte; 32]
    }

    #[test]
    fn diff_empty_prior_all_added() {
        let d = diff_children(&[], &[h(1), h(2), h(3)]);
        assert!(d.doomed_ids.is_empty());
        assert!(d.kept.is_empty());
        assert_eq!(d.added_idx, vec![0, 1, 2]);
    }

    #[test]
    fn diff_empty_new_all_doomed() {
        let d = diff_children(&[(h(1), 10), (h(2), 20)], &[]);
        let mut doomed = d.doomed_ids.clone();
        doomed.sort();
        assert_eq!(doomed, vec![10, 20]);
        assert!(d.kept.is_empty());
        assert!(d.added_idx.is_empty());
    }

    #[test]
    fn diff_partial_overlap() {
        let prior = vec![(h(1), 10), (h(2), 20), (h(3), 30)];
        let new = vec![h(2), h(3), h(4)];
        let d = diff_children(&prior, &new);
        let mut doomed = d.doomed_ids.clone();
        doomed.sort();
        assert_eq!(doomed, vec![10]);
        let mut kept_ids: Vec<QueueId> = d.kept.iter().map(|(_, id)| *id).collect();
        kept_ids.sort();
        assert_eq!(kept_ids, vec![20, 30]);
        assert_eq!(d.added_idx, vec![2]);
    }

    #[test]
    fn diff_duplicates_use_multiset_semantics() {
        // Two identical hashes prior, three identical hashes new:
        // 2 kept, 1 added, 0 doomed.
        let prior = vec![(h(7), 100), (h(7), 200)];
        let new = vec![h(7), h(7), h(7)];
        let d = diff_children(&prior, &new);
        assert!(d.doomed_ids.is_empty());
        assert_eq!(d.kept.len(), 2);
        assert_eq!(d.added_idx, vec![2]);
    }

    #[test]
    fn put_then_take_round_trip() {
        let idx = PriorChildIndex::new();
        idx.put(vec![0, 1], vec![(h(1), 10), (h(2), 20)]);
        let got = idx.take(&[0, 1]);
        assert_eq!(got.len(), 2);
        assert_eq!(idx.len(), 0);
    }

    #[test]
    fn put_empty_does_not_grow_map() {
        let idx = PriorChildIndex::new();
        idx.put(vec![0], vec![(h(1), 10)]);
        idx.put(vec![0], vec![]);
        assert_eq!(idx.len(), 0);
    }

    #[test]
    fn drain_prefix_collects_subtree() {
        let idx = PriorChildIndex::new();
        idx.put(vec![0], vec![(h(1), 1)]);
        idx.put(vec![0, 1], vec![(h(2), 2)]);
        idx.put(vec![0, 1, 0], vec![(h(3), 3)]);
        idx.put(vec![1], vec![(h(4), 4)]);

        let drained = idx.drain_prefix(&[0]);
        assert_eq!(drained.len(), 3);
        assert_eq!(idx.len(), 1); // only path [1] survives
    }
}
