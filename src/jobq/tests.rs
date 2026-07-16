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
    let mut last = Requeue::Repending;
    for _ in 1..MAX_ATTEMPTS {
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
    eprintln!(
        "[coalesce-proof] 2 enqueues -> 1 execution; arg paths = {:?}",
        paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>()
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
