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
//! Framework core is the ~80 LoC below. Everything else is ops.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A request type bundled with its response type. One impl per effect.
///
/// Optional size hints (`payload_bytes`, `response_bytes`) feed the
/// core telemetry surface. When overridden, the framework records
/// per-effect throughput (MB/s) with zero measurement code in the op.
pub trait EffectKind: Send + 'static {
    type Response: Send + 'static;

    /// Bytes that the framework should attribute to this put for
    /// throughput rollups. Default: no hint. Override in ops whose
    /// natural accounting unit is bytes (file reads, blob fetches,
    /// insert payloads, etc.).
    fn payload_bytes(&self) -> Option<usize> { None }

    /// Bytes delivered back. Most callers ignore this; it matters for
    /// effects where the response size drives downstream cost.
    fn response_bytes(_r: &Self::Response) -> Option<usize> { None }
}

/// Owns the side-effectful dispatch for one effect kind. Policy
/// (passthrough / opportunistic / windowed) lives inside `run`.
pub trait Batcher<E: EffectKind>: Send + Sync + 'static {
    fn run(&self, req: E) -> BoxFuture<'static, E::Response>;
}

// --- type-erasure layer. Invisible to op authors. ---

trait BatcherEntry: Send + Sync {
    fn submit(
        &self,
        req: Box<dyn Any + Send>,
    ) -> BoxFuture<'static, Box<dyn Any + Send>>;
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
    ) -> BoxFuture<'static, Box<dyn Any + Send>> {
        let typed: E = *req
            .downcast::<E>()
            .expect("framework invariant: TypeId matches E");
        let fut = self.batcher.run(typed);
        Box::pin(async move {
            let r = fut.await;
            Box::new(r) as Box<dyn Any + Send>
        })
    }
}

/// The runtime context. Clone-cheap (`Arc` inside). Ops call
/// `ctx.put(effect).await`. Every put opens a telemetry span on the
/// ctx's `Telemetry` sink; retrieve the summary via
/// `ctx.telemetry().summary()` after a run.
#[derive(Clone, Default)]
pub struct RtCtx {
    registry: Arc<HashMap<TypeId, Arc<dyn BatcherEntry>>>,
    telemetry: telemetry::Telemetry,
}

impl RtCtx {
    /// Submit an effect and await its typed response.
    ///
    /// The framework opens a telemetry span at entry, pulls size
    /// hints via `EffectKind::payload_bytes`, closes the span on
    /// response arrival with `EffectKind::response_bytes` applied to
    /// the returned value. Authors see zero measurement code.
    pub fn put<E: EffectKind>(&self, e: E) -> BoxFuture<'static, E::Response> {
        let entry = self
            .registry
            .get(&TypeId::of::<E>())
            .cloned()
            .expect("effect kind not registered");
        let payload_bytes = e.payload_bytes();
        let span = self.telemetry.start::<E>(payload_bytes);
        let any_in: Box<dyn Any + Send> = Box::new(e);
        Box::pin(async move {
            let any_out = entry.submit(any_in).await;
            let response: E::Response = *any_out
                .downcast::<E::Response>()
                .expect("framework invariant: response matches E::Response");
            let resp_bytes = E::response_bytes(&response);
            span.close(resp_bytes);
            response
        })
    }

    /// Access the telemetry sink for this ctx.
    pub fn telemetry(&self) -> &telemetry::Telemetry { &self.telemetry }
}

pub struct RtCtxBuilder {
    registry: HashMap<TypeId, Arc<dyn BatcherEntry>>,
}

impl Default for RtCtxBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RtCtxBuilder {
    pub fn new() -> Self {
        Self { registry: HashMap::new() }
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

    pub fn build(self) -> RtCtx {
        RtCtx {
            registry: Arc::new(self.registry),
            telemetry: telemetry::Telemetry::new(),
        }
    }
}

pub mod batchers;
pub mod telemetry;

// Effect examples live in the consumer crate (`effect_proof`). The
// framework stays domain-neutral.
