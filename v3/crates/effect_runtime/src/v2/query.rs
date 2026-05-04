//! `Query<F>` + `QueryCache<N>` + `QueryStatus<N>` — useQuery / react-
//! query analogue.
//!
//! A `QueryFn` is a deterministic, idempotent read effect: same input,
//! same output. It runs off-thread (via the `Spawner`), populates a
//! shared `QueryCache`, and signals the bus when its result lands.
//! Components return `Suspense{Key(k)}` while pending, `Emit(data)`
//! when the cache shows `Success`. The cache's `EffectKey` is
//! `blake3(ident, input.content_hash())` — content-derived, stable
//! across processes.
//!
//! Invalidation:
//!   - `QueryCache` is a `BusListener`. `DomainDirty(d)` drops every
//!     entry tagged with `d` — invalidateQueries({ queryKey: ['d'] }).
//!   - `KeyDirty(k)` is consumed by the runtime as a wake signal; it
//!     does NOT clear the cache. A pending query that "completes"
//!     dispatches `KeyDirty` so the parked Component pulls; the cache
//!     entry is what the Component actually reads.
//!
//! Query<F> impls `Component<Next = N>` where `N` is the carrier
//! shared by input and output. Multi-carrier pipes (Input ≠ Output)
//! are out of scope until the substrate gains a typed connector
//! between heterogenous components.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::component::{Component, RenderCtx};
use super::effect_dispatch::Spawner;
use super::event_bus::{BusListener, Event, EventBus};
use super::next::Next;
use super::next_key::NextKey;
use super::node::Node;
use super::wake::Wake;

pub enum QueryStatus<N: Next> {
    Idle,
    Pending,
    Success(Arc<N>),
    Error(String),
}

// Manual Clone — `derive(Clone)` adds an `N: Clone` bound that none of
// the variants actually need (Success holds `Arc<N>`).
impl<N: Next> Clone for QueryStatus<N> {
    fn clone(&self) -> Self {
        match self {
            QueryStatus::Idle          => QueryStatus::Idle,
            QueryStatus::Pending       => QueryStatus::Pending,
            QueryStatus::Success(d)    => QueryStatus::Success(d.clone()),
            QueryStatus::Error(m)      => QueryStatus::Error(m.clone()),
        }
    }
}

pub trait QueryFn<N: Next>: Send + Sync + 'static {
    fn ident(&self) -> &'static str;
    fn run(&self, input: &N) -> N;
}

pub struct QueryCache<N: Next> {
    by_key: Mutex<HashMap<NextKey, (QueryStatus<N>, Vec<&'static str>)>>,
}

impl<N: Next> QueryCache<N> {
    pub fn new() -> Self {
        Self { by_key: Mutex::new(HashMap::new()) }
    }

    pub fn status(&self, key: NextKey) -> QueryStatus<N> {
        match self.by_key.lock().unwrap().get(&key) {
            Some((s, _)) => s.clone(),
            None         => QueryStatus::Idle,
        }
    }

    pub fn set_pending(&self, key: NextKey, domains: Vec<&'static str>) {
        self.by_key.lock().unwrap()
            .insert(key, (QueryStatus::Pending, domains));
    }

    pub fn set_success(&self, key: NextKey, data: Arc<N>, domains: Vec<&'static str>) {
        self.by_key.lock().unwrap()
            .insert(key, (QueryStatus::Success(data), domains));
    }

    pub fn set_error(&self, key: NextKey, msg: String, domains: Vec<&'static str>) {
        self.by_key.lock().unwrap()
            .insert(key, (QueryStatus::Error(msg), domains));
    }

    pub fn invalidate_domain(&self, domain: &str) -> usize {
        let mut by = self.by_key.lock().unwrap();
        let before = by.len();
        by.retain(|_, (_, doms)| !doms.iter().any(|d| *d == domain));
        before - by.len()
    }

    pub fn len(&self) -> usize { self.by_key.lock().unwrap().len() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

impl<N: Next> Default for QueryCache<N> {
    fn default() -> Self { Self::new() }
}

impl<N: Next> BusListener for QueryCache<N> {
    fn on_event(&self, ev: &Event) {
        if let Event::DomainDirty(d) = ev {
            self.invalidate_domain(d);
        }
    }
}

pub fn attach_query_cache_to_bus<N: Next>(cache: Arc<QueryCache<N>>, bus: &EventBus) {
    bus.add_listener(cache);
}

pub struct Query<N: Next, F: QueryFn<N>> {
    fn_:     Arc<F>,
    cache:   Arc<QueryCache<N>>,
    bus:     Arc<EventBus>,
    spawner: Arc<dyn Spawner>,
    domains: Vec<&'static str>,
}

impl<N: Next, F: QueryFn<N>> Query<N, F> {
    pub fn new(
        fn_:     F,
        cache:   Arc<QueryCache<N>>,
        bus:     Arc<EventBus>,
        spawner: Arc<dyn Spawner>,
    ) -> Self {
        Self { fn_: Arc::new(fn_), cache, bus, spawner, domains: Vec::new() }
    }

    pub fn with_domain(mut self, d: &'static str) -> Self {
        self.domains.push(d);
        self
    }

    fn key_for(&self, input_hash: &[u8; 32]) -> NextKey {
        let mut h = blake3::Hasher::new();
        h.update(self.fn_.ident().as_bytes());
        h.update(input_hash);
        NextKey(*h.finalize().as_bytes())
    }
}

// `Clone` bound on the carrier: render needs to keep a copy for the
// spawned closure and another for the parked Suspense. Real-world
// carriers (Cursor, primitives) are all Clone; tests prove the bound.
impl<N: Next + Clone, F: QueryFn<N>> Component for Query<N, F> {
    type Next = N;

    fn render(&self, _: &RenderCtx, c: &N) -> Node<N> {
        let key = self.key_for(&c.content_hash());
        match self.cache.status(key) {
            QueryStatus::Success(data) => Node::Emit(data),

            QueryStatus::Pending => Node::Yield {
                value: Arc::new(c.clone()),
                wake:  Wake::Key(key),
            },

            QueryStatus::Idle => {
                self.cache.set_pending(key, self.domains.clone());
                let fn_     = self.fn_.clone();
                let cache   = self.cache.clone();
                let bus     = self.bus.clone();
                let domains = self.domains.clone();
                let input   = c.clone();
                self.spawner.spawn(Box::new(move || {
                    let result = fn_.run(&input);
                    cache.set_success(key, Arc::new(result), domains);
                    bus.dispatch(Event::KeyDirty(key));
                }));
                Node::Yield {
                    value: Arc::new(c.clone()),
                    wake:  Wake::Key(key),
                }
            }

            QueryStatus::Error(_) => Node::Done,
        }
    }
}
