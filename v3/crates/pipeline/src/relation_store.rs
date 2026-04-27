//! RelationStore — shared row-bag storage backing both `tag` and `rule`.
//!
//! tag/rule unification (chat_log/20260426.7): one storage layer, divergent
//! push side. tag pushes via `WriteEffect`. rule pushes by running its body
//! pipeline; terminal cursors of the body are sunk to the rule's bag using
//! the same `WriteEffect` path. Pull-side ops (probe/join/query) treat both
//! uniformly.
//!
//! Step 1 (rb8) is a mechanical rename + extract from `effects.rs`. The
//! `bodies` map and stub methods (`bind_body`, `body`, `lookup_by_key`) are
//! seeded here; later cards (rhu, bd5/kcl) wire them.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use effect_runtime::{
    Batcher, BoxFuture, CancellationToken, EffectKind, Store, SubjectKey, SubjectKind,
    SubjectRegistry, Unsubscribed,
};

use crate::Pipeline;

/// One row in a relation bag — opaque to the runtime, downstream zips by hole.
pub type Row = Vec<Arc<str>>;

/// One row sunk by a rule's body terminal cursors. Capture-named rather
/// than positional. Insertion order preserved: the order ops on the body
/// emitted captures into the cursor. Drain layers (e.g. SQLite) compute
/// the column union across rows and write a row-per-row.
pub type RuleRow = Vec<(Arc<str>, Arc<str>)>;

/// `SubjectKind` for relation wakes. Payload is the freshly-pushed row, so
/// readers receive it directly without re-snapshotting under the store mutex.
pub struct RelationWake;
impl SubjectKind for RelationWake {
    type Payload = Row;
}

/// Wake-key shape for a parked subscriber.
///
/// `Broadcast` waiters fire on every write (QueryOp / `tag?` all-unbound).
/// `Exact` waiters fire only when the appended row's leading columns match
/// the supplied key vector. Step bd5 ships exact-prefix matching; later
/// cards may add column-projection keys.
pub type WakeKey = Option<Vec<Arc<str>>>;

/// One bag of rows + parked subscribers waiting for the next write.
///
/// Subjects are pre-registered on `SubjectRegistry<RelationWake>` (entries
/// already inserted in `pending`). On write, the writer derives the row's
/// key and fires only the matching waiters via `registry.next(key, row)`.
///
/// **Set semantics, not multiset.** A row that already exists in the bag
/// is dropped at push time: no append, no waiter fired. `seen` is the
/// dedup index; `rows` preserves insertion order for snapshot+last_idx
/// readers.
#[derive(Default)]
pub struct Bag {
    pub rows:        Vec<Row>,
    pub seen:        HashSet<Row>,
    /// Waiters that fire on every write (no key narrowing).
    pub broadcast:   Vec<SubjectKey<RelationWake>>,
    /// Keyed waiters indexed by the prefix vector they match against.
    pub keyed:       HashMap<Vec<Arc<str>>, Vec<SubjectKey<RelationWake>>>,
}

/// Outcome of an atomic snapshot-or-subscribe call. Either the caller gets
/// the tail of new rows since `last_idx`, or it gets a future that resolves
/// the next time a writer drains the waiter list.
pub enum SnapshotOrSubscribed {
    Rows(Vec<Row>),
    Subscribed(BoxFuture<'static, Result<Arc<Row>, Unsubscribed>>),
}

/// Relational bag store. Holds one [`Bag`] per name plus a parallel map of
/// rule bodies keyed by name. Tag bags have no body; rule bags do.
///
/// Stored on `RtCtx` via `with_store`. Pull ops fetch via
/// `ctx.store::<RelationStore>()`; the write batcher holds an
/// `Arc<RelationStore>` at registration time.
#[derive(Default)]
pub struct RelationStore {
    inner:      Mutex<HashMap<Arc<str>, Bag>>,
    bodies:     Mutex<HashMap<Arc<str>, Arc<Pipeline>>>,
    /// Declared param names per rule. Used by `RuleCallOp` to bind
    /// caller args into the body's capture namespace.
    params:     Mutex<HashMap<Arc<str>, Vec<Arc<str>>>>,
    /// Rows sunk by rule bodies. Capture-named, distinct from `inner`'s
    /// positional `Row` shape. Drained at end of run by the SQLite
    /// writer; one table per rule, column union across rows.
    ///
    /// Set semantics: a (rule_name, row) pair seen twice is dropped on
    /// the second push. The `rule_row_seen` index mirrors `rule_rows`
    /// for O(1) dedup. Order in the Vec is first-insertion order.
    rule_rows:       Mutex<HashMap<Arc<str>, Vec<RuleRow>>>,
    rule_row_seen:   Mutex<HashMap<Arc<str>, HashSet<RuleRow>>>,
}

impl Store for RelationStore {}

impl RelationStore {
    pub fn new() -> Self { Self::default() }

    /// Test/debug hook: how many rows currently in `name`'s bag.
    pub fn rows_len(&self, name: &str) -> usize {
        self.inner
            .lock()
            .unwrap()
            .get(name)
            .map(|b| b.rows.len())
            .unwrap_or(0)
    }

    /// Atomically check for new rows past `last_idx`. If any exist, return
    /// them as `Rows`. Otherwise register a fresh waiter on the supplied
    /// `SubjectRegistry` (synchronously inserts a pending entry per
    /// `subscribe`'s contract) and return a `Subscribed` future.
    ///
    /// `wake_key` selects which writes wake this subscriber:
    ///   * `None` — broadcast: fires on every push to this bag.
    ///   * `Some(k)` — exact-prefix: fires only when the appended row's
    ///     leading columns equal `k`.
    ///
    /// The `subj_key` passed in is consumed: it goes into both the registry
    /// pending map and the bag's waiter index under one store-lock critical
    /// section, closing the producer-side race where a write would otherwise
    /// drain `subj_key` from the bag before the registry entry exists.
    pub fn snapshot_or_subscribe(
        &self,
        name:     &Arc<str>,
        last_idx: usize,
        subj_key: SubjectKey<RelationWake>,
        wake_key: WakeKey,
        registry: &Arc<SubjectRegistry<RelationWake>>,
    ) -> SnapshotOrSubscribed {
        let mut g = self.inner.lock().unwrap();
        let bag = g.entry(name.clone()).or_default();
        if bag.rows.len() > last_idx {
            let tail: Vec<Row> = bag.rows[last_idx..].to_vec();
            SnapshotOrSubscribed::Rows(tail)
        } else {
            let fut = registry.subscribe(subj_key, None);
            match wake_key {
                None => bag.broadcast.push(subj_key),
                Some(k) => bag.keyed.entry(k).or_default().push(subj_key),
            }
            SnapshotOrSubscribed::Subscribed(fut)
        }
    }

    /// Append `row` to the named bag. Returns every waiter whose key shape
    /// matches the row (broadcast + exact-prefix keyed). Caller fires
    /// `registry.next` on each.
    ///
    /// Prefer [`push_rows`] for multi-row writes — it amortizes the bag
    /// lock + waiter scan across the whole batch.
    pub fn push_row(&self, name: &Arc<str>, row: Row) -> Vec<(SubjectKey<RelationWake>, Arc<Row>)> {
        self.push_rows(std::slice::from_ref(&(name.clone(), row)))
    }

    /// Append every `(name, row)` pair into its bag under a single lock
    /// acquire. Returns every triggered waiter paired with the row it
    /// observed, so the caller can fire `registry.next(key, Arc<Row>)`.
    pub fn push_rows(
        &self,
        writes: &[(Arc<str>, Row)],
    ) -> Vec<(SubjectKey<RelationWake>, Arc<Row>)> {
        let mut g = self.inner.lock().unwrap();
        let mut out: Vec<(SubjectKey<RelationWake>, Arc<Row>)> = Vec::new();
        for (name, row) in writes {
            let bag = g.entry(name.clone()).or_default();
            // Set semantics: a duplicate row is a no-op. Skip the push
            // and skip waking waiters — there is nothing new to observe.
            if !bag.seen.insert(row.clone()) { continue; }
            bag.rows.push(row.clone());
            let row_arc: Arc<Row> = Arc::new(row.clone());
            for k in bag.broadcast.drain(..) {
                out.push((k, row_arc.clone()));
            }
            bag.keyed.retain(|key, subs| {
                let is_prefix = key.len() <= row.len()
                    && key.iter().zip(row.iter()).all(|(a, b)| a == b);
                if is_prefix {
                    for k in subs.drain(..) {
                        out.push((k, row_arc.clone()));
                    }
                    false
                } else {
                    true
                }
            });
        }
        out
    }

    /// Register a rule body pipeline under `name`. Idempotent: re-binding
    /// overwrites. Called by the lower's pass-1 walk.
    pub fn bind_body(&self, name: Arc<str>, body: Arc<Pipeline>) {
        self.bodies.lock().unwrap().insert(name, body);
    }

    /// Fetch a rule body pipeline by name.
    pub fn body(&self, name: &str) -> Option<Arc<Pipeline>> {
        self.bodies.lock().unwrap().get(name).cloned()
    }

    /// Register the param name list for `name`. Empty `Vec` for arity-0.
    pub fn bind_params(&self, name: Arc<str>, params: Vec<Arc<str>>) {
        self.params.lock().unwrap().insert(name, params);
    }

    /// Fetch param names. Empty if rule unknown.
    pub fn params(&self, name: &str) -> Vec<Arc<str>> {
        self.params.lock().unwrap().get(name).cloned().unwrap_or_default()
    }

    /// Append one rule row under `name`. Set semantics: an identical
    /// row already in the bag is dropped silently.
    pub fn push_rule_row(&self, name: &Arc<str>, row: RuleRow) {
        let mut seen_map = self.rule_row_seen.lock().unwrap();
        let seen = seen_map.entry(name.clone()).or_default();
        if !seen.insert(row.clone()) { return; }
        drop(seen_map);
        self.rule_rows
            .lock()
            .unwrap()
            .entry(name.clone())
            .or_default()
            .push(row);
    }

    /// Snapshot the entire rule-rows map. Drain-side helper for SQLite
    /// writer; clones the inner Vecs.
    pub fn rule_rows_snapshot(&self) -> HashMap<Arc<str>, Vec<RuleRow>> {
        self.rule_rows.lock().unwrap().clone()
    }

    /// Snapshot every row currently in `name`'s bag. Used by JoinOp to
    /// take one read of the bag and fan out cursors against that snapshot.
    pub fn rows_snapshot(&self, name: &Arc<str>) -> Vec<Row> {
        self.inner
            .lock()
            .unwrap()
            .get(name)
            .map(|b| b.rows.clone())
            .unwrap_or_default()
    }

    /// Atomic key-or-subscribe: under one bag-lock, scan for an existing
    /// row whose leading columns match `key`. If found, return `true`. Else
    /// register `subj_key` as a keyed waiter under `key` and return `false`,
    /// after pre-inserting the registry pending entry — same race-safety
    /// contract as [`snapshot_or_subscribe`].
    ///
    /// The returned `Subscribed` future is `None` when the key already hit
    /// (caller should not park). When `false`, the future is `Some` and
    /// resolves on the next matching write.
    pub fn lookup_or_subscribe(
        &self,
        name:     &Arc<str>,
        key:      Vec<Arc<str>>,
        subj_key: SubjectKey<RelationWake>,
        registry: &Arc<SubjectRegistry<RelationWake>>,
    ) -> Option<BoxFuture<'static, Result<Arc<Row>, Unsubscribed>>> {
        let mut g = self.inner.lock().unwrap();
        let bag = g.entry(name.clone()).or_default();
        let hit = bag
            .rows
            .iter()
            .any(|row| key.len() <= row.len() && key.iter().zip(row.iter()).all(|(a, b)| a == b));
        if hit {
            None
        } else {
            let fut = registry.subscribe(subj_key, None);
            bag.keyed.entry(key).or_default().push(subj_key);
            Some(fut)
        }
    }

    /// Exact-row key match against the named bag. Returns matching rows.
    ///
    /// Prefer [`probe_keys`] for multi-key probes — it takes the bag lock
    /// once and answers every key in one pass.
    pub fn lookup_by_key(&self, name: &str, key: &[Arc<str>]) -> Vec<Row> {
        let g = self.inner.lock().unwrap();
        let Some(bag) = g.get(name) else { return Vec::new() };
        bag.rows
            .iter()
            .filter(|row| row.as_slice() == key)
            .cloned()
            .collect()
    }

    /// Batched hit-test against the named bag. Returns one bool per input
    /// key, true iff at least one row in the bag matches the key exactly.
    /// Lock acquired once; rows scanned once per key.
    pub fn probe_keys(&self, name: &str, keys: &[Vec<Arc<str>>]) -> Vec<bool> {
        let g = self.inner.lock().unwrap();
        let Some(bag) = g.get(name) else { return vec![false; keys.len()] };
        keys.iter()
            .map(|key| bag.rows.iter().any(|row| row.as_slice() == key.as_slice()))
            .collect()
    }
}

/// Append one or more rows to (possibly different) named bags in a single
/// store transaction. Avoids the N+1 fan-out where every cursor in a pipe
/// batch would otherwise dispatch its own `WriteEffect` round-trip.
///
/// The single-row case is `WriteEffect { writes: vec![(name, row)] }`.
#[derive(Clone, Debug)]
pub struct WriteEffect {
    pub writes: Vec<(Arc<str>, Vec<Arc<str>>)>,
}

impl WriteEffect {
    /// Convenience builder for the single-row write site.
    pub fn one(name: Arc<str>, row: Vec<Arc<str>>) -> Self {
        Self { writes: vec![(name, row)] }
    }
}

impl EffectKind for WriteEffect {
    type Response = ();

    fn payload_bytes(&self) -> Option<usize> {
        Some(
            self.writes
                .iter()
                .map(|(n, r)| n.len() + r.iter().map(|s| s.len()).sum::<usize>())
                .sum::<usize>(),
        )
    }
}

pub struct WriteBatcher {
    store:    Arc<RelationStore>,
    registry: Arc<SubjectRegistry<RelationWake>>,
}

impl WriteBatcher {
    pub fn new(store: Arc<RelationStore>, registry: Arc<SubjectRegistry<RelationWake>>) -> Self {
        Self { store, registry }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Arc<str> { Arc::from(x) }

    #[test]
    fn keyed_waiter_fires_only_on_matching_row() {
        let store = RelationStore::new();
        let registry = Arc::new(SubjectRegistry::<RelationWake>::new());
        let name: Arc<str> = s("r");

        let key_match = registry.fresh_key();
        let key_other = registry.fresh_key();
        match store.snapshot_or_subscribe(&name, 0, key_match, Some(vec![s("hit")]), &registry) {
            SnapshotOrSubscribed::Subscribed(_) => {}
            _ => panic!("expected Subscribed on empty bag"),
        }
        match store.snapshot_or_subscribe(&name, 0, key_other, Some(vec![s("miss")]), &registry) {
            SnapshotOrSubscribed::Subscribed(_) => {}
            _ => panic!("expected Subscribed on empty bag"),
        }

        let woken = store.push_row(&name, vec![s("hit")]);
        let woken_keys: Vec<_> = woken.iter().map(|(k, _)| *k).collect();
        assert_eq!(woken_keys, vec![key_match]);

        // The non-matching keyed waiter is still parked.
        let g = store.inner.lock().unwrap();
        let bag = g.get(&name).unwrap();
        assert_eq!(bag.keyed.get(&vec![s("miss")]).map(|v| v.len()), Some(1));
    }

    #[test]
    fn broadcast_waiter_fires_on_every_row() {
        let store = RelationStore::new();
        let registry = Arc::new(SubjectRegistry::<RelationWake>::new());
        let name: Arc<str> = s("r");

        let k = registry.fresh_key();
        store.snapshot_or_subscribe(&name, 0, k, None, &registry);

        let woken = store.push_row(&name, vec![s("anything")]);
        let woken_keys: Vec<_> = woken.iter().map(|(k, _)| *k).collect();
        assert_eq!(woken_keys, vec![k]);
    }

    #[test]
    fn keyed_prefix_match_fires_with_extra_row_columns() {
        let store = RelationStore::new();
        let registry = Arc::new(SubjectRegistry::<RelationWake>::new());
        let name: Arc<str> = s("r");

        let k = registry.fresh_key();
        store.snapshot_or_subscribe(&name, 0, k, Some(vec![s("a")]), &registry);

        // Row has extra trailing columns; prefix [a] matches.
        let woken = store.push_row(&name, vec![s("a"), s("b")]);
        let woken_keys: Vec<_> = woken.iter().map(|(k, _)| *k).collect();
        assert_eq!(woken_keys, vec![k]);
    }
}

impl Batcher<WriteEffect> for WriteBatcher {
    fn run(
        &self,
        req:     WriteEffect,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        let store = self.store.clone();
        let registry = self.registry.clone();
        Box::pin(async move {
            let triggered = store.push_rows(&req.writes);
            for (subj_key, row_arc) in triggered {
                registry.next(subj_key, row_arc);
            }
        })
    }
}
