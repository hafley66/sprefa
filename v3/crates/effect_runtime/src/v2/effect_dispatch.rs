//! `EffectDispatch<T>` + `Spawner` — saga-flavored effect surface.
//!
//! React-query / redux-saga vocabulary:
//!   - `mutationFn`: a function that produces a `T`. Runs off the
//!     driver thread (thread or tokio task — picked by `Spawner`).
//!   - "dispatch" hands a `mutationFn` and a `NextKey` to the
//!     spawner. When the function returns, the result is `put` into
//!     a `MutationStore<T>` and the bus dispatches `KeyDirty(key)`.
//!     The Component that returned `Suspense{Key(key)}` resumes at
//!     pc+1, reads the result via `MutationStore::take`.
//!
//! The Spawner is the runtime seam. `ThreadSpawner` uses
//! `std::thread::spawn` — zero runtime dependency, the no-tokio path.
//! `TokioSpawner` uses `tokio::task::spawn_blocking` — for consumers
//! that already host a runtime and want to share its pool. Either
//! choice is invisible to the Component.

use std::sync::Arc;

use super::event_bus::{Event, EventBus};
use super::mutation_store::MutationStore;
use super::next::Next;
use super::next_key::NextKey;

pub trait Spawner: Send + Sync + 'static {
    /// Run `f` off the driver thread. Synchronous body — async effects
    /// wrap their own runtime inside `f` (e.g. `block_on`) or build on
    /// a `Spawner` impl that accepts futures.
    fn spawn(&self, f: Box<dyn FnOnce() + Send + 'static>);
}

pub struct ThreadSpawner;

impl Spawner for ThreadSpawner {
    fn spawn(&self, f: Box<dyn FnOnce() + Send + 'static>) {
        std::thread::spawn(f);
    }
}

pub struct TokioSpawner;

impl Spawner for TokioSpawner {
    fn spawn(&self, f: Box<dyn FnOnce() + Send + 'static>) {
        // spawn_blocking moves the closure to tokio's blocking pool —
        // appropriate for sync mutationFns. Async-shaped mutations want
        // a different Spawner that accepts BoxFuture; trivial to add.
        tokio::task::spawn_blocking(f);
    }
}

pub struct EffectDispatch<T: Next> {
    pub bus:     Arc<EventBus>,
    pub store:   Arc<MutationStore<T>>,
    pub spawner: Arc<dyn Spawner>,
}

impl<T: Next> EffectDispatch<T> {
    pub fn new(
        bus:     Arc<EventBus>,
        store:   Arc<MutationStore<T>>,
        spawner: Arc<dyn Spawner>,
    ) -> Self {
        Self { bus, store, spawner }
    }

    /// Fire `mutation_fn` off-thread, route its result back through
    /// `store` keyed by `key`, and notify the bus when ready. The
    /// caller typically returns `Suspense{Key(key)}` from `render` so
    /// the parked row at pc+1 picks up the result.
    pub fn dispatch<F>(&self, key: NextKey, mutation_fn: F)
    where
        F: FnOnce() -> T + Send + 'static,
    {
        let bus   = self.bus.clone();
        let store = self.store.clone();
        self.spawner.spawn(Box::new(move || {
            let result = mutation_fn();
            store.put(key, Arc::new(result));
            bus.dispatch(Event::KeyDirty(key));
        }));
    }
}
