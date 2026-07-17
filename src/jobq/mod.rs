//! Durable SQLite job queue + dispatcher (arc J1 of the job-queue plan).
//!
//! Per-root tick work used to run INLINE on the watcher thread that observed a
//! file change (and the poll thread for `@async` drains). Under load, a tick
//! held its engine mutex for its whole synchronous body while the watcher thread
//! sat blocked inside it — the "ticks are lock camping" shape. This module lifts
//! that work onto a durable queue: watchers/pollers `enqueue` a job and return;
//! a small worker set claims and runs it. The behavior is preserved (a worker
//! calls the SAME `tick_paths` / drain the inline path called, under the SAME
//! engine mutex) — but ticks are now visible (`dl daemon jobs`), listable,
//! coalesced (rapid saves collapse to one tick job carrying every changed
//! path), and durable (a `running` job is reset to pending on the next boot).
//!
//! The table lives in a small dedicated `<home>/jobs.sqlite` opened via
//! `db::open` — the daemon's other on-disk dbs are per-root corpus engines
//! (`roots/<key>/db.sqlite`) and the config-view engine (`<home>/db.sqlite`),
//! all owned by an `Engine`; a control table has no business sharing an engine's
//! db, so it gets its own file. The lock-free `dl daemon jobs` read opens a
//! read-only connection on it (`db::open_read_only`), the same pattern
//! `daemon_read` uses for row/query RPCs.
//!
//! Scope note: this is J1 only. J2 (live priorities beyond the column) and J3
//! (per-stratum `DeriveStratum` jobs + mid-tick cancellation) are follow-ups;
//! the `cancelled` column and `priority` ordering are wired now so those arcs
//! add rows, not schema.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::db::Db;

/// Bounded retry count: a job whose runner keeps erroring parks in `failed`
/// after this many attempts instead of retrying forever.
pub(crate) const MAX_ATTEMPTS: i64 = 5;

/// A `running` row whose `started_at` predates `now - LEASE_SECS` is treated by
/// `sweep` as an abandoned lease and reset to `pending`. `reset_running_on_boot`
/// recovers a dead PROCESS; this recovers a dead WORKER in a live process (a
/// silently-exited worker thread, a wedged tick) whose `tick:{root}` row would
/// otherwise block that root forever — one row exists per key, so every new
/// enqueue only sets `dirty` and waits for a `finish` that never comes. This is
/// the live-sweeper both effectum and apalis converge on (effectum's
/// `expires_at` startup recovery leaves peer-worker jobs stuck until restart;
/// apalis's `reenqueue_orphaned` runs it on a live interval).
///
/// Coarse on purpose: with no per-job heartbeat this must exceed the longest
/// legitimate tick (a cold full-corpus rebuild) so a merely-slow tick is never
/// reclaimed and double-run. effectum's `expires_at` heartbeat extension is the
/// finer-grained refinement (J3). The `finish` state-check makes the reclaim
/// race safe even if a slow tick IS reclaimed: the original worker's late
/// `finish` sees the row no longer `running` and declines to stomp it.
pub(crate) const LEASE_SECS: i64 = 900;

/// `done` rows finished before `now - DONE_RETAIN_SECS` are deleted by `sweep`.
/// Row count is bounded by distinct `key` already (the UPSERT reuses one row per
/// `tick:{root}`/`sink:{root}`), so this only reaps terminal rows for roots that
/// were dropped — it is not a defense against per-execution row growth (there is
/// none). Both effectum (a `// TODO delete old jobs`) and apalis (a manual
/// `vacuum()` never scheduled) leave retention unaddressed.
pub(crate) const DONE_RETAIN_SECS: i64 = 3600;

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS _job(
  key TEXT PRIMARY KEY, kind TEXT NOT NULL, root TEXT NOT NULL,
  arg TEXT NOT NULL DEFAULT '{}', priority INTEGER NOT NULL DEFAULT 0,
  state TEXT NOT NULL DEFAULT 'pending',
  dirty INTEGER NOT NULL DEFAULT 0, cancelled INTEGER NOT NULL DEFAULT 0,
  attempts INTEGER NOT NULL DEFAULT 0, run_at INTEGER NOT NULL DEFAULT 0,
  enqueued_at INTEGER, started_at INTEGER, finished_at INTEGER, last_error TEXT
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS _job_ready ON _job(state, run_at, priority);";

/// Poison-tolerant mutex lock (same policy as `daemon::lock`). A worker
/// panicking mid-claim should not brick the queue for every other worker; the
/// guarded state is a `Db` behind SQLite transactions that roll back on unwind.
#[inline]
fn plock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Failure backoff: seconds to defer a job's next attempt. `2, 4, 8, ...`
/// capped at 300s. Pure (unit-testable without a live queue).
pub(crate) fn backoff_secs(attempts: i64) -> i64 {
    let shift = attempts.clamp(1, 20) as u32;
    (1i64 << shift).min(300)
}

/// `backoff_secs` plus additive jitter spread over `[base, base + base/2]`,
/// deterministic in `seed` (unit-testable). Per-root keys mean a single bad
/// global config fails EVERY root's tick on the same wall-second; without jitter
/// they all back off to an identical `run_at` and wake together — a thundering
/// herd on the engine mutex. effectum multiplies its backoff by
/// `1 + rand()*randomization`; apalis adds none and its fixed-interval poll
/// wakes every worker in lockstep. Jitter is applied at the `finish` call site
/// (seeded from key+now), leaving `backoff_secs` a pure, testable base.
pub(crate) fn jittered_backoff(attempts: i64, seed: u64) -> i64 {
    let base = backoff_secs(attempts);
    let spread = (base / 2).max(1);
    base + (seed % (spread as u64 + 1)) as i64
}

/// Deterministic per-call jitter seed: mix the job key with the wall-second so
/// two roots failing on the same tick draw different offsets.
fn jitter_seed(key: &str, now: i64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    now.hash(&mut hasher);
    hasher.finish()
}

/// The job kinds J1 routes. `DeriveStratum` (per-stratum derive jobs) is J3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobKind {
    /// Re-tick a root over a set of changed paths (`tick_paths`).
    Tick,
    /// Drain a root's `@async`/external-sink effects (`poll_tick`).
    SinkDrain,
}

impl JobKind {
    fn as_str(self) -> &'static str {
        match self {
            JobKind::Tick => "tick",
            JobKind::SinkDrain => "sink_drain",
        }
    }
    fn parse(s: &str) -> Option<JobKind> {
        match s {
            "tick" => Some(JobKind::Tick),
            "sink_drain" => Some(JobKind::SinkDrain),
            _ => None,
        }
    }
}

/// One enqueued unit of work. `arg` coalesces across re-requests (Tick unions
/// its changed-path array). `key` is the dedup identity: one `tick:{root}` /
/// `sink:{root}` row can exist per root, so per-root work serializes by
/// construction.
#[derive(Clone, Debug)]
pub(crate) struct JobRow {
    pub key: String,
    pub kind: JobKind,
    pub root: String,
    pub arg: Value,
    pub priority: i64,
    /// Retry count carried out of `claim` for a runner / J3 to inspect; the J1
    /// runner ignores it (backoff is the queue's job), so it reads as unused.
    #[allow(dead_code)]
    pub attempts: i64,
    pub run_at: i64,
}

impl JobRow {
    /// A `tick:{root_id}` job carrying the changed paths.
    pub fn tick(root_id: &str, paths: &[PathBuf]) -> JobRow {
        let arr: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        JobRow {
            key: format!("tick:{root_id}"),
            kind: JobKind::Tick,
            root: root_id.to_string(),
            arg: json!({ "paths": arr }),
            priority: 0,
            attempts: 0,
            run_at: 0,
        }
    }

    /// A `sink:{root_id}` effect-drain job.
    pub fn sink_drain(root_id: &str) -> JobRow {
        JobRow {
            key: format!("sink:{root_id}"),
            kind: JobKind::SinkDrain,
            root: root_id.to_string(),
            arg: json!({}),
            priority: 0,
            attempts: 0,
            run_at: 0,
        }
    }

    /// The changed paths carried by a Tick job's coalesced `arg`.
    pub fn paths(&self) -> Vec<PathBuf> {
        paths_of(&self.arg).into_iter().map(PathBuf::from).collect()
    }
}

/// The outcome of `finish`: what state the row landed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Requeue {
    /// Completed; row is `done`.
    Done,
    /// Re-opened `pending` — either the coalesce promise (a re-request arrived
    /// while running set `dirty`), or a bounded failure retry (backoff).
    Repending,
    /// Retries exhausted; row is `failed`.
    Parked,
}
/// The durable queue: one per daemon process, its own `jobs.sqlite`. All
/// mutating ops serialize on the `Db` mutex (SQLite is single-writer anyway);
/// the read-only `list` opens its own connection and never takes it.
pub(crate) struct JobQueue {
    db_path: PathBuf,
    db: Mutex<Db>,
    /// Doorbell: `wake_gen` bumps on every enqueue / requeue, waking a worker
    /// blocked in `wait_for_work` instead of forcing it to poll on a timer.
    wake_gen: Mutex<u64>,
    wake_cv: Condvar,
}

impl JobQueue {
    /// Open (creating) `<home>/jobs.sqlite` and ensure the `_job` schema.
    pub(crate) fn open(home: &Path) -> Result<Arc<JobQueue>> {
        let db_path = home.join("jobs.sqlite");
        let db = crate::db::open(Some(&db_path.to_string_lossy()))?;
        db.execute_batch(SCHEMA)?;
        Ok(Arc::new(JobQueue {
            db_path,
            db: Mutex::new(db),
            wake_gen: Mutex::new(0),
            wake_cv: Condvar::new(),
        }))
    }

    /// UPSERT a job. Coalescing IS the upsert:
    ///   - a re-request while `pending` collapses onto the row (arg union,
    ///     priority max, attempts reset, ready now);
    ///   - a re-request while `running` sets `dirty=1` so the row re-runs once
    ///     after the current execution finishes (arg union carries the new
    ///     paths into that rerun);
    ///   - a re-request onto a `done`/`failed` row re-opens it `pending`.
    pub(crate) fn enqueue(&self, job: JobRow) -> Result<()> {
        let now = now_secs();
        let inner = {
            let db = plock(&self.db);
            let conn = db.conn();
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let body = (|| -> Result<()> {
                let existing: Option<String> = conn
                    .query_row("SELECT arg FROM _job WHERE key=?1", params![job.key], |r| r.get(0))
                    .optional()?;
                let merged = match &existing {
                    Some(prev) => merge_args(prev, &job.arg),
                    None => job.arg.to_string(),
                };
                let run_at = job.run_at.max(0);
                conn.execute(
                    "INSERT INTO _job(key, kind, root, arg, priority, state, dirty, cancelled, \
                       attempts, run_at, enqueued_at) \
                     VALUES(?1,?2,?3,?4,?5,'pending',0,0,0,?6,?7) \
                     ON CONFLICT(key) DO UPDATE SET \
                       arg = ?4, \
                       priority = max(priority, ?5), \
                       dirty = CASE WHEN state='running' THEN 1 ELSE 0 END, \
                       state = CASE WHEN state='running' THEN 'running' ELSE 'pending' END, \
                       attempts = CASE WHEN state='running' THEN attempts ELSE 0 END, \
                       run_at = CASE WHEN state='running' THEN run_at ELSE ?6 END, \
                       cancelled = 0, \
                       enqueued_at = ?7, \
                       started_at = CASE WHEN state='running' THEN started_at ELSE NULL END, \
                       finished_at = CASE WHEN state='running' THEN finished_at ELSE NULL END, \
                       last_error = CASE WHEN state='running' THEN last_error ELSE NULL END",
                    params![job.key, job.kind.as_str(), job.root, merged, job.priority, run_at, now],
                )?;
                Ok(())
            })();
            finish_tx(conn, &body)?;
            body
        };
        if inner.is_ok() {
            self.wake();
        }
        inner
    }

    /// Claim the highest-priority ready job of an open kind, marking it
    /// `running` in one `BEGIN IMMEDIATE` transaction (SQLite single-writer is
    /// the coordinator). `None` = nothing ready.
    pub(crate) fn claim(&self, kinds: &[JobKind]) -> Result<Option<JobRow>> {
        if kinds.is_empty() {
            return Ok(None);
        }
        let now = now_secs();
        let kind_list = kinds
            .iter()
            .map(|k| format!("'{}'", k.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let db = plock(&self.db);
        let conn = db.conn();
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let body = (|| -> Result<Option<JobRow>> {
            let picked: Option<(String, String, String, String, i64, i64, i64)> = conn
                .query_row(
                    &format!(
                        "SELECT key, kind, root, arg, priority, attempts, run_at FROM _job \
                         WHERE state='pending' AND cancelled=0 AND run_at<=?1 AND kind IN ({kind_list}) \
                         ORDER BY priority DESC, run_at ASC, enqueued_at ASC LIMIT 1"
                    ),
                    params![now],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
                )
                .optional()?;
            let Some((key, kind_s, root, arg_s, priority, attempts, run_at)) = picked else {
                return Ok(None);
            };
            conn.execute(
                "UPDATE _job SET state='running', started_at=?2 WHERE key=?1",
                params![key, now],
            )?;
            let kind = JobKind::parse(&kind_s).unwrap_or(JobKind::Tick);
            let arg = serde_json::from_str(&arg_s).unwrap_or_else(|_| json!({}));
            Ok(Some(JobRow { key, kind, root, arg, priority, attempts, run_at }))
        })();
        finish_tx(conn, &body)?;
        body
    }

    /// Close out a claimed job. Success -> `done`, unless a re-request set
    /// `dirty` while it ran (then re-open `pending` — the coalesce rerun).
    /// Failure -> bounded backoff: `pending` with a future `run_at` until
    /// `MAX_ATTEMPTS`, then park in `failed`.
    pub(crate) fn finish(&self, key: &str, outcome: Result<()>) -> Result<Requeue> {
        let now = now_secs();
        let inner = {
            let db = plock(&self.db);
            let conn = db.conn();
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let body = (|| -> Result<Requeue> {
                let row: Option<(String, i64, i64)> = conn
                    .query_row(
                        "SELECT state, dirty, attempts FROM _job WHERE key=?1",
                        params![key],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?;
                let Some((state, dirty, attempts)) = row else {
                    // Row vanished (dropped/cancelled out from under us).
                    return Ok(Requeue::Done);
                };
                if state != "running" {
                    // The lease `sweep` reclaimed this row (reset it to `pending`)
                    // while our slow tick ran, and another worker may already own
                    // the rerun. Do not stomp the superseding row back to done/
                    // failed — the reclaimed attempt is authoritative now. Safe
                    // because this read and any write share one BEGIN IMMEDIATE tx.
                    return Ok(Requeue::Done);
                }
                match outcome {
                    Ok(()) => {
                        if dirty != 0 {
                            conn.execute(
                                "UPDATE _job SET state='pending', dirty=0, run_at=?2, \
                                 started_at=NULL, finished_at=?2, last_error=NULL WHERE key=?1",
                                params![key, now],
                            )?;
                            Ok(Requeue::Repending)
                        } else {
                            conn.execute(
                                "UPDATE _job SET state='done', finished_at=?2, last_error=NULL \
                                 WHERE key=?1",
                                params![key, now],
                            )?;
                            Ok(Requeue::Done)
                        }
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        let attempts_new = attempts + 1;
                        if attempts_new >= MAX_ATTEMPTS {
                            conn.execute(
                                "UPDATE _job SET state='failed', attempts=?2, started_at=NULL, \
                                 finished_at=?3, last_error=?4 WHERE key=?1",
                                params![key, attempts_new, now, msg],
                            )?;
                            Ok(Requeue::Parked)
                        } else {
                            let run_at = now + jittered_backoff(attempts_new, jitter_seed(key, now));
                            conn.execute(
                                "UPDATE _job SET state='pending', attempts=?2, run_at=?3, \
                                 started_at=NULL, finished_at=NULL, last_error=?4 WHERE key=?1",
                                params![key, attempts_new, run_at, msg],
                            )?;
                            Ok(Requeue::Repending)
                        }
                    }
                }
            })();
            finish_tx(conn, &body)?;
            body
        };
        if matches!(inner, Ok(Requeue::Repending)) {
            self.wake();
        }
        inner
    }

    /// Flag a job cancelled; `claim` skips cancelled rows. (J3 checks the flag
    /// mid-run between strata; J1 just declines to start it — so no J1 caller
    /// yet outside tests.)
    #[allow(dead_code)]
    pub(crate) fn cancel(&self, key: &str) -> Result<()> {
        let db = plock(&self.db);
        db.conn()
            .execute("UPDATE _job SET cancelled=1 WHERE key=?1", params![key])?;
        Ok(())
    }

    /// Crash recovery, run once at boot before serving: any job left `running`
    /// by a previous (crashed/killed) process is reset to `pending` so a worker
    /// picks it up again. Returns the count reset.
    pub(crate) fn reset_running_on_boot(&self) -> Result<usize> {
        let db = plock(&self.db);
        let n = db.conn().execute(
            "UPDATE _job SET state='pending', started_at=NULL WHERE state='running'",
            [],
        )?;
        Ok(n)
    }

    /// Periodic maintenance, two ops in one write tx (the daemon's `idle_loop`
    /// calls it, ~30s cadence):
    ///   1. **lease reclaim** — any row still `running` whose `started_at`
    ///      predates `now - lease_secs` is reset to `pending` (see `LEASE_SECS`).
    ///      Wakes a worker when it reclaims anything so the re-pended job runs.
    ///   2. **terminal retention** — `done` rows finished before
    ///      `now - done_retain_secs` are deleted (see `DONE_RETAIN_SECS`).
    /// Returns `(reclaimed, deleted)`. Both are set-ops, no per-row loop.
    pub(crate) fn sweep(&self, lease_secs: i64, done_retain_secs: i64) -> Result<(usize, usize)> {
        let now = now_secs();
        let out = {
            let db = plock(&self.db);
            let conn = db.conn();
            conn.execute_batch("BEGIN IMMEDIATE")?;
            let body = (|| -> Result<(usize, usize)> {
                let reclaimed = conn.execute(
                    "UPDATE _job SET state='pending', started_at=NULL \
                     WHERE state='running' AND started_at IS NOT NULL AND started_at < ?1",
                    params![now - lease_secs],
                )?;
                let deleted = conn.execute(
                    "DELETE FROM _job \
                     WHERE state='done' AND finished_at IS NOT NULL AND finished_at < ?1",
                    params![now - done_retain_secs],
                )?;
                Ok((reclaimed, deleted))
            })();
            finish_tx(conn, &body)?;
            body?
        };
        if out.0 > 0 {
            self.wake();
        }
        Ok(out)
    }

    /// Every job, newest first (`enqueued_at DESC`), off a read-only
    /// connection — the lock-free `dl daemon jobs` read (never the engine lock,
    /// never the queue mutex).
    pub(crate) fn list(&self) -> Result<Vec<JobListRow>> {
        let conn = crate::db::open_read_only(&self.db_path.to_string_lossy())?;
        let mut stmt = conn.prepare(
            "SELECT key, kind, root, state, priority, attempts, run_at, \
                    enqueued_at, started_at, finished_at, last_error \
             FROM _job ORDER BY enqueued_at DESC, key",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(JobListRow {
                key: r.get(0)?,
                kind: r.get(1)?,
                root: r.get(2)?,
                state: r.get(3)?,
                priority: r.get(4)?,
                attempts: r.get(5)?,
                run_at: r.get(6)?,
                enqueued_at: r.get(7)?,
                started_at: r.get(8)?,
                finished_at: r.get(9)?,
                last_error: r.get(10)?,
            })
        })?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// Bump the doorbell and wake every worker parked in `wait_for_work`.
    pub(crate) fn wake(&self) {
        {
            let mut g = plock(&self.wake_gen);
            *g = g.wrapping_add(1);
        }
        self.wake_cv.notify_all();
    }

    /// Block until the doorbell rings (an enqueue/requeue) or `timeout` — the
    /// safety net that re-checks a future-`run_at` (backed-off) job even with no
    /// new enqueue. `last_seen` carries the generation the caller last observed.
    pub(crate) fn wait_for_work(&self, last_seen: &mut u64, timeout: Duration) {
        let g = plock(&self.wake_gen);
        if *g != *last_seen {
            *last_seen = *g;
            return;
        }
        let (g2, _timed_out) = self
            .wake_cv
            .wait_timeout(g, timeout)
            .unwrap_or_else(|p| p.into_inner());
        *last_seen = *g2;
    }

    /// Test-only raw row peek (state, dirty, arg, priority, attempts, run_at).
    #[cfg(test)]
    fn peek(&self, key: &str) -> Option<(String, i64, String, i64, i64, i64)> {
        let db = plock(&self.db);
        db.conn()
            .query_row(
                "SELECT state, dirty, arg, priority, attempts, run_at FROM _job WHERE key=?1",
                params![key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()
            .ok()
            .flatten()
    }
}

/// Commit on `Ok`, roll back on `Err` — keeps a failed mid-transaction op from
/// leaving the shared connection stuck inside a transaction (the next
/// `BEGIN IMMEDIATE` would otherwise fail "cannot start a transaction within a
/// transaction").
fn finish_tx<T>(conn: &rusqlite::Connection, body: &Result<T>) -> Result<()> {
    match body {
        Ok(_) => conn.execute_batch("COMMIT")?,
        Err(_) => {
            let _ = conn.execute_batch("ROLLBACK");
        }
    }
    Ok(())
}

/// Union a Tick job's `paths` array across a coalescing re-request, order-
/// preserving and deduped. Non-path args (SinkDrain's `{}`) pass through.
fn merge_args(prev: &str, incoming: &Value) -> String {
    let prev_v: Value = serde_json::from_str(prev).unwrap_or_else(|_| json!({}));
    let mut paths = paths_of(&prev_v);
    for p in paths_of(incoming) {
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    if paths.is_empty() {
        incoming.to_string()
    } else {
        json!({ "paths": paths }).to_string()
    }
}

fn paths_of(v: &Value) -> Vec<String> {
    v.get("paths")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// One row of `dl daemon jobs`.
pub(crate) struct JobListRow {
    pub key: String,
    pub kind: String,
    pub root: String,
    pub state: String,
    pub priority: i64,
    pub attempts: i64,
    pub run_at: i64,
    pub enqueued_at: Option<i64>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub last_error: Option<String>,
}

mod dispatch;
#[allow(unused_imports)] // `Dispatcher` is jobq-test-only since the daemon's
                         // dispatcher moved to `daemon_shell::jobs`.
pub(crate) use dispatch::{Dispatcher, JobRunner};
#[cfg(test)]
mod tests;
