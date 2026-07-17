//! Job dispatcher: the worker set that claims -> runs -> finishes queue jobs,
//! plus the `JobRunner` seam the daemon implements. Split out of `jobq/mod.rs`
//! to keep each file under the size rail.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;

use super::{JobKind, JobQueue, JobRow};

/// Run one claimed job. The daemon's impl looks the root up and calls
/// `tick_paths` / `poll_tick`; tests inject a recording/failing runner.
pub(crate) trait JobRunner: Send + Sync {
    fn run(&self, job: &JobRow) -> Result<()>;
}

/// The OS-thread worker set. The live daemon no longer spawns this — its
/// dispatcher is now `crate::daemon_shell::jobs` (tokio tasks whose job-RUN
/// bodies are `spawn_blocking`) — so this stays purely as jobq's own unit-test
/// driver (hence `#[allow(dead_code)]` outside `cfg(test)`). The queue SEMANTICS
/// it exercises (`claim`/`finish`/coalesce/backoff) are shared by both.
///
/// Each worker independently claims -> runs -> finishes; per-root serialization
/// is guaranteed by the `key` dedup (one `tick:{root}` row at a time), not by
/// pinning a worker to a root.
#[allow(dead_code)]
pub(crate) struct Dispatcher {
    /// Owns the worker `JoinHandle`s so they outlive `spawn`; the live daemon
    /// never joins them (it exits on the shutdown flag), only tests do.
    #[allow(dead_code)]
    workers: Vec<JoinHandle<()>>,
}

impl Dispatcher {
    /// Spawn `n_workers` (floored at 1) claiming the given kinds until
    /// `shutdown` flips.
    #[allow(dead_code)]
    pub(crate) fn spawn(
        queue: Arc<JobQueue>,
        runner: Arc<dyn JobRunner>,
        shutdown: Arc<AtomicBool>,
        n_workers: usize,
        kinds: Vec<JobKind>,
    ) -> Dispatcher {
        let mut workers = Vec::new();
        for i in 0..n_workers.max(1) {
            let q = queue.clone();
            let r = runner.clone();
            let sd = shutdown.clone();
            let ks = kinds.clone();
            match std::thread::Builder::new()
                .name(format!("dl-job-{i}"))
                .spawn(move || worker_loop(q, r, sd, &ks))
            {
                Ok(h) => workers.push(h),
                Err(e) => tracing::error!("[daemon] job worker {i} spawn failed: {e}"),
            }
        }
        Dispatcher { workers }
    }

    /// Wake every worker and join them (tests; the live daemon just exits).
    #[cfg(test)]
    pub(crate) fn join(self, queue: &JobQueue) {
        queue.wake();
        for h in self.workers {
            let _ = h.join();
        }
    }
}

#[allow(dead_code)]
fn worker_loop(
    queue: Arc<JobQueue>,
    runner: Arc<dyn JobRunner>,
    shutdown: Arc<AtomicBool>,
    kinds: &[JobKind],
) {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let mut last_gen = 0u64;
    loop {
        if shutdown.load(Ordering::Relaxed) {
            return;
        }
        match queue.claim(kinds) {
            Ok(Some(job)) => {
                let key = job.key.clone();
                // A panicking runner (a tick that unwinds) must not kill the
                // worker permanently; treat it as a job failure and back off.
                let outcome = catch_unwind(AssertUnwindSafe(|| runner.run(&job)))
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("job runner panicked")));
                match queue.finish(&key, outcome) {
                    Ok(req) => tracing::debug!("[daemon] job {key} -> {req:?}"),
                    Err(e) => tracing::warn!("[daemon] job {key} finish error: {e}"),
                }
            }
            Ok(None) => queue.wait_for_work(&mut last_gen, Duration::from_millis(500)),
            Err(e) => {
                tracing::warn!("[daemon] job claim error: {e}");
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}
