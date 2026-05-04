//! `EffectDispatch<T>` + `Spawner` — saga-flavored effect surface.
//!
//! React-query / redux-saga vocabulary:
//!   - `mutationFn`: a function that produces a `T`. Runs off the
//!     driver thread (thread or tokio task — picked by `Spawner`).
//!   - "dispatch" hands a `mutationFn` and a `NextKey` to the
//!     spawner. When the function returns, the result is `put` into
//!     a `MutationStore<T>` and the queue's `dispatch_park` promotes
//!     any parked rows on `(domain, key)` to runnable. The Component
//!     that returned `Yield { wake: Wake::Key { domain, key } }` re-
//!     renders in place after wake and reads the result via
//!     `MutationStore::take`.
//!
//! The Spawner is the runtime seam. `ThreadSpawner` uses
//! `std::thread::spawn` — zero runtime dependency, the no-tokio path.
//! `TokioSpawner` uses `tokio::task::spawn_blocking` — for consumers
//! that already host a runtime and want to share its pool. Either
//! choice is invisible to the Component.

use std::borrow::Cow;
use std::sync::Arc;

use super::mutation_store::MutationStore;
use super::next::Next;
use super::next_key::NextKey;
use super::queue::QueueBackend;

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

/// Default park domain for `EffectDispatch`. Components that share
/// dispatchers across distinct domains construct one `EffectDispatch`
/// per domain via `with_domain`.
pub const DEFAULT_EFFECT_DOMAIN: &str = "effect";

pub struct EffectDispatch<T: Next> {
    pub store:   Arc<MutationStore<T>>,
    pub spawner: Arc<dyn Spawner>,
    pub queue:   Arc<dyn QueueBackend<T>>,
    pub domain:  Cow<'static, str>,
}

impl<T: Next> EffectDispatch<T> {
    pub fn new(
        store:   Arc<MutationStore<T>>,
        spawner: Arc<dyn Spawner>,
        queue:   Arc<dyn QueueBackend<T>>,
    ) -> Self {
        Self {
            store,
            spawner,
            queue,
            domain: Cow::Borrowed(DEFAULT_EFFECT_DOMAIN),
        }
    }

    pub fn with_domain(mut self, d: impl Into<Cow<'static, str>>) -> Self {
        self.domain = d.into();
        self
    }

    /// Fire `mutation_fn` off-thread, route its result back through
    /// `store` keyed by `key`, then promote every parked row on
    /// `(self.domain, key)` to runnable. The caller typically returns
    /// `Yield { wake: Wake::Key { domain: self.domain, key } }` from
    /// `render` so the parker re-renders after wake and picks up the
    /// result.
    pub fn dispatch<F>(&self, key: NextKey, mutation_fn: F)
    where
        F: FnOnce() -> T + Send + 'static,
    {
        let store  = self.store.clone();
        let queue  = self.queue.clone();
        let domain = self.domain.clone();
        self.spawner.spawn(Box::new(move || {
            let result = mutation_fn();
            store.put(key, Arc::new(result));
            queue.dispatch_park(domain.as_ref(), Some(key));
        }));
    }
}
