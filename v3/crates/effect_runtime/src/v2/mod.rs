//! v2 — React-shaped runtime, generic over the carrier value.
//!
//! Lab inside `effect_runtime`. The carrier (`Next`) is whatever
//! payload the language wants to flow through the queue. `LabCursor`
//! (in tests) is the demo: a sorted list of `(name, value)` terms,
//! same shape as sprefa's `Cursor`.
//!
//! Phase A surface: Next + NextKey + EventBus. Wake is dispatched via
//! the bus (`KeyDirty` / `PathDirty` / `DomainDirty`). Wake +
//! cache-invalidation share one mechanism.

pub mod next;
pub mod next_key;
pub mod event_bus;
pub mod wake;
pub mod mutation_store;
pub mod sqlite_mutation_store;
pub mod codec;
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
pub use sqlite_mutation_store::SqliteMutationStore;
pub use codec::Codec;
pub use sqlite_queue::SqliteQueue;
pub use node::Node;
pub use component::{Component, DynComponent, RenderCtx};
pub use queue::{DriveTick, InstanceId, PipeHash, QueueBackend, QueueId, QueueRow};
pub use mem_queue::MemQueue;
pub use driver::{drive, DriveOpts, DriveStats, PipeInstance};

#[cfg(test)]
mod tests;
