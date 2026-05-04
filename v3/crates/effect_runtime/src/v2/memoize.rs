//! `Memoize<C>` + `MemoCache<N>` — the useMemo / memo HOC of the
//! runtime. Wraps a `Component`, caches its `render` output keyed by
//! `(ident, input.content_hash())`, replays the cached `Node<N>` on
//! hit. Sprefa's "rules are pure" rule maps directly to the cache's
//! purity precondition.
//!
//! Invalidation:
//!   - `MemoCache` is a `BusListener`. On `DomainDirty(d)`, every
//!     entry tagged with `d` is dropped. On `KeyDirty(k)`, the entry
//!     whose input hash equals `k.0` is dropped (rare; usually the
//!     cache is coarser-grained than the wake registry).
//!
//! Each `Memoize` records the domains its inner Component depends on
//! at construction time (`with_domain("fs")`). That tag rides with
//! the cached entry and is what `DomainDirty` matches against.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::component::{Component, RenderCtx};
use super::event_bus::{BusListener, Event, EventBus};
use super::next::Next;
use super::next_key::NextKey;
use super::node::Node;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoKey(pub [u8; 32]);

pub struct MemoCache<N: Next> {
    by_key: Mutex<HashMap<MemoKey, Entry<N>>>,
}

struct Entry<N: Next> {
    node:    Node<N>,
    domains: Vec<&'static str>,
    input:   [u8; 32],
}

impl<N: Next> MemoCache<N> {
    pub fn new() -> Self {
        Self { by_key: Mutex::new(HashMap::new()) }
    }

    pub fn lookup(&self, key: MemoKey) -> Option<Node<N>> {
        self.by_key.lock().unwrap().get(&key).map(|e| e.node.clone())
    }

    pub fn put(&self, key: MemoKey, node: Node<N>, domains: Vec<&'static str>, input: [u8; 32]) {
        self.by_key.lock().unwrap().insert(key, Entry { node, domains, input });
    }

    pub fn len(&self) -> usize { self.by_key.lock().unwrap().len() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Drop all entries tagged with `domain`. Returns the number of
    /// entries removed — useful for metrics + tests.
    pub fn invalidate_domain(&self, domain: &str) -> usize {
        let mut by = self.by_key.lock().unwrap();
        let before = by.len();
        by.retain(|_, e| !e.domains.iter().any(|d| *d == domain));
        before - by.len()
    }

    /// Drop the entry whose input hash equals `k.0`. Returns whether
    /// an entry was removed.
    pub fn invalidate_input(&self, k: NextKey) -> bool {
        let mut by = self.by_key.lock().unwrap();
        let before = by.len();
        by.retain(|_, e| e.input != k.0);
        before != by.len()
    }
}

impl<N: Next> Default for MemoCache<N> {
    fn default() -> Self { Self::new() }
}

impl<N: Next> BusListener for MemoCache<N> {
    fn on_event(&self, ev: &Event) {
        match ev {
            Event::DomainDirty(d) => { self.invalidate_domain(d); }
            Event::KeyDirty(k)    => { self.invalidate_input(*k); }
            Event::PathDirty(_)   => { /* paths track parker rows, not memo entries */ }
        }
        // PHASE E (deferred): when an entry is invalidated, the
        // children it previously emitted are downstream rows in the
        // queue that no longer have a valid parent. Walk the cache's
        // prior-children index for the dropped MemoKey(s) and call
        // queue.cascade_delete(child_id) for each. Currently those
        // rows persist as orphans — fine until the same parent re-
        // renders with a different child set, which is exactly the
        // point Phase E becomes load-bearing.
    }
}

pub struct Memoize<C: Component> {
    inner:   C,
    ident:   &'static str,
    domains: Vec<&'static str>,
    cache:   Arc<MemoCache<C::Next>>,
}

impl<C: Component> Memoize<C> {
    pub fn new(inner: C, ident: &'static str, cache: Arc<MemoCache<C::Next>>) -> Self {
        Self { inner, ident, domains: Vec::new(), cache }
    }

    pub fn with_domain(mut self, d: &'static str) -> Self {
        self.domains.push(d);
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

    fn render(&self, ctx: &RenderCtx, c: &Self::Next) -> Node<Self::Next> {
        let input = c.content_hash();
        let key   = self.key_for(&input);
        if let Some(node) = self.cache.lookup(key) {
            return node;
        }
        let node = self.inner.render(ctx, c);
        self.cache.put(key, node.clone(), self.domains.clone(), input);
        node
    }
}

/// Bind a `MemoCache` to an `EventBus` so `DomainDirty` / `KeyDirty`
/// events trigger invalidation. Idempotent at the listener level —
/// the bus stores the cache by `Arc` identity; calling twice doubles
/// the dispatch cost but is otherwise harmless.
pub fn attach_cache_to_bus<N: Next>(cache: Arc<MemoCache<N>>, bus: &EventBus) {
    bus.add_listener(cache);
}
