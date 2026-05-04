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
pub mod mutation_store;
#[cfg(feature = "sqlite")]
pub mod sqlite_mutation_store;
pub mod codec;
pub mod effect_dispatch;
pub mod memoize;
pub mod query;
#[cfg(feature = "sqlite")]
pub mod sqlite_queue;
pub mod node;
pub mod component;
pub mod queue;
pub mod mem_queue;
pub mod flatten;
pub mod driver;

pub use next::Next;
pub use next_key::{compute_key, NextKey};
pub use event_bus::{Event, EventBus};
pub use wake::Wake;
pub use mutation_store::MutationStore;
#[cfg(feature = "sqlite")]
pub use sqlite_mutation_store::SqliteMutationStore;
pub use codec::Codec;
pub use effect_dispatch::{EffectDispatch, Spawner, ThreadSpawner, TokioSpawner};
pub use event_bus::BusListener;
pub use memoize::{attach_cache_to_bus, MemoCache, MemoKey, Memoize};
pub use query::{attach_query_cache_to_bus, Query, QueryCache, QueryFn, QueryStatus};
#[cfg(feature = "sqlite")]
pub use sqlite_queue::SqliteQueue;
pub use node::Node;
pub use component::{par_render, Component, DynComponent, RenderCtx};
pub use flatten::splice_into;
pub use queue::{DriveTick, InstanceId, PipeHash, QueueBackend, QueueId, QueueRow};
pub use mem_queue::MemQueue;
pub use driver::{drive, DriveOpts, DriveStats, PipeInstance, DEFAULT_BATCH_CAP};

#[cfg(test)]
mod tests;
