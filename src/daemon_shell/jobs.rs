//! The job dispatcher as tokio tasks. Replaces the OS-thread `dl-job-N` worker
//! set (`jobq::Dispatcher`, which stays for jobq's own unit tests): a small set
//! of dispatcher tasks each loop claim -> run -> finish, with EVERY step on the
//! `spawn_blocking` pool because the queue and the runner (`tick_paths` /
//! `poll_tick`) are synchronous SQLite/engine work.
//!
//! The doorbell moved from jobq's `Condvar` to a `tokio::sync::Notify`: `enqueue`
//! (via `Shared::enqueue`) rings it, an idle worker awaits it. A 500ms safety
//! timeout re-checks a backed-off future `run_at` even with no new enqueue —
//! the same role jobq's `wait_for_work` timeout played. jobq's queue SEMANTICS
//! (claim/finish/coalesce/backoff SQL) are untouched.

use std::sync::Arc;
use std::time::Duration;

use super::ShellCtx;
use crate::jobq::{JobKind, JobQueue, JobRunner};

/// Spawn `n_workers` (floored at 1) dispatcher tasks claiming the given kinds
/// until the token cancels. Per-root serialization is guaranteed by jobq's `key`
/// dedup (one `tick:{root}` row at a time), not by pinning a worker to a root,
/// so concurrent `spawn_blocking` engine calls stay naturally small.
pub(crate) fn spawn(
    ctx: &ShellCtx,
    queue: Arc<JobQueue>,
    runner: Arc<dyn JobRunner>,
    n_workers: usize,
    kinds: Vec<JobKind>,
) {
    for _ in 0..n_workers.max(1) {
        let ctx = ctx.clone();
        let queue = queue.clone();
        let runner = runner.clone();
        let kinds = kinds.clone();
        ctx.clone().rt.spawn(worker_loop(ctx, queue, runner, kinds));
    }
}

async fn worker_loop(
    ctx: ShellCtx,
    queue: Arc<JobQueue>,
    runner: Arc<dyn JobRunner>,
    kinds: Vec<JobKind>,
) {
    loop {
        if ctx.cancel.is_cancelled() {
            return;
        }
        let q = queue.clone();
        let ks = kinds.clone();
        let claimed = tokio::task::spawn_blocking(move || q.claim(&ks)).await;
        match claimed {
            Ok(Ok(Some(job))) => {
                let key = job.key.clone();
                let r = runner.clone();
                // A panicking runner (a tick that unwinds) surfaces as a
                // `JoinError` here; treat it as a job failure so the queue backs
                // it off instead of the worker dying.
                let outcome = match tokio::task::spawn_blocking(move || r.run(&job)).await {
                    Ok(res) => res,
                    Err(_) => Err(anyhow::anyhow!("job runner panicked")),
                };
                let q2 = queue.clone();
                match tokio::task::spawn_blocking(move || q2.finish(&key, outcome)).await {
                    Ok(Ok(req)) => tracing::debug!("[daemon] job -> {req:?}"),
                    Ok(Err(e)) => tracing::warn!("[daemon] job finish error: {e}"),
                    Err(_) => tracing::warn!("[daemon] job finish task panicked"),
                }
            }
            Ok(Ok(None)) => {
                // Nothing ready: park on the doorbell, with a 500ms backstop that
                // re-checks a backed-off (future `run_at`) job even with no new
                // enqueue.
                tokio::select! {
                    _ = ctx.cancel.cancelled() => return,
                    _ = ctx.job_notify.notified() => {}
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("[daemon] job claim error: {e}");
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            Err(_) => {
                tracing::warn!("[daemon] job claim task panicked");
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}
