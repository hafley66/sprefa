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

pub mod next;
pub mod next_key;
pub mod event_bus;
pub mod wake;
pub mod row;
pub mod fact_store;
pub mod codec;
pub mod effect_dispatch;
pub mod memoize;
pub mod query;
#[cfg(feature = "sqlite")]
pub mod sqlite_queue;
#[cfg(feature = "sqlite")]
pub mod hybrid_queue;
pub mod node;
pub mod component;
pub mod diag;
pub mod queue;
pub mod mem_queue;
pub mod flatten;
pub mod prior_children;
pub mod expand;

pub use next::Next;
pub use next_key::{compute_key, NextKey};
pub use event_bus::{Event, EventBus};
pub use wake::Wake;
pub use row::Row;
pub use fact_store::{FactStore, MemFactStore, fact_domain, fact_dirty_key};
#[cfg(feature = "sqlite")]
pub use fact_store::SqliteFactStore;
pub use codec::Codec;
pub use diag::{ByteRange, Diag, DiagSink, NoopDiagSink, Severity};
pub use effect_dispatch::{EffectDispatch, Spawner, ThreadSpawner, TokioSpawner};
pub use event_bus::BusListener;
pub use memoize::{
    attach_cache_to_bus, attach_cache_to_bus_with_queue,
    CacheCascadeListener, MemoCache, MemoKey, Memoize,
};
pub use query::{attach_query_cache_to_bus, Query, QueryCache, QueryFn, QueryStatus};
#[cfg(feature = "sqlite")]
pub use sqlite_queue::SqliteQueue;
#[cfg(feature = "sqlite")]
pub use hybrid_queue::{HybridCfg, HybridQueue};
pub use node::Node;
pub use component::{par_render, Component, DynComponent, RenderCtx};
pub use flatten::{splice_into, splice_into_at, splice_into_recorded, splice_into_recorded_at};
pub use prior_children::{diff_children, ChildDiff, ContentHash, PriorChildIndex};
pub use queue::{ExpandTick, InstanceId, PipeHash, QueueBackend, QueueId, QueueRow};
pub use mem_queue::MemQueue;
pub use expand::{expand, ExpandOpts, ExpandStats, Pipe, PipeInstance, DEFAULT_BATCH_CAP};

#[cfg(test)]
mod tests;
