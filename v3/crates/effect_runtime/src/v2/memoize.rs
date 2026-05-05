//! `Memoize<C>` + `MemoCache<N>` — the useMemo / memo HOC of the
//! runtime. Wraps a `Component`, caches its `render` output keyed by
//! `(ident, input.content_hash())`, replays the cached `Node<N>` on
//! hit. Sprefa's "rules are pure" rule maps directly to the cache's
//! purity precondition.
//!
//! Invalidation:
//!   - `MemoCache` is a `BusListener`. On `Event::Dirty { domain, key:
//!     None }`, every entry tagged with `domain` is dropped. On
//!     `Event::Dirty { domain: _, key: Some(k) }`, the entry whose
//!     input hash equals `k.0` is dropped (rare; usually the cache is
//!     coarser-grained than the wake registry).
//!
//! Each `Memoize` records the domains its inner Component depends on
//! at construction time (`with_domain("fs")`). That tag rides with
//! the cached entry and is what `DomainDirty` matches against.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::component::{Component, RenderCtx};
use super::event_bus::{BusListener, Event, EventBus};
use super::flatten::{flatten, splice_into};
use super::next::Next;
use super::next_key::NextKey;
use super::node::Node;
use super::prior_children::{diff_children, PriorChildIndex};
use super::queue::{QueueBackend, QueueId, QueueRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoKey(pub [u8; 32]);

pub struct MemoCache<N: Next> {
    by_key: Mutex<HashMap<MemoKey, Entry<N>>>,
}

struct Entry<N: Next> {
    node:           Node<N>,
    domains:        Vec<&'static str>,
    input:          [u8; 32],
    /// QueueId of the row whose render produced this entry. `Some`
    /// when stored via `Memoize::dispatch` (the substrate path).
    /// `None` when stored via `MemoCache::put` directly (test paths).
    /// On eviction this row + its descendants are cascade-deleted.
    source_row_id:  Option<QueueId>,
}

impl<N: Next> MemoCache<N> {
    pub fn new() -> Self {
        Self { by_key: Mutex::new(HashMap::new()) }
    }

    pub fn lookup(&self, key: MemoKey) -> Option<Node<N>> {
        self.by_key.lock().unwrap().get(&key).map(|e| e.node.clone())
    }

    pub fn put(&self, key: MemoKey, node: Node<N>, domains: Vec<&'static str>, input: [u8; 32]) {
        self.put_with_source(key, node, domains, input, None);
    }

    pub fn put_with_source(
        &self,
        key:           MemoKey,
        node:          Node<N>,
        domains:       Vec<&'static str>,
        input:         [u8; 32],
        source_row_id: Option<QueueId>,
    ) {
        self.by_key.lock().unwrap().insert(key, Entry {
            node, domains, input, source_row_id,
        });
    }

    pub fn len(&self) -> usize { self.by_key.lock().unwrap().len() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Drop all entries tagged with `domain`. Returns the source_row_ids
    /// of evicted entries (in unspecified order). Used by the cascade
    /// listener; bare callers can ignore the return.
    pub fn invalidate_domain(&self, domain: &str) -> Vec<QueueId> {
        let mut by = self.by_key.lock().unwrap();
        let mut evicted = Vec::new();
        by.retain(|_, e| {
            let kill = e.domains.iter().any(|d| *d == domain);
            if kill {
                if let Some(id) = e.source_row_id { evicted.push(id); }
                false
            } else { true }
        });
        evicted
    }

    /// Drop entries whose input hash equals `k.0`. Returns the
    /// source_row_ids of evicted entries.
    pub fn invalidate_input(&self, k: NextKey) -> Vec<QueueId> {
        let mut by = self.by_key.lock().unwrap();
        let mut evicted = Vec::new();
        by.retain(|_, e| {
            let kill = e.input == k.0;
            if kill {
                if let Some(id) = e.source_row_id { evicted.push(id); }
                false
            } else { true }
        });
        evicted
    }
}

impl<N: Next> Default for MemoCache<N> {
    fn default() -> Self { Self::new() }
}

impl<N: Next> BusListener for MemoCache<N> {
    fn on_event(&self, ev: &Event) {
        // Cache-only listener: drops entries, does not cascade. Use
        // `attach_cache_to_bus_with_queue` to also remove descendants
        // from the queue on eviction.
        match ev {
            Event::Dirty { domain, key: None }       => { let _ = self.invalidate_domain(domain); }
            Event::Dirty { domain: _, key: Some(k) } => { let _ = self.invalidate_input(*k); }
        }
    }
}

/// Bus listener that evicts cache entries AND cascade-deletes their
/// downstream queue rows. This is the wiring that makes fine-grain
/// invalidation work: when an upstream fact changes, every memoized
/// render keyed off the now-stale data is dropped, and every queued or
/// parked descendant of those renders is removed before it can fire.
pub struct CacheCascadeListener<N: Next> {
    cache: Arc<MemoCache<N>>,
    queue: Arc<dyn QueueBackend<N>>,
}

impl<N: Next> CacheCascadeListener<N> {
    pub fn new(cache: Arc<MemoCache<N>>, queue: Arc<dyn QueueBackend<N>>) -> Self {
        Self { cache, queue }
    }
}

impl<N: Next> BusListener for CacheCascadeListener<N> {
    fn on_event(&self, ev: &Event) {
        let evicted = match ev {
            Event::Dirty { domain, key: None }       => self.cache.invalidate_domain(domain),
            Event::Dirty { domain: _, key: Some(k) } => self.cache.invalidate_input(*k),
        };
        for id in evicted { self.queue.cascade_delete(id); }
    }
}

pub struct Memoize<C: Component> {
    inner:    C,
    ident:    &'static str,
    domains:  Vec<&'static str>,
    cache:    Arc<MemoCache<C::Next>>,
    /// Optional per-position child index. When set, `dispatch` does
    /// multiset-diff reconcile against prior children at the same
    /// `row.path`, cascade-deleting only what changed.
    children: Option<Arc<PriorChildIndex>>,
}

impl<C: Component> Memoize<C> {
    pub fn new(inner: C, ident: &'static str, cache: Arc<MemoCache<C::Next>>) -> Self {
        Self { inner, ident, domains: Vec::new(), cache, children: None }
    }

    pub fn with_domain(mut self, d: &'static str) -> Self {
        self.domains.push(d);
        self
    }

    /// Enable B2-style positional reconcile. Without this, `dispatch`
    /// behaves as in (b): always re-splice children, no diff.
    pub fn with_prior_children(mut self, idx: Arc<PriorChildIndex>) -> Self {
        self.children = Some(idx);
        self
    }

    fn key_for(&self, input_hash: &[u8; 32]) -> MemoKey {
        let mut h = blake3::Hasher::new();
        h.update(self.ident.as_bytes());
        h.update(input_hash);
        MemoKey(*h.finalize().as_bytes())
    }
}

impl<C: Component> Component for Memoize<C> {
    type Next = C::Next;

    /// Tier 1 fallback. Stores the entry but cannot record a
    /// source_row_id (no row available outside dispatch). On
    /// invalidation the entry drops but no cascade fires for it.
    fn render(&self, ctx: &RenderCtx, c: &Self::Next) -> Node<Self::Next> {
        let input = c.content_hash();
        let key   = self.key_for(&input);
        if let Some(node) = self.cache.lookup(key) {
            return node;
        }
        let node = self.inner.render(ctx, c);
        self.cache.put_with_source(key, node.clone(), self.domains.clone(), input, None);
        node
    }

    /// Tier 3. Per-row substrate path that records the producing row's
    /// id so cascade_delete can later wipe its descendants on cache
    /// eviction. Bypasses `render_batch` of the inner Component since
    /// caching is per-row by definition.
    ///
    /// When `with_prior_children` was called, this also performs
    /// multiset-diff reconcile against the prior child set at the same
    /// `row.path`: doomed children are cascade_deleted, kept children
    /// stay (their downstream subtrees are preserved), only added
    /// children are enqueued. Otherwise behaves as in (b): re-splices
    /// every child, no diff.
    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Self::Next>],
        queue: &dyn QueueBackend<Self::Next>,
    ) {
        for row in rows {
            let input = row.value.content_hash();
            let key   = self.key_for(&input);
            let node  = if let Some(cached) = self.cache.lookup(key) {
                cached
            } else {
                let n = self.inner.render(ctx, &row.value);
                self.cache.put_with_source(
                    key,
                    n.clone(),
                    self.domains.clone(),
                    input,
                    Some(row.id),
                );
                n
            };

            match &self.children {
                None => {
                    splice_into(row, node, ctx.depth + 1, ctx.expand_tick, queue);
                }
                Some(idx) => {
                    // B2 reconcile: diff new children against prior at row.path.
                    let new_children = flatten(node, row, ctx.depth + 1, ctx.expand_tick);
                    let new_hashes: Vec<[u8; 32]> = new_children
                        .iter()
                        .map(|c| c.value.content_hash())
                        .collect();

                    let prior = idx.take(&row.path);
                    let diff  = diff_children(&prior, &new_hashes);

                    for id in &diff.doomed_ids { queue.cascade_delete(*id); }

                    // Walk new_children once, consuming. For each index
                    // not in added_idx, drop the child (kept slot
                    // already covered by prior id).
                    let added: std::collections::HashSet<usize> =
                        diff.added_idx.iter().copied().collect();
                    let mut merged = diff.kept;
                    for (i, child) in new_children.into_iter().enumerate() {
                        if added.contains(&i) {
                            let h  = new_hashes[i];
                            let id = queue.enqueue(child);
                            merged.push((h, id));
                        }
                    }
                    idx.put(row.path.clone(), merged);
                }
            }
        }
    }
}

/// Bind a `MemoCache` to an `EventBus` so `Dirty` events drop matching
/// entries. Cache-only path — no queue cascade. Use
/// `attach_cache_to_bus_with_queue` for fine-grain invalidation.
pub fn attach_cache_to_bus<N: Next>(cache: Arc<MemoCache<N>>, bus: &EventBus) {
    bus.add_listener(cache);
}

/// Bind a `MemoCache` to an `EventBus` AND a queue. On `Dirty`, evict
/// matching entries and cascade_delete the queue subtree under each
/// evicted entry's source_row_id.
pub fn attach_cache_to_bus_with_queue<N: Next>(
    cache: Arc<MemoCache<N>>,
    bus:   &EventBus,
    queue: Arc<dyn QueueBackend<N>>,
) {
    bus.add_listener(Arc::new(CacheCascadeListener::new(cache, queue)));
}
