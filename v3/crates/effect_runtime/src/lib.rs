//! v3 effect-dispatch prototype.
//!
//! Validates that the op authoring surface collapses to one file per
//! (effect + batcher) by proving:
//!
//! 1. `ctx.put(Effect).await` returns a typed `E::Response` with zero
//!    `Any`/downcast visible to the author.
//! 2. Adding a new effect kind touches exactly one new file. Framework
//!    core never grows.
//! 3. Two effect kinds coexist in one registry, each monomorphized.
//!
//! Framework core is the ~140 LoC below. Everything else is ops.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;

use arc_swap::ArcSwap;
pub use tokio_util::sync::CancellationToken;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A request type bundled with its response type. One impl per effect.
///
/// Optional size hints (`payload_bytes`, `response_bytes`) feed the
/// core telemetry surface. When overridden, the framework records
/// per-effect throughput (MB/s) with zero measurement code in the op.
pub trait EffectKind: Send + 'static {
    type Response: Send + 'static;

    /// Bytes that the framework should attribute to this put for
    /// throughput rollups.
    fn payload_bytes(&self) -> Option<usize> { None }

    /// Bytes delivered back.
    fn response_bytes(_r: &Self::Response) -> Option<usize> { None }
}

/// A read-like effect: idempotent, keyable, cacheable. Implement when a
/// distinct (Key, Response) pairing can be memoized and invalidated as
/// a unit. `DOMAIN` names the invalidation bucket — writes to the same
/// domain clear every `PureEffect` tagged with it.
pub trait PureEffect: EffectKind {
    type Key: Hash + Eq + Clone + Send + Sync + 'static;
    const DOMAIN: &'static str;
    fn cache_key(&self) -> Self::Key;
}

/// Owns the side-effectful dispatch for one effect kind. Policy
/// (passthrough / opportunistic / windowed) lives inside `run`.
///
/// `cancel` is cooperative. Long-running batchers should `select!` on
/// `cancel.cancelled()` and drop reply channels; short/CPU-bound work
/// can ignore it. Callers that drop the returned future also signal
/// cancellation via Rust's normal drop semantics.
pub trait Batcher<E: EffectKind>: Send + Sync + 'static {
    fn run(&self, req: E, cancel: CancellationToken) -> BoxFuture<'static, E::Response>;
}

/// Sharing a single batcher across many `RtCtx` builds (the v3 server's
/// long-lived `BoundedWorkSteal<AstParseEffect>` is the load-bearing
/// case) only needs an `Arc` to be `Batcher<E>` itself. The blanket
/// impl forwards through the deref so registering `arc.clone()` Just
/// Works in any builder slot.
impl<E, B> Batcher<E> for Arc<B>
where
    E: EffectKind,
    B: Batcher<E> + ?Sized,
{
    fn run(&self, req: E, cancel: CancellationToken) -> BoxFuture<'static, E::Response> {
        (**self).run(req, cancel)
    }
}

// --- type-erasure layer. Invisible to op authors. ---

trait BatcherEntry: Send + Sync {
    fn submit(
        &self,
        req: Box<dyn Any + Send>,
        cancel: CancellationToken,
    ) -> BoxFuture<'static, Box<dyn Any + Send>>;

    /// Framework hook: drop cached state tagged with `domain`. Non-cache
    /// entries return without doing anything.
    fn invalidate_domain(&self, _domain: &str) {}
}

struct TypedEntry<E: EffectKind, B: Batcher<E>> {
    batcher: B,
    _phantom: PhantomData<fn() -> E>,
}

impl<E, B> BatcherEntry for TypedEntry<E, B>
where
    E: EffectKind,
    B: Batcher<E>,
{
    fn submit(
        &self,
        req: Box<dyn Any + Send>,
        cancel: CancellationToken,
    ) -> BoxFuture<'static, Box<dyn Any + Send>> {
        let typed: E = *req
            .downcast::<E>()
            .expect("framework invariant: TypeId matches E");
        let fut = self.batcher.run(typed, cancel);
        Box::pin(async move {
            let r = fut.await;
            Box::new(r) as Box<dyn Any + Send>
        })
    }
}

/// Framework hook used by `CacheLayer` (and any future domain-aware
/// batcher) to participate in domain invalidation without leaking
/// knowledge of cache internals into the core.
pub trait DomainInvalidate: Send + Sync + 'static {
    fn invalidate_domain(&self, domain: &str);
}

struct DomainAwareEntry<E: EffectKind, B: Batcher<E> + DomainInvalidate> {
    batcher: B,
    _phantom: PhantomData<fn() -> E>,
}

impl<E, B> BatcherEntry for DomainAwareEntry<E, B>
where
    E: EffectKind,
    B: Batcher<E> + DomainInvalidate,
{
    fn submit(
        &self,
        req: Box<dyn Any + Send>,
        cancel: CancellationToken,
    ) -> BoxFuture<'static, Box<dyn Any + Send>> {
        let typed: E = *req
            .downcast::<E>()
            .expect("framework invariant: TypeId matches E");
        let fut = self.batcher.run(typed, cancel);
        Box::pin(async move {
            let r = fut.await;
            Box::new(r) as Box<dyn Any + Send>
        })
    }
    fn invalidate_domain(&self, domain: &str) {
        self.batcher.invalidate_domain(domain);
    }
}

type Registry = HashMap<TypeId, Arc<dyn BatcherEntry>>;

// --- store TypeMap ---
//
// State stores live alongside effect handlers in the ctx. Different role
// from the effect Registry: handlers dispatch typed effect calls; stores
// hold mutable shared state that batchers and outside code (LSP server,
// CLI runner) reach into. Two examples planned: SubjectRegistry (Yield),
// TagStore (tag op rows). Marker trait keeps registration explicit; ops
// opt types in via `impl Store for X {}`.

/// Marker trait for state stores held by `RtCtx`. Stored as
/// `Arc<dyn Any + Send + Sync>` and looked up by `TypeId::of::<S>()`.
/// `S` implementations decide their own internal sync (Mutex, DashMap,
/// RwLock, watch::Receiver, etc.) — the runtime is sync-agnostic.
pub trait Store: Any + Send + Sync + 'static {}

type Stores = HashMap<TypeId, Arc<dyn Any + Send + Sync>>;

/// The runtime context. Clone-cheap (`Arc` inside). Ops call
/// `ctx.put(effect).await`. The registry lives behind an `ArcSwap` so
/// handlers can be rebound without rebuilding the ctx (e.g. config
/// reload, test substitution). The store TypeMap is the parallel
/// channel for stateful capabilities.
#[derive(Clone)]
pub struct RtCtx {
    registry: Arc<ArcSwap<Registry>>,
    stores: Arc<ArcSwap<Stores>>,
    root_cancel: CancellationToken,
    telemetry: telemetry::Telemetry,
}

impl Default for RtCtx {
    fn default() -> Self {
        RtCtxBuilder::new().build()
    }
}

impl RtCtx {
    /// Submit an effect and await its typed response. The per-put
    /// cancellation token is a child of the ctx's root — calling
    /// `ctx.cancel_all()` signals every in-flight put.
    pub fn put<E: EffectKind>(&self, e: E) -> BoxFuture<'static, E::Response> {
        self.put_with_cancel(e, self.root_cancel.child_token())
    }

    /// Submit an effect with a caller-supplied cancellation token. The
    /// token is still a descendant of the root — either source can
    /// cancel the work.
    pub fn put_with_cancel<E: EffectKind>(
        &self,
        e: E,
        cancel: CancellationToken,
    ) -> BoxFuture<'static, E::Response> {
        let snapshot = self.registry.load();
        let entry = snapshot
            .get(&TypeId::of::<E>())
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "effect kind not registered: {} (registered: {} kinds)",
                    std::any::type_name::<E>(),
                    snapshot.len(),
                )
            });
        let payload_bytes = e.payload_bytes();
        let span = self.telemetry.start::<E>(payload_bytes);
        let any_in: Box<dyn Any + Send> = Box::new(e);
        Box::pin(async move {
            let any_out = entry.submit(any_in, cancel).await;
            let response: E::Response = *any_out
                .downcast::<E::Response>()
                .expect("framework invariant: response matches E::Response");
            let resp_bytes = E::response_bytes(&response);
            span.close(resp_bytes);
            response
        })
    }

    /// Rebind the handler for one effect kind. Stored atomically; new
    /// puts pick it up, in-flight puts continue against the previous
    /// entry. Writes a fresh registry snapshot.
    pub fn rebind<E, B>(&self, batcher: B)
    where
        E: EffectKind,
        B: Batcher<E>,
    {
        let entry: Arc<dyn BatcherEntry> = Arc::new(TypedEntry::<E, B> {
            batcher,
            _phantom: PhantomData,
        });
        self.registry.rcu(|prev| {
            let mut next = (**prev).clone();
            next.insert(TypeId::of::<E>(), entry.clone());
            Arc::new(next)
        });
    }

    /// Rebind to a batcher that participates in domain invalidation.
    pub fn rebind_domain_aware<E, B>(&self, batcher: B)
    where
        E: EffectKind,
        B: Batcher<E> + DomainInvalidate,
    {
        let entry: Arc<dyn BatcherEntry> = Arc::new(DomainAwareEntry::<E, B> {
            batcher,
            _phantom: PhantomData,
        });
        self.registry.rcu(|prev| {
            let mut next = (**prev).clone();
            next.insert(TypeId::of::<E>(), entry.clone());
            Arc::new(next)
        });
    }

    /// Drop cached entries in every batcher tagged with `domain`.
    /// Called after a write that mutates shared state (e.g. a
    /// `WriteBytes` to the fs domain invalidates `ReadBytes` caches).
    pub fn invalidate_domain(&self, domain: &str) {
        for entry in self.registry.load().values() {
            entry.invalidate_domain(domain);
        }
    }

    /// Signal every in-flight put to cancel. Batchers that cooperate
    /// will drop work. The root token is consumed; use
    /// `fresh_cancel_root` on the builder if you need a fresh ctx.
    pub fn cancel_all(&self) {
        self.root_cancel.cancel();
    }

    /// The root cancellation token. Clone for listeners that want to
    /// observe shutdown without initiating it.
    pub fn root_cancel(&self) -> CancellationToken {
        self.root_cancel.clone()
    }

    /// Access the telemetry sink for this ctx.
    pub fn telemetry(&self) -> &telemetry::Telemetry { &self.telemetry }

    /// Fetch a store registered at builder time (or via `bind_store`).
    /// Returns `None` if no store of type `S` was registered. Callers
    /// that require the store should `.expect()` with a message naming
    /// the consumer ("tag op requires TagStore registration").
    pub fn store<S: Store>(&self) -> Option<Arc<S>> {
        let snapshot = self.stores.load();
        snapshot
            .get(&TypeId::of::<S>())
            .cloned()
            .and_then(|a| Arc::downcast::<S>(a).ok())
    }

    /// Bind or replace a store after build. Atomic via ArcSwap, same
    /// shape as `rebind` for effect handlers. New `store::<S>()` calls
    /// see the new value; existing `Arc<S>` clones held elsewhere keep
    /// pointing at the old value.
    pub fn bind_store<S: Store>(&self, s: Arc<S>) {
        let entry: Arc<dyn Any + Send + Sync> = s;
        self.stores.rcu(|prev| {
            let mut next = (**prev).clone();
            next.insert(TypeId::of::<S>(), entry.clone());
            Arc::new(next)
        });
    }
}

pub struct RtCtxBuilder {
    registry: Registry,
    stores: Stores,
}

impl Default for RtCtxBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RtCtxBuilder {
    pub fn new() -> Self {
        Self { registry: HashMap::new(), stores: HashMap::new() }
    }

    /// Register a state store. Lookup-by-type via `cx.store::<S>()`.
    /// Replaces any prior store of the same type.
    pub fn with_store<S: Store>(mut self, s: Arc<S>) -> Self {
        let entry: Arc<dyn Any + Send + Sync> = s;
        self.stores.insert(TypeId::of::<S>(), entry);
        self
    }

    pub fn register<E, B>(mut self, batcher: B) -> Self
    where
        E: EffectKind,
        B: Batcher<E>,
    {
        let entry: Arc<dyn BatcherEntry> = Arc::new(TypedEntry::<E, B> {
            batcher,
            _phantom: PhantomData,
        });
        self.registry.insert(TypeId::of::<E>(), entry);
        self
    }

    /// Register a batcher that participates in domain invalidation.
    pub fn register_domain_aware<E, B>(mut self, batcher: B) -> Self
    where
        E: EffectKind,
        B: Batcher<E> + DomainInvalidate,
    {
        let entry: Arc<dyn BatcherEntry> = Arc::new(DomainAwareEntry::<E, B> {
            batcher,
            _phantom: PhantomData,
        });
        self.registry.insert(TypeId::of::<E>(), entry);
        self
    }

    /// Sugar for pure reads: wraps `inner` in a `CacheLayer` sized at
    /// `capacity` entries and registers it as domain-aware. Equivalent
    /// to `register_domain_aware::<E, _>(CacheLayer::new(capacity, inner))`.
    pub fn register_pure<E, B>(self, capacity: u64, inner: B) -> Self
    where
        E: PureEffect,
        E::Response: Clone + Send + Sync + 'static,
        B: Batcher<E>,
    {
        self.register_domain_aware::<E, _>(batchers::CacheLayer::new(capacity, inner))
    }

    pub fn build(self) -> RtCtx {
        RtCtx {
            registry: Arc::new(ArcSwap::from_pointee(self.registry)),
            stores: Arc::new(ArcSwap::from_pointee(self.stores)),
            root_cancel: CancellationToken::new(),
            telemetry: telemetry::Telemetry::new(),
        }
    }
}

pub mod batchers;
pub mod subjects;
pub mod telemetry;

pub use subjects::{
    Lineage, SubjectKey, SubjectKind, SubjectRegistry, Unsubscribed, Yield,
    YieldBatcher,
};

#[cfg(feature = "rx")]
pub mod rx;

// Effect examples live in the consumer crate (`effect_proof`). The
// framework stays domain-neutral.

#[cfg(test)]
mod store_typemap_tests {
    use super::*;
    use std::sync::Mutex;

    struct CountStore { hits: Mutex<u32> }
    impl Store for CountStore {}

    struct OtherStore { name: &'static str }
    impl Store for OtherStore {}

    #[test]
    fn store_typemap_register_fetch_rebind() {
        let s = Arc::new(CountStore { hits: Mutex::new(0) });
        let other = Arc::new(OtherStore { name: "alpha" });
        let cx = RtCtxBuilder::new()
            .with_store(s.clone())
            .with_store(other.clone())
            .build();

        let fetched = cx.store::<CountStore>().expect("registered");
        *fetched.hits.lock().unwrap() += 1;
        assert_eq!(*s.hits.lock().unwrap(), 1, "Arc shared, not cloned-by-value");

        let other_fetched = cx.store::<OtherStore>().expect("registered");
        assert_eq!(other_fetched.name, "alpha");

        struct Unregistered;
        impl Store for Unregistered {}
        assert!(cx.store::<Unregistered>().is_none());

        // bind_store rebinds atomically; new lookups see the new value.
        let s2 = Arc::new(CountStore { hits: Mutex::new(99) });
        cx.bind_store(s2.clone());
        let after = cx.store::<CountStore>().expect("rebound");
        assert_eq!(*after.hits.lock().unwrap(), 99);
        // Old Arc still valid, still points at the old value.
        assert_eq!(*s.hits.lock().unwrap(), 1);
    }
}
