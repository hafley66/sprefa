//! Class-18 residual pins (daemon-side req_id mid-tick cancellation), split
//! from `tests.rs` per the file-size law. Two fail-pre-fix behaviors:
//!   1. a job a client request caused threads the request's id into the
//!      follow-up jobs its RUN mints (tick_full's cold staging shape) — the
//!      worker enters `reqid::scope(job.req_id)` around the runner;
//!   2. `cancel_req` during a long-RUNNING job aborts it at the next
//!      `crate::cancel::checkpoint` (the probe the worker installs) and the
//!      row parks `Killed` — not `Done`, not `Failed`-with-backoff.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio_util::sync::CancellationToken;

use super::tests::{wait_for_state, TmpDir};
use super::workers::{self, JobRunner};
use super::*;
use crate::daemon::shell::ShellCtx;

/// Sandbox worker rig over a caller-built queue (unlike `tests::rig`, the
/// runner here needs the queue handle itself, so the queue is built first).
async fn rig_with(
    dir: &TmpDir,
    build_runner: impl FnOnce(Arc<JobQueue>) -> Arc<dyn JobRunner>,
) -> (Arc<JobQueue>, ShellCtx, CancellationToken) {
    let pool = workers::open_pool(&dir.0.join("jobs.sqlite"))
        .await
        .expect("pool");
    let q = JobQueue::open_with_budget(&dir.0, None).expect("open");
    let cancel = CancellationToken::new();
    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<String>(8);
    let ctx = ShellCtx {
        rt: tokio::runtime::Handle::current(),
        cancel: cancel.clone(),
        job_notify: Arc::new(tokio::sync::Notify::new()),
        broadcast_tx,
    };
    let runner = build_runner(q.clone());
    workers::spawn_workers(&ctx, &pool, q.clone(), runner, 1);
    (q, ctx, cancel)
}

/// The `$.req_id` mark `cancel_req` matches rows by, straight off the table.
fn meta_req_id(q: &JobQueue, key: &str) -> Option<String> {
    let db = plock(&q.db);
    db.query_opt(
        "Jobs",
        "SELECT json_extract(metadata, '$.req_id') FROM Jobs WHERE idempotency_key=?1",
        &[key.into()],
        |r| Ok(r.get(0)?),
    )
    .ok()
    .flatten()
}

/// Mirrors `tick_full`'s cold staging: a Tick job's RUN mints a follow-up
/// ColdExtract job through the ordinary `JobRow` constructor (which captures
/// `reqid::current()` on the enqueuing thread).
struct ChainEnqueueRunner {
    jobq: Arc<JobQueue>,
}
impl JobRunner for ChainEnqueueRunner {
    fn run(&self, job: &JobRow) -> Result<()> {
        if job.kind == JobKind::Tick {
            self.jobq
                .enqueue(JobRow::cold_extract(&job.root, "module-rels", 0, 1))?;
        }
        Ok(())
    }
}

/// Fail-pre-fix 1: pre-fix, `run_job` never re-entered the originating
/// request's reqid scope, so the follow-up job's `req_id` was `None` and a
/// client disconnect could not reach the work its request transitively caused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_caused_job_threads_req_id_into_follow_up_enqueues() {
    let dir = TmpDir::new();
    let (q, ctx, cancel) = rig_with(&dir, |jobq| Arc::new(ChainEnqueueRunner { jobq })).await;

    // The request-scoped entry half (already landed with the apalis
    // migration): an enqueue on a thread inside `reqid::scope` stamps the row.
    {
        let _scope = crate::reqid::scope("req-42");
        q.enqueue(JobRow::tick("r1", &[PathBuf::from("/a.rs")]))
            .unwrap();
    }
    assert_eq!(meta_req_id(&q, "tick:r1").as_deref(), Some("req-42"));

    ctx.job_notify.notify_waiters();
    assert!(
        wait_for_state(&q, "tick:r1", "Done", 200).await,
        "tick job never completed"
    );

    // The follow-up cold job minted DURING the run must inherit the id — both
    // in the payload and in the `$.req_id` metadata `cancel_req` matches.
    let mut payload = None;
    for _ in 0..100 {
        if let Some((_, _, p, ..)) = q.peek("cold:r1:module-rels:0") {
            payload = Some(p);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    cancel.cancel();
    let payload = payload.expect("the run must have enqueued its follow-up cold job");
    let row: JobRow = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        row.req_id.as_deref(),
        Some("req-42"),
        "a follow-up job minted during a request-caused run must inherit the request's id"
    );
    assert_eq!(
        meta_req_id(&q, "cold:r1:module-rels:0").as_deref(),
        Some("req-42"),
        "the durable metadata mark (what cancel_req matches) must carry the id too"
    );
}

/// A runner standing in for a long tick: loops on the mid-run checkpoint until
/// the cancellation probe fires. Pre-fix, no probe was installed, so the loop
/// ran out and the job failed ordinarily (`Failed` + backoff) instead of
/// parking `Killed`.
struct CheckpointLoopRunner {
    started: Arc<AtomicBool>,
    /// Set when the loop ran out WITHOUT the checkpoint ever firing — the
    /// pre-fix shape (no probe installed; the job only dies as an ordinary
    /// failure and reaches `Killed` late, via backoff + the refetch's
    /// job-boundary check, burning a failed attempt on the way).
    exhausted: Arc<AtomicBool>,
}
impl JobRunner for CheckpointLoopRunner {
    fn run(&self, _job: &JobRow) -> Result<()> {
        self.started.store(true, Ordering::SeqCst);
        for _ in 0..240 {
            crate::cancel::checkpoint("test-loop")?;
            std::thread::sleep(Duration::from_millis(25));
        }
        self.exhausted.store(true, Ordering::SeqCst);
        anyhow::bail!("cancellation never observed — mid-run probe not installed")
    }
}

/// Fail-pre-fix 2: `cancel_req` during a RUNNING job aborts it at the next
/// checkpoint and the row lands `Killed` (the parked no-retry state), leaving
/// no retry burn.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_req_aborts_a_running_job_at_the_next_checkpoint() {
    let dir = TmpDir::new();
    let started = Arc::new(AtomicBool::new(false));
    let exhausted = Arc::new(AtomicBool::new(false));
    let (started_r, exhausted_r) = (started.clone(), exhausted.clone());
    let (q, ctx, cancel) = rig_with(&dir, move |_jobq| {
        Arc::new(CheckpointLoopRunner {
            started: started_r,
            exhausted: exhausted_r,
        })
    })
    .await;

    {
        let _scope = crate::reqid::scope("req-9");
        q.enqueue(JobRow::tick("slow", &[PathBuf::from("/a.rs")]))
            .unwrap();
    }
    ctx.job_notify.notify_waiters();

    // Wait until the runner is actually executing (the job-boundary check has
    // passed), then cancel the request mid-run.
    let mut is_started = false;
    for _ in 0..200 {
        if started.load(Ordering::SeqCst) {
            is_started = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(is_started, "the job never started running");
    let (_killed, flagged) = q.cancel_req("req-9").unwrap();
    assert_eq!(flagged, 1, "the running job is flagged, not stomped");

    let parked = wait_for_state(&q, "tick:slow", "Killed", 200).await;
    cancel.cancel();
    assert!(
        parked,
        "a cancelled running job must abort at the next checkpoint and park Killed, got {:?}",
        q.peek("tick:slow")
    );
    assert!(
        !exhausted.load(Ordering::SeqCst),
        "the abort must come from the mid-run checkpoint, not from the runner \
         failing on its own and dying later at a refetch's job boundary"
    );
}
