//! `EffectDispatch<T>` + `Spawner` — saga-flavored effect surface.
//!
//! React-query / redux-saga vocabulary:
//!   - `mutationFn`: a function that produces a `T`. Runs off the
//!     driver thread (thread or tokio task — picked by `Spawner`).
//!   - "dispatch" hands a `mutationFn` and a `NextKey` to the spawner.
//!     When the function returns, the result is stamped with
//!     `MUTATION_KEY_COL = hex(key)` and `insert`-ed into the
//!     dispatcher's `FactStore` under `table` (default
//!     `mutation_results`). Then the queue's `dispatch_park` promotes
//!     any parked rows on `(domain, key)` to runnable. The Component
//!     that returned `Yield { wake: Wake::Key { domain, key } }` re-
//!     renders in place after wake and looks up the result via
//!     `store.read_where(table, MUTATION_KEY_COL, hex(key))`.
//!
//! Unification note: this used to be a dedicated `MutationStore<T>`
//! (one-cell, one-row-per-key). It is now a degenerate `FactStore<T>`
//! table — the unified state backbone of the v2 runtime.
//!
//! The Spawner is the runtime seam. `ThreadSpawner` uses
//! `std::thread::spawn` — zero runtime dependency, the no-tokio path.
//! `TokioSpawner` uses `tokio::task::spawn_blocking` — for consumers
//! that already host a runtime and want to share its pool. Either
//! choice is invisible to the Component.

use std::borrow::Cow;
use std::sync::Arc;

use super::fact_store::FactStore;
use super::next_key::NextKey;
use super::queue::QueueBackend;
use super::row::Row;

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

/// Default fact table for mutation result writeback.
pub const DEFAULT_MUTATION_TABLE: &str = "mutation_results";

/// Column name used to key mutation results by their `NextKey` (hex).
/// Colon-prefixed = internal term, not a user capture.
pub const MUTATION_KEY_COL: &str = ":mutation_key";

/// Hex-encode a `NextKey` for storage as a string column.
pub fn key_hex(key: NextKey) -> String {
    let mut s = String::with_capacity(64);
    for b in &key.0 {
        use std::fmt::Write;
        let _ = write!(&mut s, "{:02x}", b);
    }
    s
}

pub struct EffectDispatch<T: Row> {
    pub store: Arc<dyn FactStore<T>>,
    pub spawner: Arc<dyn Spawner>,
    pub queue: Arc<dyn QueueBackend<T>>,
    pub domain: Cow<'static, str>,
    pub table: Cow<'static, str>,
}

impl<T: Row> EffectDispatch<T> {
    pub fn new(
        store: Arc<dyn FactStore<T>>,
        spawner: Arc<dyn Spawner>,
        queue: Arc<dyn QueueBackend<T>>,
    ) -> Self {
        // Declare the default mutation_results table with the key
        // column. Idempotent on re-construction.
        store.declare(DEFAULT_MUTATION_TABLE, &[MUTATION_KEY_COL]);
        Self {
            store,
            spawner,
            queue,
            domain: Cow::Borrowed(DEFAULT_EFFECT_DOMAIN),
            table: Cow::Borrowed(DEFAULT_MUTATION_TABLE),
        }
    }

    pub fn with_domain(mut self, d: impl Into<Cow<'static, str>>) -> Self {
        self.domain = d.into();
        self
    }

    /// Override the writeback table. The caller must `declare` it
    /// (with `MUTATION_KEY_COL` first or as one of the columns).
    pub fn with_table(mut self, t: impl Into<Cow<'static, str>>) -> Self {
        self.table = t.into();
        self
    }

    /// Look up a previously-dispatched mutation result by its
    /// `NextKey`. Returns the most recent row whose
    /// `MUTATION_KEY_COL` equals `hex(key)`, or `None` if no result
    /// has landed yet.
    pub fn take_result(&self, key: NextKey) -> Option<Arc<T>> {
        let hex = key_hex(key);
        self.store
            .read_where(self.table.as_ref(), MUTATION_KEY_COL, &hex)
            .into_iter()
            .last()
    }

    /// Fire `mutation_fn` off-thread, stamp the result with
    /// `MUTATION_KEY_COL = hex(key)`, insert into the dispatcher's
    /// table, then promote every parked row on `(self.domain, key)`
    /// to runnable. The caller typically returns
    /// `Yield { wake: Wake::Key { domain: self.domain, key } }` from
    /// `render` so the parker re-renders after wake and reads the
    /// result via `take_result(key)` (or directly via
    /// `store.read_where(...)`).
    pub fn dispatch<F>(&self, key: NextKey, mutation_fn: F)
    where
        F: FnOnce() -> T + Send + 'static,
    {
        let store = self.store.clone();
        let queue = self.queue.clone();
        let domain = self.domain.clone();
        let table = self.table.clone();
        self.spawner.spawn(Box::new(move || {
            let mut result = mutation_fn();
            result.set(MUTATION_KEY_COL, &key_hex(key));
            store.insert(table.as_ref(), Arc::new(result));
            queue.dispatch_park(domain.as_ref(), Some(key));
        }));
    }
}
