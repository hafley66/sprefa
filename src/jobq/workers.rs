//! The apalis worker set (plan `2026-07-18-infra-library-adoption.md` 5.3
//! step 3). Replaces the bespoke claim/park dispatcher loop that lived in
//! `daemon/shell/jobs.rs`: fetch/lock/ack, worker registration+heartbeat, and
//! live orphan re-enqueue are apalis's; this module only builds the two
//! workers and adapts each fetched task back onto the sync [`JobRunner`] seam
//! (`spawn_blocking`, engine untouched — the sync-tick-engine law holds).
//!
//! Shape:
//!   - one sqlx pool on `<home>/jobs.sqlite` (`open_pool`, runs the apalis
//!     migrations — the bought schema);
//!   - worker `dl-engine-{pid}` serves the `dl-engine` queue (Tick +
//!     SinkDrain) with `.concurrency(n_workers)` — the old n-claim-loop
//!     budget, now a tower `ConcurrencyLimitLayer`;
//!   - worker `dl-cold-{pid}` serves `dl-cold` with `.concurrency(1)` — the
//!     single-flight cold worker (root ORDER comes from the hold/promote
//!     admission in `jobq::enqueue`/`reconcile`);
//!   - the poll strategy is doorbell + 500ms interval: `Shared::enqueue`
//!     rings the shell `Notify`, so a fresh job is fetched immediately and an
//!     idle daemon polls at the same 500ms backstop the old dispatcher's
//!     `wait_for_work` had;
//!   - a reconciler task drives `JobQueue::reconcile` on the same doorbell +
//!     500ms cadence (dirty reruns, cold promotion, budget deferral,
//!     retention).
//!
//! The perfetto `job` span is entered INSIDE the `spawn_blocking` closure, so
//! the tick/phase spans nest under it via tracing's thread-local span stack —
//! byte-for-byte the old dispatcher's DL_TRACE_CHROME behavior.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use apalis::prelude::*;
use apalis_sqlite::{Config, PoolOptions, SqliteConnectOptions, SqlitePool, SqliteStorage};
use tokio::sync::Notify;

use super::{JobKind, JobQueue, JobRow, COLD_QUEUE, ENGINE_QUEUE, MAX_ATTEMPTS};
use crate::daemon::shell::ShellCtx;

/// Run one claimed job. The daemon's impl looks the root up and calls
/// `tick_paths` / `poll_tick` / `run_cold_node`; tests inject recording or
/// failing runners.
pub(crate) trait JobRunner: Send + Sync {
    fn run(&self, job: &JobRow) -> Result<()>;
}

/// Everything the task handler needs, injected via apalis `Data`.
#[derive(Clone)]
struct HandlerCtx {
    jobq: Arc<JobQueue>,
    runner: Arc<dyn JobRunner>,
}

/// Open (creating) the sqlx pool on `<home>/jobs.sqlite` and run the apalis
/// migrations. Must complete before `JobQueue::open` (which refuses to run
/// without the `Jobs` table) and before any worker spawns.
pub(crate) async fn open_pool(db_path: &std::path::Path) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = PoolOptions::new().max_connections(2).connect_with(opts).await?;
    SqliteStorage::setup(&pool).await?;
    Ok(pool)
}

/// The doorbell-or-backstop poll strategy: fires on `notify_waiters` (a fresh
/// enqueue / a reconcile that made work) and every 500ms regardless (the
/// backed-off `run_at` safety net the old `wait_for_work` timeout provided).
fn doorbell_strategy(notify: Arc<Notify>) -> MultiStrategy {
    let doorbell = Box::pin(futures::stream::unfold(notify, |notify| async move {
        notify.notified().await;
        Some(((), notify))
    }));
    StrategyBuilder::new()
        .apply(StreamStrategy::new(doorbell))
        .apply(IntervalStrategy::new(Duration::from_millis(500)))
        .build()
}

/// Spawn both apalis workers + the reconciler onto the shell runtime. Workers
/// stop when the shell cancellation token fires (graceful: in-flight jobs
/// finish; an unfinished one is recovered by the boot reset / orphan sweep).
pub(crate) fn spawn_workers(
    ctx: &ShellCtx,
    pool: &SqlitePool,
    jobq: Arc<JobQueue>,
    runner: Arc<dyn JobRunner>,
    n_engine_workers: usize,
) {
    let pid = std::process::id();
    let handler_ctx = HandlerCtx { jobq: jobq.clone(), runner };
    for (queue, name, concurrency) in [
        (ENGINE_QUEUE, format!("dl-engine-{pid}"), n_engine_workers.max(1)),
        (COLD_QUEUE, format!("dl-cold-{pid}"), 1usize),
    ] {
        let config = Config::new(queue)
            .with_poll_interval(doorbell_strategy(ctx.job_notify.clone()))
            .set_buffer_size(1);
        let storage = SqliteStorage::<JobRow, (), ()>::new_with_config(pool, &config);
        let worker = WorkerBuilder::new(&name)
            .backend(storage)
            .data(handler_ctx.clone())
            .concurrency(concurrency)
            .build(run_job);
        let cancel = ctx.cancel.clone();
        ctx.rt.spawn(async move {
            let stop = async move {
                cancel.cancelled().await;
                Ok::<(), WorkerError>(())
            };
            if let Err(e) = worker.run_until(stop).await {
                tracing::warn!("[daemon] worker {name} exited: {e}");
            }
        });
    }
    spawn_reconciler(ctx, jobq);
}

/// The reconciler task: doorbell + 500ms cadence, `spawn_blocking` because
/// `reconcile` is sync SQLite. Rings the doorbell itself when a pass made new
/// work claimable (a dirty rerun or a cold promotion) — BEFORE re-parking, so
/// it never wakes itself in a loop.
fn spawn_reconciler(ctx: &ShellCtx, jobq: Arc<JobQueue>) {
    let cancel = ctx.cancel.clone();
    let notify = ctx.job_notify.clone();
    ctx.rt.spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
            let q = jobq.clone();
            match tokio::task::spawn_blocking(move || q.reconcile()).await {
                Ok(Ok(report)) => {
                    if report.made_work() {
                        notify.notify_waiters();
                    }
                }
                Ok(Err(e)) => tracing::warn!("[daemon] job reconcile: {e}"),
                Err(_) => tracing::warn!("[daemon] job reconcile task panicked"),
            }
        }
    });
}

/// The one task handler both workers run. Sync engine work stays on the
/// blocking pool; a panicking runner surfaces as a `JoinError` and is
/// converted to a job failure (the worker survives — the old
/// `catch_unwind` behavior, now provided by `spawn_blocking`'s panic
/// containment).
async fn run_job(
    job: JobRow,
    task_id: TaskId<ulid::Ulid>,
    attempt: Attempt,
    ctx: Data<HandlerCtx>,
) -> Result<(), BoxDynError> {
    // req_id cancellation, job-boundary check: a queued job whose request was
    // cancelled aborts here without burning retries (`AbortError` -> Killed).
    if ctx.jobq.take_cancel(&job.key) {
        tracing::info!(key = %job.key, "[daemon] job aborted — request cancelled");
        return Err(Box::new(AbortError::new(anyhow::anyhow!(
            "cancelled: client request gone"
        ))));
    }
    let kind = job.kind;
    let key = job.key.clone();
    let req_id = job.req_id.clone().unwrap_or_default();
    tracing::info!(kind = ?kind, key = %key, root = %job.root, req_id, "[daemon] job claimed");
    let t = std::time::Instant::now();
    let runner = ctx.runner.clone();
    let job_for_run = job.clone();
    let outcome = match tokio::task::spawn_blocking(move || {
        // Governor gate between jobs (nothing-seizes-the-machine law): a
        // worker never starts new work while the process is over its CPU
        // ceiling. On the blocking pool because it may sleep.
        crate::budget::throttle_point();
        // The "job" span for the DL_TRACE_CHROME timeline. Entered INSIDE the
        // spawn_blocking closure: the runner executes the whole tick
        // synchronously on this thread, so the tick/phase spans nest under it.
        let _job_span = tracing::info_span!(
            "job",
            kind = ?job_for_run.kind,
            key = %job_for_run.key,
            root = %job_for_run.root,
            req_id = %job_for_run.req_id.clone().unwrap_or_default(),
        )
        .entered();
        runner.run(&job_for_run)
    })
    .await
    {
        Ok(res) => res,
        Err(_) => Err(anyhow::anyhow!("job runner panicked")),
    };
    let ms = t.elapsed().as_millis() as u64;
    match outcome {
        Ok(()) => {
            if kind == JobKind::ColdExtract {
                if let Some(budget) = ctx.jobq.write_budget() {
                    budget.record_job();
                }
            }
            tracing::info!(kind = ?kind, req_id, ms, "[daemon] job done");
            Ok(())
        }
        Err(e) => {
            // apalis acks the row `Failed` (or `Killed` at the attempt cap)
            // AFTER this returns; the ack never touches `run_at`, so the
            // jittered backoff stamped here survives and defers the re-fetch.
            let attempts = attempt.current() as i64;
            if attempts >= MAX_ATTEMPTS {
                tracing::warn!(key, attempts, max = MAX_ATTEMPTS, ms, error = %e,
                    "[jobq] job PARKED killed — attempt cap hit, no more retries");
            } else {
                let delay = ctx
                    .jobq
                    .note_failure_backoff(&task_id.to_string(), &key, attempts)
                    .unwrap_or(0);
                tracing::warn!(key, attempts, max = MAX_ATTEMPTS, retry_in_secs = delay, ms,
                    error = %e, "[jobq] job failed — backoff repend");
            }
            Err(e.into())
        }
    }
}
