//! jobq behavior pins, ported from the bespoke-queue suite onto the
//! apalis-sqlite store + admission layer. Two tiers:
//!   - sync tests drive `enqueue`/`reconcile`/`cancel_req` and inspect the
//!     apalis `Jobs` rows directly (the state transitions apalis itself
//!     performs — fetch/lock/ack — are simulated with targeted pokes, the
//!     same test-scenario shape the old suite used);
//!   - `#[tokio::test]` tests spawn the REAL apalis workers
//!     (`workers::spawn_workers`) against a sandbox home and prove the
//!     end-to-end behaviors: coalesce-to-one-run, panic survival,
//!     backoff-then-park.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use tokio_util::sync::CancellationToken;

use super::workers::JobRunner;
use super::*;
use crate::daemon::shell::ShellCtx;

/// A unique temp dir removed on drop (no `tempfile` dev-dep in this crate).
/// `pub(crate)` so the sibling `tests_cancel` module reuses it.
pub(crate) struct TmpDir(pub(crate) PathBuf);
impl TmpDir {
    pub(crate) fn new() -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("dl_jobq_{}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TmpDir(dir)
    }
}
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn queue() -> (Arc<JobQueue>, TmpDir) {
    let dir = TmpDir::new();
    let q = test_open(&dir.0);
    (q, dir)
}

/// Register a fake worker row so pokes can set `lock_by` (the `Jobs` table
/// has a foreign key onto `Workers(id)`).
fn register_test_worker(q: &JobQueue, id: &str) {
    q.poke(
        "INSERT OR IGNORE INTO Workers(id, worker_type, storage_name, last_seen) \
         VALUES(?1, 'test', 'SqliteStorage', strftime('%s','now'))",
        &[id.into()],
    )
    .unwrap();
}

/// Simulate an apalis fetch on the cold queue: the claimable row in exactly
/// `fetch_next.sql`'s order (`priority DESC, run_at ASC, id ASC` over ready
/// Pending / retryable Failed rows). Returns `(key, root)`.
fn claimable_cold(q: &JobQueue) -> Option<(String, String)> {
    let db = plock(&q.db);
    db.query_opt(
        "Jobs",
        "SELECT idempotency_key, json_extract(metadata, '$.root') FROM Jobs \
         WHERE job_type='dl-cold' \
           AND ((status='Pending' AND lock_by IS NULL) \
                OR (status='Failed' AND attempts < max_attempts)) \
           AND run_at <= strftime('%s', 'now') \
         ORDER BY priority DESC, run_at ASC, id ASC LIMIT 1",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .ok()
    .flatten()
}

// ---- coalescing enqueue over the apalis Jobs table ----

#[test]
fn enqueue_coalesces_pending_union_arg_and_max_priority() {
    let (q, _d) = queue();
    let mut a = JobRow::tick("r1", &[PathBuf::from("/a")]);
    a.priority = 1;
    q.enqueue(a).unwrap();
    let mut b = JobRow::tick("r1", &[PathBuf::from("/b")]);
    b.priority = 5;
    q.enqueue(b).unwrap();

    let (state, dirty, payload, priority, attempts, _run_at) = q.peek("tick:r1").unwrap();
    assert_eq!(state, "Pending");
    assert_eq!(dirty, 0, "coalescing a pending row never sets dirty");
    assert_eq!(priority, 5, "priority is the max across coalesced requests");
    assert_eq!(attempts, 0);
    let row: JobRow = serde_json::from_str(&payload).unwrap();
    assert_eq!(row.paths(), vec![PathBuf::from("/a"), PathBuf::from("/b")],
        "the pending payload unions both changed paths");

    // Exactly one row exists for the key.
    let all = q.list().unwrap();
    assert_eq!(all.iter().filter(|r| r.key == "tick:r1").count(), 1);
}

#[test]
fn enqueue_while_running_sets_dirty_and_reconcile_reruns_with_the_union() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/a")])).unwrap();
    // Simulate the apalis fetch+lock (Pending -> Running under a worker).
    register_test_worker(&q, "w1");
    q.poke(
        "UPDATE Jobs SET status='Running', lock_by='w1', \
         lock_at=strftime('%s','now') WHERE idempotency_key='tick:r1'",
        &[],
    )
    .unwrap();

    // A re-request while running must NOT reopen the row; it sets dirty and
    // unions the new path for the post-ack rerun.
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/b")])).unwrap();
    let (state, dirty, payload, ..) = q.peek("tick:r1").unwrap();
    assert_eq!(state, "Running", "a mid-run re-request leaves the row running");
    assert_eq!(dirty, 1, "a re-request while running arms the rerun");
    let row: JobRow = serde_json::from_str(&payload).unwrap();
    assert_eq!(row.paths(), vec![PathBuf::from("/a"), PathBuf::from("/b")]);

    // A reconcile pass while still running does nothing.
    let rep = q.reconcile().unwrap();
    assert_eq!(rep.dirty_reruns, 0, "no rerun while the row is still running");

    // The worker acks Done; the next reconcile reopens the dirty row with the
    // unioned payload (the coalesce promise).
    q.poke(
        "UPDATE Jobs SET status='Done', done_at=strftime('%s','now') \
         WHERE idempotency_key='tick:r1'",
        &[],
    )
    .unwrap();
    let rep = q.reconcile().unwrap();
    assert_eq!(rep.dirty_reruns, 1, "the ack'd dirty row reopens");
    let (state, dirty, payload, _prio, attempts, _run_at) = q.peek("tick:r1").unwrap();
    assert_eq!(state, "Pending");
    assert_eq!(dirty, 0);
    assert_eq!(attempts, 0);
    let row: JobRow = serde_json::from_str(&payload).unwrap();
    assert_eq!(row.paths(), vec![PathBuf::from("/a"), PathBuf::from("/b")],
        "the rerun carries the unioned paths");
}

#[test]
fn enqueue_reopens_a_terminal_row() {
    let (q, _d) = queue();
    q.enqueue(JobRow::sink_drain("r1")).unwrap();
    q.poke(
        "UPDATE Jobs SET status='Killed', attempts=5, done_at=strftime('%s','now'), \
         last_result='{\"Err\":\"boom\"}' WHERE idempotency_key='sink:r1'",
        &[],
    )
    .unwrap();
    q.enqueue(JobRow::sink_drain("r1")).unwrap();
    let (state, _dirty, _payload, _prio, attempts, _run_at) = q.peek("sink:r1").unwrap();
    assert_eq!(state, "Pending", "a re-request reopens a parked row");
    assert_eq!(attempts, 0, "reopen resets the attempt count");
}

// ---- backoff (pure fns + the failure stamp) ----

#[test]
fn backoff_secs_doubles_then_caps() {
    assert_eq!(backoff_secs(1), 2);
    assert_eq!(backoff_secs(2), 4);
    assert_eq!(backoff_secs(3), 8);
    assert_eq!(backoff_secs(20), 300, "caps at 300s, never grows unbounded");
    assert_eq!(backoff_secs(100), 300, "a huge attempt count must not overflow/panic");
}

#[test]
fn jittered_backoff_stays_in_band_and_never_panics() {
    for attempts in 1..=64i64 {
        let base = backoff_secs(attempts);
        let spread = (base / 2).max(1);
        for seed in [0u64, 1, 7, 12345, u64::MAX] {
            let j = jittered_backoff(attempts, seed);
            assert!(j >= base, "jitter dropped below base ({j} < {base})");
            assert!(j <= base + spread, "jitter exceeded base+spread ({j} > {base}+{spread})");
        }
    }
    // Distinct seeds spread across the band (not all pinned to base).
    let spread_vals: std::collections::HashSet<i64> =
        (0..50u64).map(|s| jittered_backoff(20, s)).collect();
    assert!(spread_vals.len() > 1, "jitter never varies — thundering herd not broken");
}

#[test]
fn note_failure_backoff_stamps_a_future_run_at() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/a")])).unwrap();
    let task_id: String = {
        let db = plock(&q.db);
        db.query_one("Jobs", "SELECT id FROM Jobs WHERE idempotency_key='tick:r1'", &[], |r| {
            Ok(r.get(0)?)
        })
        .unwrap()
    };
    let now = now_secs();
    let delay = q.note_failure_backoff(&task_id, "tick:r1", 1).unwrap();
    assert!(delay >= 2, "first failure backs off at least backoff_secs(1)");
    let (.., run_at) = q.peek("tick:r1").unwrap();
    assert!(run_at > now, "a failed job backs off to a future run_at ({run_at} <= {now})");
}

// ---- boot crash recovery ----

#[test]
fn reset_orphaned_on_boot_re_pends_in_flight_rows() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/a")])).unwrap();
    q.enqueue(JobRow::tick("r2", &[PathBuf::from("/b")])).unwrap();
    register_test_worker(&q, "w");
    q.poke("UPDATE Jobs SET status='Running', lock_by='w' WHERE idempotency_key='tick:r1'", &[])
        .unwrap();
    q.poke("UPDATE Jobs SET status='Queued', lock_by='w' WHERE idempotency_key='tick:r2'", &[])
        .unwrap();

    let n = q.reset_orphaned_on_boot().unwrap();
    assert_eq!(n, 2, "both in-flight rows reset on boot");
    assert_eq!(q.peek("tick:r1").unwrap().0, "Pending");
    assert_eq!(q.peek("tick:r2").unwrap().0, "Pending");
}

// ---- req_id cancellation (class-18 residual) ----

#[test]
fn cancel_req_kills_pending_and_flags_running() {
    let (q, _d) = queue();
    {
        let _scope = crate::reqid::scope("req-77");
        q.enqueue(JobRow::tick("pending-root", &[PathBuf::from("/a")])).unwrap();
        q.enqueue(JobRow::tick("running-root", &[PathBuf::from("/b")])).unwrap();
    }
    q.enqueue(JobRow::tick("other-root", &[PathBuf::from("/c")])).unwrap();
    register_test_worker(&q, "w");
    q.poke(
        "UPDATE Jobs SET status='Running', lock_by='w' \
         WHERE idempotency_key='tick:running-root'",
        &[],
    )
    .unwrap();

    let (killed, flagged) = q.cancel_req("req-77").unwrap();
    assert_eq!(killed, 1, "the request's pending job is killed");
    assert_eq!(flagged, 1, "the request's running job is flagged");
    assert_eq!(q.peek("tick:pending-root").unwrap().0, "Killed");
    assert_eq!(q.peek("tick:running-root").unwrap().0, "Running",
        "a running job is not stomped, only flagged");
    assert_eq!(q.peek("tick:other-root").unwrap().0, "Pending",
        "another request's job is untouched");

    // The worker consumes the flag exactly once at its next job boundary.
    assert!(q.take_cancel("tick:running-root"));
    assert!(!q.take_cancel("tick:running-root"), "the flag is consumed");

    // A fresh enqueue for the same key clears any stale flag.
    let (_, flagged2) = q.cancel_req("req-77").unwrap();
    assert_eq!(flagged2, 1);
    q.enqueue(JobRow::tick("running-root", &[PathBuf::from("/d")])).unwrap();
    assert!(!q.take_cancel("tick:running-root"), "re-enqueue clears the cancel flag");
}

// ---- root-serialized ColdExtract (2026-07-18 incident pin) ----

/// Seed cold nodes for two roots exactly as `Engine::seed_cold_nodes` would
/// (a `scip-index` node above every OTHER family, priority scale restarting
/// at each root's own family count), drain every job through the simulated
/// fetch order, and assert root-a fully finishes before root-b's first claim.
/// Under a GLOBAL `priority DESC` order root-b's `scip-index` (priority 4)
/// would jump root-a's remaining 3/2/1 nodes — the hold/promote admission is
/// what prevents it.
#[test]
fn cold_extract_finishes_one_root_before_starting_another() {
    let (q, _d) = queue();
    q.enqueue(JobRow::cold_extract("root-a", "scip-index", 0, 4)).unwrap();
    q.enqueue(JobRow::cold_extract("root-a", "module-rels", 0, 3)).unwrap();
    q.enqueue(JobRow::cold_extract("root-a", "type-rels", 0, 2)).unwrap();
    q.enqueue(JobRow::cold_extract("root-a", "call-rels", 0, 1)).unwrap();
    q.enqueue(JobRow::cold_extract("root-b", "scip-index", 0, 4)).unwrap();
    q.enqueue(JobRow::cold_extract("root-b", "module-rels", 0, 3)).unwrap();

    let mut order: Vec<String> = Vec::new();
    while let Some((key, _root)) = claimable_cold(&q) {
        order.push(key.clone());
        q.poke(
            "UPDATE Jobs SET status='Done', done_at=strftime('%s','now') \
             WHERE idempotency_key=?1",
            &[key.as_str().into()],
        )
        .unwrap();
        // The reconciler runs continuously in the live daemon; a pass after
        // each completion promotes the next root once the active one drains.
        q.reconcile().unwrap();
    }
    assert_eq!(order.len(), 6, "every seeded node claimed exactly once: {order:?}");

    let last_a = order.iter().rposition(|k| k.starts_with("cold:root-a:")).unwrap();
    let first_b = order.iter().position(|k| k.starts_with("cold:root-b:")).unwrap();
    assert!(
        last_a < first_b,
        "root-a's cold work must fully finish before root-b's first claim; order={order:?}"
    );
    assert_eq!(order[0], "cold:root-a:scip-index:0", "root-a's scip claims first overall");
    let b_order: Vec<&str> = order.iter().filter(|k| k.starts_with("cold:root-b:"))
        .map(|s| s.as_str()).collect();
    assert_eq!(
        b_order[0], "cold:root-b:scip-index:0",
        "root-b's own scip-index still claims first among root-b's nodes"
    );
}

/// A mid-batch root whose only remaining work is backed off (future `run_at`)
/// does not stall another root: `reconcile` promotes the held root.
#[test]
fn reconcile_promotes_past_a_backed_off_root() {
    let (q, _d) = queue();
    q.enqueue(JobRow::cold_extract("root-a", "module-rels", 0, 3)).unwrap();
    q.enqueue(JobRow::cold_extract("root-b", "module-rels", 0, 3)).unwrap();
    // root-b was pushed held (root-a is active).
    let (_, _, _, _, _, run_at_b) = q.peek("cold:root-b:module-rels:0").unwrap();
    assert!(run_at_b > now_secs() + 3600, "second root's cold job is pushed held");

    // root-a's only remaining row backs off into the future (a retry window).
    q.poke(
        "UPDATE Jobs SET run_at=strftime('%s','now') + 10000 \
         WHERE idempotency_key='cold:root-a:module-rels:0'",
        &[],
    )
    .unwrap();
    let rep = q.reconcile().unwrap();
    assert!(rep.cold_promoted >= 1, "the held root is promoted past the backed-off one");
    let (state_b, _, _, _, _, run_at_b) = q.peek("cold:root-b:module-rels:0").unwrap();
    assert_eq!(state_b, "Pending");
    assert!(run_at_b <= now_secs(), "promoted row is ready now");
    let claimed = claimable_cold(&q).expect("root-b's work claims");
    assert_eq!(claimed.1, "root-b");
}

/// While the active root has ready work, `reconcile` promotes nothing.
#[test]
fn reconcile_holds_the_second_root_while_the_first_has_ready_work() {
    let (q, _d) = queue();
    q.enqueue(JobRow::cold_extract("root-a", "module-rels", 0, 3)).unwrap();
    q.enqueue(JobRow::cold_extract("root-b", "module-rels", 0, 3)).unwrap();
    let rep = q.reconcile().unwrap();
    assert_eq!(rep.cold_promoted, 0, "no promotion while root-a is runnable");
    let (_, _, _, _, _, run_at_b) = q.peek("cold:root-b:module-rels:0").unwrap();
    assert!(run_at_b > now_secs() + 3600, "root-b stays held");
}

// ---- write-volume budget lever (scheduler plan steps 1-2 seam) ----

#[test]
fn reconcile_defers_ready_cold_jobs_when_the_write_budget_is_spent() {
    let dir = TmpDir::new();
    // Budget: 2 jobs per window at 100 estimated bytes each.
    let budget = Arc::new(WriteBudget::new(200, 100));
    let q = test_open_with_budget(&dir.0, Some(budget.clone()));
    q.enqueue(JobRow::cold_extract("r1", "module-rels", 0, 3)).unwrap();
    q.enqueue(JobRow::cold_extract("r1", "type-rels", 0, 2)).unwrap();

    // Budget untouched: nothing deferred.
    let rep = q.reconcile().unwrap();
    assert_eq!(rep.budget_deferred, 0);
    assert!(claimable_cold(&q).is_some());

    // Two jobs' spend recorded: the window is exhausted, ready rows defer.
    budget.record_job();
    budget.record_job();
    let rep = q.reconcile().unwrap();
    assert_eq!(rep.budget_deferred, 2, "both ready cold jobs deferred to the next window");
    assert!(claimable_cold(&q).is_none(), "no cold job is claimable in an exhausted window");
    let (_, _, _, _, _, run_at) = q.peek("cold:r1:module-rels:0").unwrap();
    let now = now_secs();
    assert!(run_at > now && run_at <= now + 61, "deferred to the next window boundary");
}

// ---- retention ----

#[test]
fn reconcile_trims_old_done_rows_but_keeps_fresh_ones() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("old", &[PathBuf::from("/a")])).unwrap();
    q.enqueue(JobRow::tick("fresh", &[PathBuf::from("/b")])).unwrap();
    q.poke("UPDATE Jobs SET status='Done', done_at=1 WHERE idempotency_key='tick:old'", &[])
        .unwrap();
    q.poke(
        "UPDATE Jobs SET status='Done', done_at=strftime('%s','now') \
         WHERE idempotency_key='tick:fresh'",
        &[],
    )
    .unwrap();

    let rep = q.reconcile().unwrap();
    assert_eq!(rep.trimmed, 1, "the aged done row was not trimmed");
    assert!(q.peek("tick:old").is_none(), "the aged done row still exists");
    assert!(q.peek("tick:fresh").is_some(), "a fresh done row was wrongly trimmed");
}

// ---- end-to-end through the REAL apalis workers ----

/// A runner that records every job it ran (for the coalesce proof).
struct RecordingRunner {
    seen: Arc<Mutex<Vec<JobRow>>>,
}
impl JobRunner for RecordingRunner {
    fn run(&self, job: &JobRow) -> Result<()> {
        plock(&self.seen).push(job.clone());
        Ok(())
    }
}

/// A runner that always errors (for the backoff/park path).
struct FailingRunner;
impl JobRunner for FailingRunner {
    fn run(&self, _job: &JobRow) -> Result<()> {
        anyhow::bail!("boom")
    }
}

/// A runner that PANICS on one root and records the rest — the proof a worker
/// survives an unwinding job (`spawn_blocking` contains the panic; the
/// handler converts it to a job failure).
struct PanicOnRootRunner {
    boom_root: String,
    seen: Arc<Mutex<Vec<String>>>,
}
impl JobRunner for PanicOnRootRunner {
    fn run(&self, job: &JobRow) -> Result<()> {
        if job.root == self.boom_root {
            panic!("runner unwind on {}", job.root);
        }
        plock(&self.seen).push(job.root.clone());
        Ok(())
    }
}

/// Sandbox worker rig: pool + queue + shell ctx + spawned workers.
async fn rig(dir: &TmpDir, runner: Arc<dyn JobRunner>) -> (Arc<JobQueue>, ShellCtx, CancellationToken) {
    let pool = workers::open_pool(&dir.0.join("jobs.sqlite")).await.expect("pool");
    let q = JobQueue::open_with_budget(&dir.0, None).expect("open");
    let cancel = CancellationToken::new();
    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<String>(8);
    let ctx = ShellCtx {
        rt: tokio::runtime::Handle::current(),
        cancel: cancel.clone(),
        job_notify: Arc::new(tokio::sync::Notify::new()),
        broadcast_tx,
    };
    workers::spawn_workers(&ctx, &pool, q.clone(), runner, 1);
    (q, ctx, cancel)
}

pub(crate) async fn wait_for_state(q: &JobQueue, key: &str, state: &str, tries: u32) -> bool {
    for _ in 0..tries {
        if q.peek(key).map(|r| r.0 == state).unwrap_or(false) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Two enqueues land before the worker fetches; the worker then runs the tick
/// EXACTLY once, with a payload carrying BOTH paths.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workers_run_a_coalesced_tick_once_with_both_paths() {
    let dir = TmpDir::new();
    let seen = Arc::new(Mutex::new(Vec::<JobRow>::new()));
    // Enqueue BEFORE the workers spawn so both saves coalesce onto one row.
    {
        let pool = workers::open_pool(&dir.0.join("jobs.sqlite")).await.expect("pool");
        pool.close().await;
    }
    let q0 = JobQueue::open_with_budget(&dir.0, None).expect("open");
    q0.enqueue(JobRow::tick("r1", &[PathBuf::from("/one.rs")])).unwrap();
    q0.enqueue(JobRow::tick("r1", &[PathBuf::from("/two.rs")])).unwrap();
    drop(q0);

    let (q, ctx, cancel) = rig(&dir, Arc::new(RecordingRunner { seen: seen.clone() })).await;
    ctx.job_notify.notify_waiters();
    assert!(wait_for_state(&q, "tick:r1", "Done", 200).await,
        "the coalesced tick job never completed");
    cancel.cancel();

    let runs = plock(&seen);
    assert_eq!(runs.len(), 1, "two rapid saves must produce ONE tick execution, got {}", runs.len());
    assert_eq!(
        runs[0].paths(),
        vec![PathBuf::from("/one.rs"), PathBuf::from("/two.rs")],
        "the single execution's payload carries both coalesced paths"
    );
}

/// A single worker panics on the `boom` tick, then still runs the `ok` tick —
/// the panic did not kill the worker. The panicked job counts as a failure
/// (attempts bumped, not left in flight).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_survives_a_panicking_runner_and_keeps_serving() {
    let dir = TmpDir::new();
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let (q, ctx, cancel) = rig(
        &dir,
        Arc::new(PanicOnRootRunner { boom_root: "boom".into(), seen: seen.clone() }),
    )
    .await;
    q.enqueue(JobRow::tick("boom", &[PathBuf::from("/a")])).unwrap();
    q.enqueue(JobRow::tick("ok", &[PathBuf::from("/b")])).unwrap();
    ctx.job_notify.notify_waiters();

    assert!(wait_for_state(&q, "tick:ok", "Done", 200).await,
        "the worker died on the panic; the good job never ran");
    // The panicked job was treated as a failure (attempts bumped, backed off),
    // not silently dropped or left running.
    let mut failed_seen = false;
    for _ in 0..100 {
        if let Some((state, _, _, _, attempts, _)) = q.peek("tick:boom") {
            if state != "Running" && state != "Queued" && attempts >= 1 {
                failed_seen = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    cancel.cancel();
    assert!(failed_seen, "a panicking job must count as a failed attempt");
    assert_eq!(*plock(&seen), vec!["ok".to_string()]);
}

/// A job whose runner keeps erroring backs off (future `run_at` stamped by the
/// failure path) and eventually parks as `Killed` at the attempt cap.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workers_park_a_persistently_failing_job() {
    let dir = TmpDir::new();
    let (q, ctx, cancel) = rig(&dir, Arc::new(FailingRunner)).await;
    q.enqueue(JobRow::sink_drain("r1")).unwrap();
    ctx.job_notify.notify_waiters();

    // Each failure backs off; force the row ready-now between attempts rather
    // than waiting out the natural 2/4/8s schedule, until it parks.
    let mut parked = false;
    'outer: for _ in 0..(MAX_ATTEMPTS as usize + 2) {
        for _ in 0..100 {
            match q.peek("sink:r1") {
                Some((state, ..)) if state == "Killed" => {
                    parked = true;
                    break 'outer;
                }
                Some((state, _, _, _, _, run_at)) if state == "Failed" && run_at > now_secs() => {
                    // Backed off: the stamp landed. Pull it ready-now.
                    q.poke(
                        "UPDATE Jobs SET run_at=0 WHERE idempotency_key='sink:r1'",
                        &[],
                    )
                    .unwrap();
                    ctx.job_notify.notify_waiters();
                    break;
                }
                _ => {}
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    cancel.cancel();
    assert!(parked, "a persistently failing job must park as Killed, not spin: {:?}",
        q.peek("sink:r1"));
    let (_, _, _, _, attempts, _) = q.peek("sink:r1").unwrap();
    assert_eq!(attempts, MAX_ATTEMPTS, "parks exactly at the attempt cap");
}
