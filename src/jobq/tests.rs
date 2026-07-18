use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// A unique temp dir removed on drop (no `tempfile` dev-dep in this crate).
struct TmpDir(PathBuf);
impl TmpDir {
    fn new() -> Self {
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
    let q = JobQueue::open(&dir.0).unwrap();
    (q, dir)
}

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

/// A runner that always errors (for the backoff-through-the-dispatcher path).
struct FailingRunner;
impl JobRunner for FailingRunner {
    fn run(&self, _job: &JobRow) -> Result<()> {
        anyhow::bail!("boom")
    }
}

// ---- item 7: enqueue-coalesce SQL semantics ----

#[test]
fn enqueue_coalesces_pending_union_arg_and_max_priority() {
    let (q, _d) = queue();
    let mut a = JobRow::tick("r1", &[PathBuf::from("/a")]);
    a.priority = 1;
    q.enqueue(a).unwrap();
    let mut b = JobRow::tick("r1", &[PathBuf::from("/b")]);
    b.priority = 5;
    q.enqueue(b).unwrap();

    let (state, dirty, arg, priority, attempts, _run_at) = q.peek("tick:r1").unwrap();
    assert_eq!(state, "pending");
    assert_eq!(dirty, 0, "coalescing a pending row never sets dirty");
    assert_eq!(priority, 5, "priority is the max across coalesced requests");
    assert_eq!(attempts, 0);
    let paths = paths_of(&serde_json::from_str::<Value>(&arg).unwrap());
    assert_eq!(paths, vec!["/a".to_string(), "/b".to_string()],
        "the pending arg unions both changed paths");

    // Exactly one row exists for the key.
    let all = q.list().unwrap();
    assert_eq!(all.iter().filter(|r| r.key == "tick:r1").count(), 1);
}

#[test]
fn enqueue_while_running_sets_dirty_and_unions_for_the_rerun() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/a")])).unwrap();
    let claimed = q.claim(&[JobKind::Tick]).unwrap().expect("claim");
    assert_eq!(claimed.paths(), vec![PathBuf::from("/a")]);

    // A re-request while running must NOT create a second claimable row; it
    // sets dirty and unions the new path for the post-finish rerun.
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/b")])).unwrap();
    assert!(q.claim(&[JobKind::Tick]).unwrap().is_none(),
        "no second row is claimable while the tick runs");
    let (state, dirty, arg, ..) = q.peek("tick:r1").unwrap();
    assert_eq!(state, "running");
    assert_eq!(dirty, 1, "a re-request while running arms the rerun");
    assert_eq!(paths_of(&serde_json::from_str::<Value>(&arg).unwrap()),
        vec!["/a".to_string(), "/b".to_string()]);

    // Finishing a dirty running job re-opens it pending (the promised rerun),
    // and the rerun carries the unioned paths.
    assert_eq!(q.finish("tick:r1", Ok(())).unwrap(), Requeue::Repending);
    let rerun = q.claim(&[JobKind::Tick]).unwrap().expect("rerun claim");
    assert_eq!(rerun.paths(), vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    assert_eq!(q.finish("tick:r1", Ok(())).unwrap(), Requeue::Done);
}

#[test]
fn claim_orders_by_priority_then_skips_cancelled() {
    let (q, _d) = queue();
    q.enqueue(JobRow::sink_drain("low")).unwrap();
    let mut hi = JobRow::sink_drain("hi");
    hi.priority = 10;
    q.enqueue(hi).unwrap();
    q.cancel("sink:low").unwrap();

    // `low` is cancelled, so the high-priority `hi` is the only claim.
    let first = q.claim(&[JobKind::SinkDrain]).unwrap().expect("claim hi");
    assert_eq!(first.key, "sink:hi");
    assert!(q.claim(&[JobKind::SinkDrain]).unwrap().is_none(),
        "the cancelled low-priority job is never claimed");
}

// ---- item 6(b): durability across a simulated crash ----

#[test]
fn reset_running_on_boot_re_pends_a_crashed_job() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/a")])).unwrap();
    // Claim marks it running (a worker grabbed it) — then the process
    // "crashes" without finishing.
    let _ = q.claim(&[JobKind::Tick]).unwrap().expect("claim");
    assert_eq!(q.peek("tick:r1").unwrap().0, "running");

    let n = q.reset_running_on_boot().unwrap();
    assert_eq!(n, 1, "one running job reset on boot");
    assert_eq!(q.peek("tick:r1").unwrap().0, "pending",
        "a crashed running job is pending again after reset_running_on_boot");
    // And it is claimable again.
    assert!(q.claim(&[JobKind::Tick]).unwrap().is_some());
}

// ---- item 6(d): bounded failure backoff ----

#[test]
fn failure_backs_off_then_parks_after_the_attempt_bound() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/a")])).unwrap();
    q.claim(&[JobKind::Tick]).unwrap().expect("claim");

    // First failure: pending, attempts=1, run_at in the future.
    let now = now_secs();
    assert_eq!(q.finish("tick:r1", Err(anyhow::anyhow!("x"))).unwrap(), Requeue::Repending);
    let (state, _dirty, _arg, _prio, attempts, run_at) = q.peek("tick:r1").unwrap();
    assert_eq!(state, "pending");
    assert_eq!(attempts, 1);
    assert!(run_at > now, "a failed job backs off to a future run_at ({run_at} <= {now})");

    // Keep failing until the bound; the row parks in `failed` and stops.
    // One finish already landed (attempts=1), so MAX_ATTEMPTS-1 more reach it.
    // A worker re-claims (-> running) before each retry, as it does live — the
    // `finish` ownership guard requires the row be `running`, so force it ready
    // and re-claim between attempts rather than finishing a `pending` row.
    let mut last = Requeue::Repending;
    for _ in 1..MAX_ATTEMPTS {
        {
            let db = plock(&q.db);
            db.conn()
                .execute("UPDATE _job SET run_at=0 WHERE key='tick:r1' AND state='pending'", [])
                .unwrap();
        }
        q.claim(&[JobKind::Tick]).unwrap().expect("re-claim before the next attempt");
        last = q.finish("tick:r1", Err(anyhow::anyhow!("x"))).unwrap();
    }
    assert_eq!(last, Requeue::Parked);
    let (state, .., attempts, _run_at) = q.peek("tick:r1").unwrap();
    assert_eq!(state, "failed");
    assert_eq!(attempts, MAX_ATTEMPTS);
}

#[test]
fn backoff_secs_doubles_then_caps() {
    assert_eq!(backoff_secs(1), 2);
    assert_eq!(backoff_secs(2), 4);
    assert_eq!(backoff_secs(3), 8);
    assert_eq!(backoff_secs(20), 300, "caps at 300s, never grows unbounded");
    assert_eq!(backoff_secs(100), 300, "a huge attempt count must not overflow/panic");
}

// ---- item 6(a): coalesce PROOF through the dispatcher ----

/// Two enqueues land before any worker claims; the dispatcher then runs the
/// tick EXACTLY once, with an arg carrying BOTH paths.
#[test]
fn dispatcher_runs_coalesced_tick_once_with_both_paths() {
    let (q, _d) = queue();
    // Enqueue two rapid "saves" for the same root BEFORE the worker exists.
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/one.rs")])).unwrap();
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/two.rs")])).unwrap();

    let seen = Arc::new(Mutex::new(Vec::<JobRow>::new()));
    let runner: Arc<dyn JobRunner> = Arc::new(RecordingRunner { seen: seen.clone() });
    let shutdown = Arc::new(AtomicBool::new(false));
    let dispatcher =
        Dispatcher::spawn(q.clone(), runner, shutdown.clone(), 1, vec![JobKind::Tick]);

    // Wait for the single job to reach `done`.
    let mut done = false;
    for _ in 0..200 {
        if q.peek("tick:r1").map(|r| r.0 == "done").unwrap_or(false) {
            done = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    shutdown.store(true, Ordering::Relaxed);
    dispatcher.join(&q);

    assert!(done, "the coalesced tick job never completed");
    let runs = plock(&seen);
    assert_eq!(runs.len(), 1, "two rapid saves must produce ONE tick execution, got {}", runs.len());
    let paths = runs[0].paths();
    assert_eq!(
        paths,
        vec![PathBuf::from("/one.rs"), PathBuf::from("/two.rs")],
        "the single execution's arg carries both coalesced paths"
    );
    let path_strs: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    tracing::debug!(
        path_strs = ?path_strs,
        "[coalesce-proof] 2 enqueues -> 1 execution; arg paths = {:?}",
        path_strs
    );
}

/// A job whose runner keeps erroring backs off through the dispatcher and
/// eventually parks in `failed` (does not spin forever).
#[test]
fn dispatcher_parks_a_persistently_failing_job() {
    let (q, _d) = queue();
    q.enqueue(JobRow::sink_drain("r1")).unwrap();
    let runner: Arc<dyn JobRunner> = Arc::new(FailingRunner);
    let shutdown = Arc::new(AtomicBool::new(false));
    let dispatcher =
        Dispatcher::spawn(q.clone(), runner, shutdown.clone(), 1, vec![JobKind::SinkDrain]);

    // Each failure backs off; force the row ready-now between attempts rather
    // than waiting out the natural 2/4/8s schedule, and let the worker fail
    // it once more, until it parks.
    let mut parked = false;
    for _ in 0..MAX_ATTEMPTS {
        {
            let db = plock(&q.db);
            let _ = db.conn().execute(
                "UPDATE _job SET run_at=0 WHERE key='sink:r1' AND state='pending'",
                [],
            );
        }
        q.wake();
        for _ in 0..100 {
            if q.peek("sink:r1").map(|r| r.0 == "failed").unwrap_or(false) {
                parked = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if parked {
            break;
        }
    }
    shutdown.store(true, Ordering::Relaxed);
    dispatcher.join(&q);
    assert!(parked, "a persistently failing job must park in `failed`, not spin");
    assert_eq!(q.peek("sink:r1").unwrap().4, MAX_ATTEMPTS);
}

// ---- library-mined hardening (effectum/apalis embodied knowledge) ----

/// A runner that PANICS on one root and records the rest — the proof that a
/// worker survives an unwinding job (effectum wraps its runner in
/// `AssertUnwindSafe(..).catch_unwind()`; apalis's bare handler panic kills the
/// poll task, #639). Our `worker_loop` catches the unwind and fails the job.
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

/// A single worker panics on the `boom` tick, then still runs the `ok` tick —
/// proof the panic did not kill the worker thread. The panicked job backs off
/// (its unwind is treated as a job failure), it does not vanish or wedge.
#[test]
fn worker_survives_a_panicking_runner_and_keeps_serving() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("boom", &[PathBuf::from("/a")])).unwrap();
    q.enqueue(JobRow::tick("ok", &[PathBuf::from("/b")])).unwrap();

    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let runner: Arc<dyn JobRunner> =
        Arc::new(PanicOnRootRunner { boom_root: "boom".into(), seen: seen.clone() });
    let shutdown = Arc::new(AtomicBool::new(false));
    let dispatcher =
        Dispatcher::spawn(q.clone(), runner, shutdown.clone(), 1, vec![JobKind::Tick]);

    // Wait for the good job to complete — it can only do so on a live worker.
    let mut ok_done = false;
    for _ in 0..200 {
        if q.peek("tick:ok").map(|r| r.0 == "done").unwrap_or(false) {
            ok_done = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    shutdown.store(true, Ordering::Relaxed);
    dispatcher.join(&q);

    assert!(ok_done, "the worker died on the panic; the good job never ran");
    assert_eq!(*plock(&seen), vec!["ok".to_string()]);
    // The panicked job was treated as a failure (backed off, attempts bumped),
    // not silently dropped or left running.
    let (state, _dirty, _arg, _prio, attempts, _run_at) = q.peek("tick:boom").unwrap();
    assert!(attempts >= 1, "a panicking job must count as an attempt, got {attempts}");
    assert_ne!(state, "running", "a panicked job must not be left `running`");
}

/// Many workers, many distinct-key jobs: each job runs EXACTLY once. Proves the
/// `BEGIN IMMEDIATE` claim + queue mutex admit no double-claim under concurrency
/// (effectum funnels claims through one writer thread; apalis relies on an
/// atomic conditional `UPDATE ... WHERE status='Pending'`). Distinct keys so
/// coalescing is not what enforces once-ness here.
#[test]
fn concurrent_workers_claim_each_job_exactly_once() {
    let (q, _d) = queue();
    const N: usize = 24;
    for i in 0..N {
        q.enqueue(JobRow::tick(&format!("r{i}"), &[PathBuf::from("/x")])).unwrap();
    }
    let seen = Arc::new(Mutex::new(Vec::<JobRow>::new()));
    let runner: Arc<dyn JobRunner> = Arc::new(RecordingRunner { seen: seen.clone() });
    let shutdown = Arc::new(AtomicBool::new(false));
    let dispatcher =
        Dispatcher::spawn(q.clone(), runner, shutdown.clone(), 8, vec![JobKind::Tick]);

    let mut all_done = false;
    for _ in 0..400 {
        let done = (0..N).all(|i| {
            q.peek(&format!("tick:r{i}")).map(|r| r.0 == "done").unwrap_or(false)
        });
        if done {
            all_done = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    shutdown.store(true, Ordering::Relaxed);
    dispatcher.join(&q);

    assert!(all_done, "not every job reached done");
    let runs = plock(&seen);
    assert_eq!(runs.len(), N, "each job must run exactly once across the worker set");
    let mut keys: Vec<String> = runs.iter().map(|j| j.key.clone()).collect();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), N, "a job was claimed and run by more than one worker");
}

/// `sweep` reclaims a `running` row whose lease expired — a worker that died
/// WITHOUT the process dying (silent thread exit / wedged tick), which
/// `reset_running_on_boot` (a boot-only op) would never reach. This is apalis's
/// live `reenqueue_orphaned`; effectum only recovers such rows at restart.
#[test]
fn sweep_reclaims_an_expired_running_lease() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/a")])).unwrap();
    q.claim(&[JobKind::Tick]).unwrap().expect("claim");
    assert_eq!(q.peek("tick:r1").unwrap().0, "running");
    // Backdate the lease so it predates now - LEASE_SECS regardless of the
    // wall-second the claim stamped.
    {
        let db = plock(&q.db);
        db.conn()
            .execute("UPDATE _job SET started_at=1 WHERE key='tick:r1'", [])
            .unwrap();
    }

    let (reclaimed, _deleted) = q.sweep(LEASE_SECS, DONE_RETAIN_SECS).unwrap();
    assert_eq!(reclaimed, 1, "the expired lease was not reclaimed");
    assert_eq!(q.peek("tick:r1").unwrap().0, "pending",
        "an expired running lease must return to pending");
    assert!(q.claim(&[JobKind::Tick]).unwrap().is_some(),
        "the reclaimed job must be claimable again");

    // A freshly-claimed (started_at = now) lease is NOT reclaimed.
    let (reclaimed2, _) = q.sweep(LEASE_SECS, DONE_RETAIN_SECS).unwrap();
    assert_eq!(reclaimed2, 0, "a live lease must not be reclaimed");
}

/// A worker whose slow job was reclaimed by `sweep` mid-run must NOT stomp the
/// superseding row when it finally finishes: the reclaimed `pending` attempt is
/// authoritative. Guards the lease-reclaim race (effectum's worker-scoped
/// `expires_at`/heartbeat ownership check is the analog).
#[test]
fn finish_declines_to_stomp_a_reclaimed_row() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("r1", &[PathBuf::from("/a")])).unwrap();
    q.claim(&[JobKind::Tick]).unwrap().expect("claim");
    // Simulate the sweep reclaiming it (state back to pending) while the slow
    // worker is still running the job.
    {
        let db = plock(&q.db);
        db.conn()
            .execute("UPDATE _job SET state='pending', started_at=NULL WHERE key='tick:r1'", [])
            .unwrap();
    }
    // The slow worker's late success must be a no-op, not a stomp to `done`.
    assert_eq!(q.finish("tick:r1", Ok(())).unwrap(), Requeue::Done);
    assert_eq!(q.peek("tick:r1").unwrap().0, "pending",
        "a late finish stomped the reclaimed pending row");
}

/// `sweep` deletes `done` rows older than the retention window and leaves fresh
/// ones. Both effectum and apalis leave this entirely to the operator.
#[test]
fn sweep_trims_old_done_rows_but_keeps_fresh_ones() {
    let (q, _d) = queue();
    q.enqueue(JobRow::tick("old", &[PathBuf::from("/a")])).unwrap();
    q.claim(&[JobKind::Tick]).unwrap().expect("claim");
    q.finish("tick:old", Ok(())).unwrap();
    q.enqueue(JobRow::tick("fresh", &[PathBuf::from("/b")])).unwrap();
    q.claim(&[JobKind::Tick]).unwrap().expect("claim");
    q.finish("tick:fresh", Ok(())).unwrap();
    // Backdate only the `old` row's finish time past the retention window.
    {
        let db = plock(&q.db);
        db.conn()
            .execute("UPDATE _job SET finished_at=1 WHERE key='tick:old'", [])
            .unwrap();
    }

    let (_reclaimed, deleted) = q.sweep(LEASE_SECS, DONE_RETAIN_SECS).unwrap();
    assert_eq!(deleted, 1, "the aged done row was not trimmed");
    assert!(q.peek("tick:old").is_none(), "the aged done row still exists");
    assert!(q.peek("tick:fresh").is_some(), "a fresh done row was wrongly trimmed");
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

// ---- root serialization for ColdExtract (2026-07-18 incident: 4 registered
// roots cold-rebuilt concurrently on a blank-slate boot) ----

/// Fail-pre-fix: seed cold nodes for two sandbox roots exactly as
/// `Engine::seed_cold_nodes` would (a `scip-index` node above every OTHER
/// family, priority scale restarting at each root's own family count), drain
/// every `ColdExtract` job to completion, and assert root-a's cold work
/// finishes BEFORE root-b's first claim. Before the `active_cold_root` fix,
/// global `ORDER BY priority DESC` let root-b's `scip-index` (priority 4)
/// jump ahead of root-a's still-pending `module-rels`/`type-rels`/`call-rels`
/// (priorities 3/2/1) the moment root-a's own scip node finished — this test
/// fails on that code (root-b's scip claims 2nd, not 5th).
#[test]
fn cold_extract_claim_finishes_one_root_before_starting_another() {
    let (q, _d) = queue();
    // root-a seeded first: scip-index highest, then module/type/call descending
    // (the canonical extraction order `cold_family_priority` produces).
    q.enqueue(JobRow::cold_extract("root-a", "scip-index", 0, 4)).unwrap();
    q.enqueue(JobRow::cold_extract("root-a", "module-rels", 0, 3)).unwrap();
    q.enqueue(JobRow::cold_extract("root-a", "type-rels", 0, 2)).unwrap();
    q.enqueue(JobRow::cold_extract("root-a", "call-rels", 0, 1)).unwrap();
    // root-b seeded second, same priority SCALE (its own scip-index is also
    // priority 4 — the exact incident shape: a second root's scip node ties or
    // beats the first root's remaining non-scip nodes on a GLOBAL ordering).
    q.enqueue(JobRow::cold_extract("root-b", "scip-index", 0, 4)).unwrap();
    q.enqueue(JobRow::cold_extract("root-b", "module-rels", 0, 3)).unwrap();

    let mut order: Vec<String> = Vec::new();
    while let Some(job) = q.claim(&[JobKind::ColdExtract]).unwrap() {
        order.push(job.key.clone());
        q.finish(&job.key, Ok(())).unwrap();
    }
    assert_eq!(order.len(), 6, "every seeded node claimed exactly once: {order:?}");

    let last_a = order.iter().rposition(|k| k.starts_with("cold:root-a:")).unwrap();
    let first_b = order.iter().position(|k| k.starts_with("cold:root-b:")).unwrap();
    assert!(
        last_a < first_b,
        "root-a's cold work must fully finish before root-b's first claim; order={order:?}"
    );
    // scip-index still claims first WITHIN each root (priority ordering intact
    // once the active root is chosen).
    assert_eq!(order[0], "cold:root-a:scip-index:0", "root-a's scip claims first overall");
    let b_order: Vec<&str> = order.iter().filter(|k| k.starts_with("cold:root-b:"))
        .map(|s| s.as_str()).collect();
    assert_eq!(
        b_order[0], "cold:root-b:scip-index:0",
        "root-b's own scip-index still claims first among root-b's nodes"
    );
}

/// `active_cold_root` unit-level: no root has any progress yet (a fresh
/// multi-root blank boot) picks the EARLIEST-SEEDED root, not an arbitrary
/// one. `root-z` is enqueued first but backdated `enqueued_at` makes the
/// ordering unambiguous regardless of same-wall-clock-second ties (`root-a`
/// sorts first alphabetically, the tie-break — this pins the PRIMARY key,
/// `enqueued_at`, wins over it).
#[test]
fn active_cold_root_picks_earliest_seeded_when_no_root_has_progress() {
    let (q, _d) = queue();
    q.enqueue(JobRow::cold_extract("root-z", "module-rels", 0, 3)).unwrap();
    q.enqueue(JobRow::cold_extract("root-a", "module-rels", 0, 3)).unwrap();
    let now = now_secs();
    let db = plock(&q.db);
    // Backdates `enqueued_at` so the earliest-seeded assertion below is
    // unambiguous regardless of same-wall-clock-second ties (same
    // test-scenario poke shape as `finish_declines_to_stomp_a_reclaimed_row`).
    // @rusqlite-ok: test-only direct `_job` write
    let conn = db.conn();
    conn.execute(
        "UPDATE _job SET enqueued_at=?1 WHERE key='cold:root-z:module-rels:0'",
        params![now - 100],
    )
    .unwrap();
    conn.execute(
        "UPDATE _job SET enqueued_at=?1 WHERE key='cold:root-a:module-rels:0'",
        params![now],
    )
    .unwrap();
    let root = active_cold_root(conn, now + 1).unwrap();
    assert_eq!(root, Some("root-z".to_string()), "root-z was seeded first, so it stays active");
}

/// A mid-batch root with only a backed-off (future `run_at`) row remaining
/// does not stall another root's ready work: `active_cold_root` falls through
/// once its remaining row is not yet ready.
#[test]
fn active_cold_root_falls_through_a_backed_off_root() {
    let (q, _d) = queue();
    q.enqueue(JobRow::cold_extract("root-a", "module-rels", 0, 3)).unwrap();
    q.enqueue(JobRow::cold_extract("root-b", "module-rels", 0, 3)).unwrap();
    let now = now_secs();
    {
        let db = plock(&q.db);
        // root-a: one row already done, its only remaining row not seeded here
        // — simulate "mid-batch but backed off" by marking root-a done directly
        // (same test-scenario poke shape as the sibling
        // `active_cold_root_picks_earliest_seeded...` test).
        // @rusqlite-ok: test-only direct `_job` write
        let conn = db.conn();
        conn.execute(
            "UPDATE _job SET state='done' WHERE key='cold:root-a:module-rels:0'",
            [],
        )
        .unwrap();
    }
    q.enqueue(JobRow::cold_extract("root-a", "type-rels", 0, 2)).unwrap();
    let db = plock(&q.db);
    // Same test-scenario poke as above, plus reuses this transaction's
    // connection to call `active_cold_root` directly (the raw connection type
    // it takes is the same seam-adjacent shape `claim` passes it).
    // @rusqlite-ok: test-only direct `_job` write
    let conn = db.conn();
    conn.execute(
        "UPDATE _job SET run_at=?1 WHERE key='cold:root-a:type-rels:0'",
        params![now + 10_000],
    )
    .unwrap();
    let root = active_cold_root(conn, now).unwrap();
    assert_eq!(
        root,
        Some("root-b".to_string()),
        "root-a's only remaining row is backed off, so root-b's ready work claims instead"
    );
}
