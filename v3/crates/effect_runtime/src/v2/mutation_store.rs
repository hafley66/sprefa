//! `MutationStore<T>` — keyed in-RAM box for routing mutation results
//! back to components.
//!
//! React-query vocabulary: a `mutationFn` runs (often off-thread, often
//! async), produces a result, and the result is delivered back to the
//! caller. Here, the caller's continuation is a parked queue row at
//! `pc+1`. The Component returns `Suspense{Key(k)}`, the mutation
//! producer writes the result under `k`, dispatches `KeyDirty(k)` on
//! the `EventBus`, and the row at `pc+1` reads the result by `k`.
//!
//! Generic over `T: Next`. The store carries one type per instance —
//! consumers spin up multiple stores for multiple result types. Phase F
//! adds a sqlite-backed variant so results survive process restart.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use super::next::Next;
use super::next_key::NextKey;

pub struct MutationStore<T: Next> {
    by_key: Mutex<HashMap<NextKey, Arc<T>>>,
}

impl<T: Next> MutationStore<T> {
    pub fn new() -> Self {
        Self { by_key: Mutex::new(HashMap::new()) }
    }

    pub fn put(&self, key: NextKey, value: Arc<T>) {
        self.by_key.lock().unwrap().insert(key, value);
    }

    /// Read without removing. Useful when the result may be consulted
    /// by multiple parked rows on the same key.
    pub fn peek(&self, key: NextKey) -> Option<Arc<T>> {
        self.by_key.lock().unwrap().get(&key).cloned()
    }

    /// Read and remove. One-shot consumption.
    pub fn take(&self, key: NextKey) -> Option<Arc<T>> {
        self.by_key.lock().unwrap().remove(&key)
    }

    pub fn len(&self) -> usize {
        self.by_key.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.lock().unwrap().is_empty()
    }
}

impl<T: Next> Default for MutationStore<T> {
    fn default() -> Self { Self::new() }
}
