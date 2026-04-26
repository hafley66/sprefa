//! Yield / Next / Unsubscribe — the bidirectional coroutine primitive.
//!
//! Vocabulary (locked 2026-04-26):
//!
//! - **Yield**: op-side suspend point. Op calls `cx.put(Yield {..})` and
//!   awaits a `Result<NextValue, Unsubscribed>`. JS-generator analogue.
//! - **Next**: outside-world resume. Outside code calls
//!   `registry.next(key, value)` to deliver a value to the awaiting op.
//!   `Subject.next(v)` from RxJS is the analogue.
//! - **Unsubscribe**: outside-world cancel. Outside code calls
//!   `registry.unsubscribe(key)` (or bulk via `unsubscribe_where`) to
//!   tear down the subscription. The op's await resolves
//!   `Err(Unsubscribed)`.
//!
//! Three runtime constructs collapse to this primitive: parked write
//! effects (await user approval), tag-subscribe (await matching write),
//! Pending captures (await upstream binding).
//!
//! ## Cancellation paths
//!
//! Every path GCs the subject entry:
//!
//! 1. External `next(key, v)`: entry removed, op resolves `Ok(v)`.
//! 2. External `unsubscribe(key)`: entry removed, op resolves `Err`.
//! 3. Bulk `unsubscribe_where(pred)`: matching entries removed, ops resolve `Err`.
//! 4. Runtime cancel token (`cancel_all` or per-put): batcher selects on
//!    `cancel.cancelled()`, removes its own entry, op resolves `Err`.
//! 5. Future drop (op-side abort, e.g. stream consumer dropped): a Drop
//!    guard inside the future calls `unsubscribe` on the registry.
//!
//! ## Lineage
//!
//! `Yield` carries an opaque `Option<Lineage>` (boxed `Any`). The
//! framework stays domain-neutral; consumers (e.g. the pipeline crate)
//! attach their own `CursorLineage` struct and use `unsubscribe_where`
//! with a downcast predicate to invalidate by upstream-cursor identity.

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use crate::{Batcher, BoxFuture, CancellationToken, EffectKind, Store};

/// Identifier for a pending subject. Allocated by
/// `SubjectRegistry::fresh_key`; opaque to consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubjectKey(u64);

impl SubjectKey {
    pub fn raw(self) -> u64 { self.0 }
}

/// Sentinel error: the subject was unsubscribed before a value arrived.
#[derive(Debug, PartialEq, Eq)]
pub struct Unsubscribed;

/// Resume payload. `Arc<dyn Any>` keeps the runtime domain-neutral.
/// Consumers downcast to their concrete type.
pub type NextValue = Arc<dyn Any + Send + Sync>;

/// Optional lineage tag attached at register time. Used by
/// `unsubscribe_where` for bulk teardown when an upstream invariant
/// changes. Consumers define their own struct (e.g. `CursorLineage`)
/// and attach it as `Arc<MyLineage>`.
pub type Lineage = Arc<dyn Any + Send + Sync>;

/// Effect kind. Op author writes `cx.put(Yield { key, lineage }).await`.
pub struct Yield {
    pub key: SubjectKey,
    pub lineage: Option<Lineage>,
}

impl EffectKind for Yield {
    type Response = Result<NextValue, Unsubscribed>;
}

struct Entry {
    sender: oneshot::Sender<Result<NextValue, Unsubscribed>>,
    lineage: Option<Lineage>,
}

/// Keyed registry of pending Yield calls. Held as a `Store` on `RtCtx`.
/// Outside code (LSP server, CLI runner, tag-write batcher) gets it via
/// `cx.store::<SubjectRegistry>()` and drives `next` / `unsubscribe`.
pub struct SubjectRegistry {
    next_id: AtomicU64,
    pending: Mutex<HashMap<SubjectKey, Entry>>,
}

impl Default for SubjectRegistry {
    fn default() -> Self { Self::new() }
}

impl Store for SubjectRegistry {}

impl SubjectRegistry {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Allocate a fresh subject key. Op authors call this before
    /// dispatching `Yield` so the same key is reachable to whichever
    /// outside path will resume them.
    pub fn fresh_key(&self) -> SubjectKey {
        SubjectKey(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Number of currently-pending subjects. Useful for tests + debug.
    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    /// Resume the op awaiting `key` with `value`. Returns true if a
    /// pending subject was found and woken; false if no such key (e.g.
    /// already resolved or unsubscribed). Concurrent `next` calls on
    /// the same key: first wins, rest return false.
    pub fn next(&self, key: SubjectKey, value: NextValue) -> bool {
        let entry = self.pending.lock().unwrap().remove(&key);
        match entry {
            Some(e) => {
                let _ = e.sender.send(Ok(value));
                true
            }
            None => false,
        }
    }

    /// Cancel `key`. Returns true if a pending subject was found.
    pub fn unsubscribe(&self, key: SubjectKey) -> bool {
        let entry = self.pending.lock().unwrap().remove(&key);
        match entry {
            Some(e) => {
                let _ = e.sender.send(Err(Unsubscribed));
                true
            }
            None => false,
        }
    }

    /// Cancel every subject whose lineage matches `pred`. Subjects with
    /// no lineage are ignored. Returns the count cancelled.
    pub fn unsubscribe_where<F>(&self, mut pred: F) -> usize
    where
        F: FnMut(&dyn Any) -> bool,
    {
        let mut guard = self.pending.lock().unwrap();
        let to_remove: Vec<SubjectKey> = guard
            .iter()
            .filter_map(|(k, e)| match e.lineage.as_ref() {
                Some(lin) if pred(lin.as_ref()) => Some(*k),
                _ => None,
            })
            .collect();
        let n = to_remove.len();
        for k in to_remove {
            if let Some(e) = guard.remove(&k) {
                let _ = e.sender.send(Err(Unsubscribed));
            }
        }
        n
    }

    /// Cancel every pending subject. Used by `RtCtx::cancel_all` paths
    /// that want to drain the runtime; not called from the framework
    /// directly (root cancel token already wakes each YieldBatcher
    /// future, which then GCs its own entry).
    pub fn unsubscribe_all(&self) -> usize {
        let mut guard = self.pending.lock().unwrap();
        let n = guard.len();
        for (_, e) in guard.drain() {
            let _ = e.sender.send(Err(Unsubscribed));
        }
        n
    }

    fn register(
        &self,
        key: SubjectKey,
        lineage: Option<Lineage>,
    ) -> oneshot::Receiver<Result<NextValue, Unsubscribed>> {
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap()
            .insert(key, Entry { sender: tx, lineage });
        rx
    }
}

/// Batcher for `Yield`. Construct once with an `Arc<SubjectRegistry>`
/// shared with whoever drives `next` / `unsubscribe`. Register on the
/// builder alongside the store:
///
/// ```ignore
/// let registry = Arc::new(SubjectRegistry::new());
/// let cx = RtCtxBuilder::new()
///     .with_store(registry.clone())
///     .register::<Yield, _>(YieldBatcher::new(registry.clone()))
///     .build();
/// ```
pub struct YieldBatcher {
    registry: Arc<SubjectRegistry>,
}

impl YieldBatcher {
    pub fn new(registry: Arc<SubjectRegistry>) -> Self {
        Self { registry }
    }
}

/// RAII guard that cleans up the registry entry on future-drop. Any
/// path that exits the `select!` (Next, Unsubscribe, cancel-token,
/// future-drop from outside) hits this Drop. Already-resolved entries
/// are no-ops because `unsubscribe` returns false.
struct YieldGuard {
    registry: Arc<SubjectRegistry>,
    key: SubjectKey,
}

impl Drop for YieldGuard {
    fn drop(&mut self) {
        self.registry.unsubscribe(self.key);
    }
}

impl Batcher<Yield> for YieldBatcher {
    fn run(
        &self,
        req: Yield,
        cancel: CancellationToken,
    ) -> BoxFuture<'static, Result<NextValue, Unsubscribed>> {
        let registry = self.registry.clone();
        let key = req.key;
        let rx = registry.register(key, req.lineage);
        let guard = YieldGuard {
            registry: registry.clone(),
            key,
        };
        Box::pin(async move {
            let _g = guard;
            tokio::select! {
                resolved = rx => match resolved {
                    Ok(result) => result,
                    Err(_) => Err(Unsubscribed),
                },
                _ = cancel.cancelled() => Err(Unsubscribed),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RtCtx, RtCtxBuilder};
    use std::time::Duration;

    fn build_cx() -> (RtCtx, Arc<SubjectRegistry>) {
        let registry = Arc::new(SubjectRegistry::new());
        let cx = RtCtxBuilder::new()
            .with_store(registry.clone())
            .register::<Yield, _>(YieldBatcher::new(registry.clone()))
            .build();
        (cx, registry)
    }

    fn val(s: &'static str) -> NextValue {
        Arc::new(s.to_string())
    }

    fn as_str(v: &NextValue) -> String {
        v.downcast_ref::<String>().unwrap().clone()
    }

    #[tokio::test]
    async fn roundtrip_next_resolves_yield() {
        let (cx, reg) = build_cx();
        let key = reg.fresh_key();
        let yield_fut =
            cx.put(Yield { key, lineage: None });

        let driver = {
            let reg = reg.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                assert!(reg.next(key, val("hello")));
            })
        };

        let result = yield_fut.await;
        let v = result.expect("Ok");
        assert_eq!(as_str(&v), "hello");
        assert_eq!(reg.pending_count(), 0);
        driver.await.unwrap();
    }

    fn assert_unsub<T>(r: Result<T, Unsubscribed>) {
        match r {
            Err(Unsubscribed) => {}
            Ok(_) => panic!("expected Unsubscribed, got Ok"),
        }
    }

    #[tokio::test]
    async fn unsubscribe_resolves_with_err() {
        let (cx, reg) = build_cx();
        let key = reg.fresh_key();
        let yield_fut = cx.put(Yield { key, lineage: None });

        let driver = {
            let reg = reg.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                assert!(reg.unsubscribe(key));
            })
        };

        assert_unsub(yield_fut.await);
        assert_eq!(reg.pending_count(), 0);
        driver.await.unwrap();
    }

    #[tokio::test]
    async fn future_drop_gcs_entry() {
        let (cx, reg) = build_cx();
        let key = reg.fresh_key();

        // Spawn a task that awaits the Yield, then abort it. Aborting
        // drops the future while the Yield is pending, which exercises
        // the YieldGuard Drop path.
        let cx_clone = cx.clone();
        let handle = tokio::spawn(async move {
            let _ = cx_clone.put(Yield { key, lineage: None }).await;
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(reg.pending_count(), 1, "registered while awaited");

        handle.abort();
        let _ = handle.await;
        tokio::task::yield_now().await;
        assert_eq!(reg.pending_count(), 0, "guard cleaned up on abort");
    }

    #[tokio::test]
    async fn runtime_cancel_resolves_err_and_gcs() {
        let (cx, reg) = build_cx();
        let key = reg.fresh_key();
        let yield_fut = cx.put(Yield { key, lineage: None });

        let driver = {
            let cx = cx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                cx.cancel_all();
            })
        };

        assert_unsub(yield_fut.await);
        // YieldGuard drop runs, registry entry gone.
        tokio::task::yield_now().await;
        assert_eq!(reg.pending_count(), 0);
        driver.await.unwrap();
    }

    #[derive(Debug)]
    struct Lin { tag: u32 }

    /// Spawn a `Yield` so the future is actually driven. Returns a
    /// JoinHandle the test can `.await` for the resolved result.
    fn spawn_yield(
        cx: &RtCtx,
        key: SubjectKey,
        lineage: Option<Lineage>,
    ) -> tokio::task::JoinHandle<Result<NextValue, Unsubscribed>> {
        let cx = cx.clone();
        tokio::spawn(async move { cx.put(Yield { key, lineage }).await })
    }

    #[tokio::test]
    async fn unsubscribe_where_predicate_matches_lineage() {
        let (cx, reg) = build_cx();
        let k1 = reg.fresh_key();
        let k2 = reg.fresh_key();
        let k3 = reg.fresh_key();

        let h1 = spawn_yield(&cx, k1, Some(Arc::new(Lin { tag: 1 })));
        let h2 = spawn_yield(&cx, k2, Some(Arc::new(Lin { tag: 2 })));
        let h3 = spawn_yield(&cx, k3, None);

        // Let all three register.
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(reg.pending_count(), 3);

        let n = reg.unsubscribe_where(|any| {
            matches!(any.downcast_ref::<Lin>(), Some(l) if l.tag == 1)
        });
        assert_eq!(n, 1);
        assert_unsub(h1.await.unwrap());

        // h2 and h3 still pending. Resolve them so the test isn't
        // dependent on cleanup ordering.
        assert!(reg.next(k2, val("two")));
        assert!(reg.unsubscribe(k3));
        assert_eq!(as_str(&h2.await.unwrap().unwrap()), "two");
        assert_unsub(h3.await.unwrap());
        assert_eq!(reg.pending_count(), 0);
    }

    #[tokio::test]
    async fn concurrent_next_first_wins() {
        let (cx, reg) = build_cx();
        let key = reg.fresh_key();
        let h = spawn_yield(&cx, key, None);

        // Let it register.
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(reg.next(key, val("first")));
        assert!(!reg.next(key, val("second")), "second next is no-op");
        assert!(!reg.unsubscribe(key), "unsubscribe after next is no-op");

        let v = h.await.unwrap().unwrap();
        assert_eq!(as_str(&v), "first");
    }

    #[tokio::test]
    async fn registry_reachable_via_store_typemap() {
        let (cx, reg) = build_cx();
        let from_store: Arc<SubjectRegistry> =
            cx.store::<SubjectRegistry>().expect("registered");
        // Same Arc — fresh_key sequence is shared.
        let k = from_store.fresh_key();
        assert_eq!(k.raw(), reg.fresh_key().raw() - 1);
    }

    #[tokio::test]
    async fn unsubscribe_all_drains_registry() {
        let (cx, reg) = build_cx();
        let h1 = spawn_yield(&cx, reg.fresh_key(), None);
        let h2 = spawn_yield(&cx, reg.fresh_key(), None);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(reg.pending_count(), 2);
        let n = reg.unsubscribe_all();
        assert_eq!(n, 2);
        assert_unsub(h1.await.unwrap());
        assert_unsub(h2.await.unwrap());
    }
}
