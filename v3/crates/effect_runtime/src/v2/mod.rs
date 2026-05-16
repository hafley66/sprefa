//! v2 — React-shaped runtime, generic over the carrier value.
//!
//! Lab inside `effect_runtime`. The carrier (`Next`) is whatever
//! payload the language wants to flow through the queue. `LabCursor`
//! (in tests) is the demo: a sorted list of `(name, value)` terms,
//! same shape as sprefa's `Cursor`.
//!
//! Park-as-row surface: wake subscriptions live on queue rows
//! (`Wake::Key { domain, key }`) and get promoted by
//! `QueueBackend::dispatch_park`. EventBus is cache fan-out only
//! (`Event::Dirty { domain, key }`).

pub mod codec;
pub mod component;
pub mod diag;
pub mod effect_dispatch;
pub mod event_bus;
pub mod expand;
pub mod fact_store;
pub mod flatten;
#[cfg(feature = "sqlite")]
pub mod hybrid_queue;
pub mod mem_queue;
pub mod memoize;
pub mod next;
pub mod next_key;
pub mod node;
pub mod prior_children;
pub mod probe;
pub mod query;
pub mod queue;
pub mod row;
pub mod runtime_graph;
#[cfg(feature = "sqlite")]
pub mod sqlite_queue;
pub mod wake;

pub use codec::Codec;
pub use component::{
    par_render, Component, ComponentLifecycle, DynComponent, EffectTelemetry,
    EffectTelemetrySnapshot, Purity, RenderCtx, RuntimePut,
};
pub use diag::{ByteRange, Diag, DiagSink, NoopDiagSink, Severity};
pub use effect_dispatch::{EffectDispatch, Spawner, ThreadSpawner, TokioSpawner};
pub use event_bus::BusListener;
pub use event_bus::{Event, EventBus};
pub use expand::{expand, ExpandOpts, ExpandStats, Pipe, PipeInstance, DEFAULT_BATCH_CAP};
#[cfg(feature = "sqlite")]
pub use fact_store::SqliteFactStore;
pub use fact_store::{
    content_id, row_dirty_key, table_dirty_key, FactStore, MemFactStore, ID_COL, ROW_DOMAIN,
    TABLE_DOMAIN,
};
#[cfg(feature = "sqlite")]
pub use fact_store::{register_sprf_udfs, PragmaProfile};
pub use flatten::{splice_into, splice_into_at, splice_into_recorded, splice_into_recorded_at};
#[cfg(feature = "sqlite")]
pub use hybrid_queue::{HybridCfg, HybridQueue};
pub use mem_queue::MemQueue;
pub use memoize::{
    attach_cache_to_bus, attach_cache_to_bus_with_queue, CacheCascadeListener, MemoCache, MemoKey,
    Memoize,
};
pub use next::Next;
pub use next_key::{compute_key, NextKey};
pub use node::Node;
pub use prior_children::{diff_children, ChildDiff, ContentHash, PriorChildIndex};
pub use probe::{BufferProbeSink, NoopProbeSink, Probe, ProbeSink};
pub use query::{attach_query_cache_to_bus, Query, QueryCache, QueryFn, QueryStatus};
pub use queue::{
    attach_dirty_to_queue, BarrierScope, DirtyQueuePromoter, ExpandTick, InstanceId,
    PendingSummary, PipeHash, QueueBackend, QueueId, QueueRow,
};
pub use row::Row;
pub use runtime_graph::{
    ActiveChild, DirtyOwner, EmitValue, FactRuntimeGraph, GraphRef, NodeId, NodeKind,
    QuiescenceError, RuntimeContinuation, RuntimeEdge, RuntimeNode, RuntimeValue,
    RuntimeValuePayload, SubResult,
    Subscribe, SupportRows, TraversalOrder, VisibleDelta,
};
#[cfg(feature = "sqlite")]
pub use sqlite_queue::SqliteQueue;
pub use wake::Wake;

#[cfg(test)]
mod tests;
