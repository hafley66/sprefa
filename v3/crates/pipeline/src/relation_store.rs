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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use effect_runtime::{
    Batcher, BoxFuture, CancellationToken, EffectKind, Store, SubjectKey, SubjectKind,
    SubjectRegistry, Unsubscribed,
};

use crate::Pipeline;

/// One row in a relation bag — opaque to the runtime, downstream zips by hole.
pub type Row = Vec<Arc<str>>;

/// `SubjectKind` for relation wakes. Payload is the freshly-pushed row, so
/// readers receive it directly without re-snapshotting under the store mutex.
pub struct RelationWake;
impl SubjectKind for RelationWake {
    type Payload = Row;
}

/// One bag of rows + parked subscribers waiting for the next write.
#[derive(Default)]
pub struct Bag {
    pub rows: Vec<Row>,
    /// Subjects pre-registered on `SubjectRegistry<RelationWake>` (entries
    /// already inserted in `pending`). On write, each is woken via
    /// `registry.next(key, row)` and removed from this list.
    pub waiters: Vec<SubjectKey<RelationWake>>,
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
    inner:  Mutex<HashMap<Arc<str>, Bag>>,
    bodies: Mutex<HashMap<Arc<str>, Arc<Pipeline>>>,
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
    /// The `key` passed in is consumed: it goes into both the registry
    /// pending map and the bag's waiters list under one store-lock critical
    /// section, closing the producer-side race where a write would otherwise
    /// drain `key` from the bag before the registry entry exists.
    pub fn snapshot_or_subscribe(
        &self,
        name: &Arc<str>,
        last_idx: usize,
        key:      SubjectKey<RelationWake>,
        registry: &Arc<SubjectRegistry<RelationWake>>,
    ) -> SnapshotOrSubscribed {
        let mut g = self.inner.lock().unwrap();
        let bag = g.entry(name.clone()).or_default();
        if bag.rows.len() > last_idx {
            let tail: Vec<Row> = bag.rows[last_idx..].to_vec();
            SnapshotOrSubscribed::Rows(tail)
        } else {
            let fut = registry.subscribe(key, None);
            bag.waiters.push(key);
            SnapshotOrSubscribed::Subscribed(fut)
        }
    }

    /// Append `row` to the named bag. Returns the waiter list that was parked
    /// at the time of the write; caller fires `registry.next` on each.
    pub fn push_row(&self, name: &Arc<str>, row: Row) -> Vec<SubjectKey<RelationWake>> {
        let mut g = self.inner.lock().unwrap();
        let bag = g.entry(name.clone()).or_default();
        bag.rows.push(row);
        std::mem::take(&mut bag.waiters)
    }

    /// Stub: register a rule body pipeline under `name`. Wired by `rhu`.
    pub fn bind_body(&self, name: Arc<str>, body: Arc<Pipeline>) {
        self.bodies.lock().unwrap().insert(name, body);
    }

    /// Stub: fetch a rule body pipeline by name. Wired by `rhu`.
    pub fn body(&self, name: &str) -> Option<Arc<Pipeline>> {
        self.bodies.lock().unwrap().get(name).cloned()
    }

    /// Stub: exact-row key match against the named bag. Step 3 (bd5) replaces
    /// linear scan with a keyed waiter index; step 4 (kcl) consumes this for
    /// strict-bound probe.
    pub fn lookup_by_key(&self, name: &str, key: &[Arc<str>]) -> Vec<Row> {
        let g = self.inner.lock().unwrap();
        let Some(bag) = g.get(name) else { return Vec::new() };
        bag.rows
            .iter()
            .filter(|row| row.as_slice() == key)
            .cloned()
            .collect()
    }
}

/// Append one row to a named bag. The batcher fires `registry.next` on every
/// parked waiter so the read op can re-snapshot.
#[derive(Clone, Debug)]
pub struct WriteEffect {
    pub name: Arc<str>,
    pub row:  Vec<Arc<str>>,
}

impl EffectKind for WriteEffect {
    type Response = ();

    fn payload_bytes(&self) -> Option<usize> {
        Some(self.row.iter().map(|s| s.len()).sum::<usize>() + self.name.len())
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

impl Batcher<WriteEffect> for WriteBatcher {
    fn run(
        &self,
        req:     WriteEffect,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        let store = self.store.clone();
        let registry = self.registry.clone();
        Box::pin(async move {
            let row = req.row;
            let waiters = store.push_row(&req.name, row.clone());
            let wake: Arc<Row> = Arc::new(row);
            for k in waiters {
                registry.next(k, wake.clone());
            }
        })
    }
}
