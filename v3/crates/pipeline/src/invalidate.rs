//! Phase F (sprefa-3n6): cache invalidation channel + driver loop.
//!
//! There is no Watcher trait here. The runtime exposes one shape — a
//! `tokio::sync::broadcast::Sender<Change>` registered as a [`Store`]
//! on the [`RtCtx`]. Producers (notify-glue, polling, manual test
//! pushes) all just `send(change)` into the channel. The consumer is
//! a single tokio task ([`spawn_invalidator`]) that drains the channel
//! and applies each change to the [`OpCache`].
//!
//! Composition shape (rxjs framing): `ChangeSubject` is the side-band
//! `Subject<Change>` that flows alongside the cursor `Observable`.
//! The cache is a `memoize()` operator listening on the Subject for
//! eviction. Pipeline re-subscription is the driver's job; cached
//! cells that survived stay hot, the rest re-execute.
//!
//! Buffer-no-L2 invariant (`feedback_buffer_no_l2`): the LSP buffer
//! overlay never participates in this channel; buffer edits invalidate
//! by replacement on the next pipeline subscribe, not by Change events.

use std::path::PathBuf;
use std::sync::Arc;

use effect_runtime::{RtCtx, Store};
use tokio::sync::broadcast;

use crate::cache_key::{LeafCoord, OpCache};

/// Coalesced delta from a watcher producer. One per quiesce window.
#[derive(Clone, Debug)]
pub enum Change {
    /// One file's bytes changed under `(repo, rev)`. Eviction targets
    /// every cache entry whose batch carried a cursor at this leaf.
    FileContent {
        repo: Arc<str>,
        rev: Arc<str>,
        path: PathBuf,
    },
    /// A batch of file changes coalesced from one quiesce window. Same
    /// semantics as N `FileContent` events but one eviction pass.
    FileBatch {
        repo: Arc<str>,
        rev: Arc<str>,
        paths: Vec<PathBuf>,
    },
    /// The worktree (or branch) `(repo, rev)` moved. Every cached entry
    /// tagged with this rev becomes stale because cursor.rev flowed
    /// into batch_fingerprint.
    RevHead { repo: Arc<str>, rev: Arc<str> },
}

/// Side-band `Subject<Change>`. Cloning the wrapped `Sender` is cheap;
/// producers hold one and call `.send(...)`. Consumers `subscribe()`
/// for a `Receiver<Change>`.
#[derive(Clone)]
pub struct ChangeSubject {
    tx: broadcast::Sender<Change>,
}

impl std::fmt::Debug for ChangeSubject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChangeSubject")
            .field("subscribers", &self.tx.receiver_count())
            .finish()
    }
}

impl ChangeSubject {
    /// Default capacity sized for bursty FS event windows. Lagged
    /// receivers drop the oldest events; for cache invalidation that is
    /// a correctness regression (a missed event leaves stale entries),
    /// so the capacity is intentionally generous.
    pub const DEFAULT_CAPACITY: usize = 4096;

    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    pub fn with_capacity(cap: usize) -> Self {
        let (tx, _rx) = broadcast::channel(cap);
        Self { tx }
    }

    pub fn sender(&self) -> broadcast::Sender<Change> { self.tx.clone() }
    pub fn subscribe(&self) -> broadcast::Receiver<Change> { self.tx.subscribe() }

    /// Test/manual hook: push a change synthetically. Returns the count
    /// of receivers that observed it (0 when no driver is attached).
    pub fn publish(&self, ev: Change) -> usize {
        self.tx.send(ev).unwrap_or(0)
    }
}

impl Default for ChangeSubject {
    fn default() -> Self { Self::new() }
}

impl Store for ChangeSubject {}

/// Apply a single `Change` to the cache. Pure function; the driver
/// loop wraps this. Returns the L1 eviction count for telemetry.
pub fn apply_change(cache: &OpCache, change: &Change) -> usize {
    match change {
        Change::FileContent { repo, rev, path } => {
            let coord: LeafCoord = (repo.clone(), rev.clone(), path.clone());
            cache.evict_paths(&[coord])
        }
        Change::FileBatch { repo, rev, paths } => {
            let coords: Vec<LeafCoord> = paths
                .iter()
                .map(|p| (repo.clone(), rev.clone(), p.clone()))
                .collect();
            cache.evict_paths(&coords)
        }
        Change::RevHead { repo, rev } => cache.evict_rev(repo, rev),
    }
}

/// Spawn the long-running drain task. Reads from the `Subject` and
/// applies each event to the cache. Exits when the subject is dropped
/// (no more producers) or the receiver lags catastrophically (logged).
///
/// The driver does NOT re-run pipelines. Re-execution is the caller's
/// concern (LSP republish on quiesce, CLI re-invoke). Cache eviction
/// alone is enough to ensure the next pipeline subscription recomputes
/// the affected cells.
pub fn spawn_invalidator(
    cache: Arc<OpCache>,
    subject: &ChangeSubject,
) -> tokio::task::JoinHandle<()> {
    let mut rx = subject.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(change) => {
                    let evicted = apply_change(&cache, &change);
                    tracing::debug!(
                        target: "sprefa::invalidate",
                        ?change,
                        evicted,
                        "cache.invalidate"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    // Missed events leave stale cache entries. Best
                    // remedy short of full flush: log loudly so the
                    // operator notices. A future hardening step would
                    // either bump capacity or fall back to a wholesale
                    // flush on lag.
                    tracing::warn!(
                        target: "sprefa::invalidate",
                        missed = n,
                        "subject.lagged"
                    );
                }
            }
        }
    })
}

/// Convenience: bind a fresh `ChangeSubject` to the runtime store.
pub fn install_subject(ctx: &RtCtx) -> ChangeSubject {
    let s = ChangeSubject::new();
    ctx.bind_store(Arc::new(s.clone()));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_cursor::Cursor;
    use std::path::Path;
    use std::sync::Arc;

    fn cursor_at(repo: &str, rev: &str, path: &str) -> Cursor {
        let mut c = Cursor::default();
        c.repo = Arc::from(repo);
        c.rev = Arc::from(rev);
        c.fs = Some(Arc::from(Path::new(path)));
        c
    }

    #[test]
    fn evict_paths_drops_only_matching_entries() {
        let cache = OpCache::new(true);
        let k_a = [1u8; 32];
        let k_b = [2u8; 32];
        cache.insert(k_a, Arc::from(vec![cursor_at("r", "wt", "a.rs")]));
        cache.insert(k_b, Arc::from(vec![cursor_at("r", "wt", "b.rs")]));
        assert_eq!(cache.len(), 2);

        let change = Change::FileContent {
            repo: Arc::from("r"),
            rev: Arc::from("wt"),
            path: PathBuf::from("a.rs"),
        };
        let n = apply_change(&cache, &change);
        assert_eq!(n, 1);
        assert!(cache.get(&k_a).is_none(), "a.rs entry evicted");
        assert!(cache.get(&k_b).is_some(), "b.rs entry survives");
    }

    #[test]
    fn evict_rev_drops_every_entry_under_rev() {
        let cache = OpCache::new(true);
        let k_a = [1u8; 32];
        let k_b = [2u8; 32];
        let k_c = [3u8; 32];
        cache.insert(k_a, Arc::from(vec![cursor_at("r", "wt", "a.rs")]));
        cache.insert(k_b, Arc::from(vec![cursor_at("r", "wt", "b.rs")]));
        cache.insert(k_c, Arc::from(vec![cursor_at("r", "HEAD", "c.rs")]));
        assert_eq!(cache.len(), 3);

        let n = apply_change(
            &cache,
            &Change::RevHead { repo: Arc::from("r"), rev: Arc::from("wt") },
        );
        assert_eq!(n, 2);
        assert!(cache.get(&k_a).is_none());
        assert!(cache.get(&k_b).is_none());
        assert!(cache.get(&k_c).is_some(), "HEAD entry survives wt rev hop");
    }

    #[test]
    fn batch_change_one_pass() {
        let cache = OpCache::new(true);
        for (i, path) in ["a.rs", "b.rs", "c.rs"].iter().enumerate() {
            cache.insert(
                [(i + 1) as u8; 32],
                Arc::from(vec![cursor_at("r", "wt", path)]),
            );
        }
        let n = apply_change(
            &cache,
            &Change::FileBatch {
                repo: Arc::from("r"),
                rev: Arc::from("wt"),
                paths: vec![PathBuf::from("a.rs"), PathBuf::from("c.rs")],
            },
        );
        assert_eq!(n, 2);
        assert_eq!(cache.len(), 1);
    }

    #[tokio::test]
    async fn driver_drains_published_changes() {
        let cache = Arc::new(OpCache::new(true));
        cache.insert([7u8; 32], Arc::from(vec![cursor_at("r", "wt", "x.rs")]));
        let subject = ChangeSubject::new();
        let handle = spawn_invalidator(cache.clone(), &subject);

        subject.publish(Change::FileContent {
            repo: Arc::from("r"),
            rev: Arc::from("wt"),
            path: PathBuf::from("x.rs"),
        });

        // Yield repeatedly so the driver task observes the broadcast
        // before the assertion. Bounded so a regression fails fast.
        for _ in 0..50 {
            if cache.is_empty() { break; }
            tokio::task::yield_now().await;
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        assert!(cache.is_empty(), "driver should have evicted x.rs entry");

        drop(subject);
        let _ = handle.await;
    }
}
