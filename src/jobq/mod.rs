//! Job queue on **apalis-sqlite** (plan `2026-07-18-infra-library-adoption.md`
//! section 1.5 verdict / 5.3 step 3; "infra is bought, never built").
//!
//! The bespoke `_job` table + claim/finish SQL from the 2026-07-16 job-queue
//! plan is gone. The durable store is apalis-sqlite's `Jobs` table (its own
//! migrations) in the same `<home>/jobs.sqlite`; fetch/lock/ack, priority
//! ordering, `run_at` scheduling, retry bookkeeping, worker registration and
//! live orphan re-enqueue (the old lease sweep) are apalis's. The workers are
//! apalis `Worker`s on the daemon shell runtime (`jobq::workers`).
//!
//! What stays HERE is the admission layer the adoption plan says no crate
//! ships (section 1.6 q7, scheduler plan section 2 cross-cutting finding):
//!
//!   - **coalescing enqueue** — one row per job key. apalis's idempotency key
//!     is `ON CONFLICT DO NOTHING` dedup; the arg-union / dirty-rerun /
//!     reopen-terminal semantics the daemon tests pin are an UPSERT this
//!     module runs over the apalis `Jobs` table (the plan's "SQL-
//!     introspectable" integration posture: `dl daemon why` reads the same
//!     table).
//!   - **root-serialized ColdExtract admission** — a second root's cold jobs
//!     are pushed HELD (`run_at` far future + a metadata flag) until the
//!     active root has no runnable cold work; `reconcile` promotes the next
//!     root. Replaces the old `active_cold_root` claim scoping (2026-07-18
//!     incident: 4 roots cold-rebuilt concurrently).
//!   - **failure backoff stamping** — apalis re-fetches a `Failed` row with
//!     attempts left immediately; `note_failure_backoff` stamps the jittered
//!     2/4/8..300s `run_at` the old queue applied (`calculate_status` parks
//!     the row as `Killed` at the attempt cap — the old `failed` park).
//!   - **req_id cancellation** (class-18 residual) — kill a request's pending
//!     jobs, flag its queued/running ones; the worker aborts a flagged job at
//!     the next job boundary (`AbortError` -> `Killed`, no retry burn).
//!   - **the write-volume budget lever** (`write_budget`) — scheduler plan
//!     steps 1-2 seam: a per-window byte budget `reconcile` enforces by
//!     deferring ready cold jobs to the next window.
//!
//! All of the above is plain SQL through the repo's `Db` seam on the SAME
//! SQLite file apalis's sqlx pool writes (WAL + busy handlers on both sides);
//! the sync daemon threads never touch the async pool.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::db::Db;

pub(crate) mod workers;
pub(crate) mod write_budget;
pub(crate) use workers::JobRunner;
pub(crate) use write_budget::WriteBudget;

/// Bounded retry count, mapped onto apalis's `max_attempts` column: the
/// 5th failed attempt makes `calculate_status` park the row as `Killed`.
pub(crate) const MAX_ATTEMPTS: i64 = 5;

/// `Done` rows finished before `now - DONE_RETAIN_SECS` are deleted by
/// `reconcile` (apalis ships a manual `vacuum()` nothing schedules — retention
/// stays ours, same 1h window the old queue had).
pub(crate) const DONE_RETAIN_SECS: i64 = 3600;

/// The `run_at` a HELD cold job parks at (2100-01-01): apalis's fetch skips
/// `run_at > now`, which is the only pull-side knob an admission layer above
/// the store has. `reconcile` promotes held rows back to `run_at = now`.
const HOLD_RUN_AT: i64 = 4_102_444_800;

/// apalis queue (`Jobs.job_type`) served by the tick/sink worker set.
pub(crate) const ENGINE_QUEUE: &str = "dl-engine";
/// apalis queue served by the single-flight cold-extract worker.
pub(crate) const COLD_QUEUE: &str = "dl-cold";

/// Poison-tolerant mutex lock (same policy as `daemon::lock`).
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
/// deterministic in `seed`. Per-root keys mean a single bad global config
/// fails EVERY root's tick on the same wall-second; without jitter they all
/// wake in lockstep on the engine mutex. apalis adds no backoff at all on the
/// storage side (a `Failed` row with attempts left is re-fetched immediately),
/// so this stamp is applied by the worker's failure path
/// (`note_failure_backoff`).
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

/// The job kinds. Tick/SinkDrain ride the `dl-engine` queue; ColdExtract rides
/// `dl-cold` (its own worker, concurrency 1 = the single-flight cold worker).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum JobKind {
    /// Re-tick a root over a set of changed paths (`tick_paths`).
    #[serde(rename = "tick")]
    Tick,
    /// Drain a root's `@async`/external-sink effects (`poll_tick`).
    #[serde(rename = "sink_drain")]
    SinkDrain,
    /// Run ONE cold-start `(family, shard)` extraction node.
    #[serde(rename = "cold_extract")]
    ColdExtract,
}

impl JobKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            JobKind::Tick => "tick",
            JobKind::SinkDrain => "sink_drain",
            JobKind::ColdExtract => "cold_extract",
        }
    }

    /// The apalis queue (`Jobs.job_type`) this kind rides.
    pub(crate) fn queue(self) -> &'static str {
        match self {
            JobKind::Tick | JobKind::SinkDrain => ENGINE_QUEUE,
            JobKind::ColdExtract => COLD_QUEUE,
        }
    }
}

/// One unit of work: the task payload apalis stores in `Jobs.job` (JSON via
/// its codec) and hands the worker back. `key` is the dedup identity, carried
/// as the row's `idempotency_key`; `arg` coalesces across re-requests (Tick
/// unions its changed-path array).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct JobRow {
    pub key: String,
    pub kind: JobKind,
    pub root: String,
    pub arg: Value,
    pub priority: i64,
    /// See `crate::reqid`: the request in scope on the constructing thread,
    /// captured automatically. `None` on organically-triggered enqueues (file
    /// watcher, poll timer) — honest, those jobs have no causing request.
    pub req_id: Option<String>,
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
            req_id: crate::reqid::current(),
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
            req_id: crate::reqid::current(),
        }
    }

    /// A `cold:{root_id}:{family}:{shard}` staged-extraction job. One row per
    /// `_cold_node`, so a re-seed UPSERT is idempotent. `priority` orders the
    /// canonical extraction sequence WITHIN a root (apalis fetch is
    /// `priority DESC, run_at ASC, id ASC`); ACROSS roots the hold/promote
    /// admission in `enqueue`/`reconcile` serializes whole roots.
    pub fn cold_extract(root_id: &str, family: &str, shard: u32, priority: i64) -> JobRow {
        JobRow {
            key: format!("cold:{root_id}:{family}:{shard}"),
            kind: JobKind::ColdExtract,
            root: root_id.to_string(),
            arg: json!({ "family": family, "shard": shard }),
            priority,
            req_id: crate::reqid::current(),
        }
    }

    /// The changed paths carried by a Tick job's coalesced `arg`.
    pub fn paths(&self) -> Vec<PathBuf> {
        paths_of(&self.arg).into_iter().map(PathBuf::from).collect()
    }

    /// The `(family, shard)` a ColdExtract job carries in its `arg`.
    pub fn cold_target(&self) -> Option<(String, u32)> {
        let family = self.arg.get("family")?.as_str()?.to_string();
        let shard = self.arg.get("shard")?.as_u64()? as u32;
        Some((family, shard))
    }
}

/// What one `reconcile` pass did (all counts; zero = quiet pass).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReconcileReport {
    /// `Done` rows with a coalesced re-request (`dirty`) reopened `Pending`.
    pub dirty_reruns: usize,
    /// Held cold rows promoted (a new active cold root started).
    pub cold_promoted: usize,
    /// Ready cold rows deferred by the write-volume budget lever.
    pub budget_deferred: usize,
    /// Aged `Done` rows deleted (retention).
    pub trimmed: usize,
}

impl ReconcileReport {
    /// Did this pass make new work claimable (worth ringing the doorbell)?
    pub(crate) fn made_work(&self) -> bool {
        self.dirty_reruns > 0 || self.cold_promoted > 0
    }
}

/// The admission/introspection seam over the apalis `Jobs` table. One per
/// daemon process. All mutating ops serialize on the `Db` mutex; the
/// read-only `list` opens its own connection and never takes it. The apalis
/// workers reach the same file through their own sqlx pool.
pub(crate) struct JobQueue {
    db_path: PathBuf,
    db: Mutex<Db>,
    /// Keys whose in-flight (queued/running) job a `cancel_req` flagged; the
    /// worker checks `take_cancel` at the next job boundary and aborts.
    cancel_flags: Mutex<HashSet<String>>,
    /// The cold-extract write-volume budget lever; `None` = lever off.
    budget: Option<Arc<WriteBudget>>,
}

impl JobQueue {
    /// Open the admission connection on `<home>/jobs.sqlite`. The apalis
    /// migrations must already have run (`workers::open_pool` at daemon boot;
    /// tests run them through the same path) — this fails loudly if the
    /// `Jobs` table is missing rather than shipping a parallel schema.
    pub(crate) fn open(home: &Path) -> Result<Arc<JobQueue>> {
        Self::open_with_budget(home, WriteBudget::from_env())
    }

    /// `open` with an explicit budget lever (tests inject one; the daemon
    /// path reads `WriteBudget::from_env`).
    pub(crate) fn open_with_budget(
        home: &Path,
        budget: Option<Arc<WriteBudget>>,
    ) -> Result<Arc<JobQueue>> {
        let db_path = home.join("jobs.sqlite");
        let db = crate::db::open(Some(&db_path.to_string_lossy()))?;
        let has_jobs: Option<String> = db.query_opt(
            "Jobs",
            "SELECT name FROM sqlite_master WHERE type='table' AND name='Jobs'",
            &[],
            |r| Ok(r.get::<_, String>(0)?),
        )?;
        if has_jobs.is_none() {
            anyhow::bail!(
                "jobs.sqlite at {} has no `Jobs` table — apalis migrations \
                 (jobq::workers::open_pool) must run before JobQueue::open",
                db_path.display()
            );
        }
        Ok(Arc::new(JobQueue {
            db_path,
            db: Mutex::new(db),
            cancel_flags: Mutex::new(HashSet::new()),
            budget,
        }))
    }

    /// UPSERT a job into the apalis `Jobs` table. Coalescing IS the upsert:
    ///   - a re-request while `Pending` collapses onto the row (arg union,
    ///     priority max, attempts reset, ready now);
    ///   - a re-request while `Queued`/`Running` sets the `dirty` metadata
    ///     flag so `reconcile` reopens the row once the current execution
    ///     acks (arg union carries the new paths into that rerun);
    ///   - a re-request onto a `Done`/`Failed`/`Killed` row reopens it.
    ///
    /// ColdExtract admission: if another root already has live unheld cold
    /// work, this root's job is pushed HELD (`run_at` far future +
    /// `$.hold` metadata); `reconcile` promotes it when the active root has
    /// nothing runnable left. Enqueuing for the ACTIVE root also unholds any
    /// of its own held stragglers, keeping "one unheld live root" invariant.
    pub(crate) fn enqueue(&self, job: JobRow) -> Result<()> {
        let now = now_secs();
        let queue = job.kind.queue();
        let db = plock(&self.db);
        db.begin_immediate()?;
        let body = (|| -> Result<()> {
            let existing: Option<String> = db.query_opt(
                "Jobs",
                "SELECT CAST(job AS TEXT) FROM Jobs WHERE job_type=?1 AND idempotency_key=?2",
                &[queue.into(), job.key.clone().into()],
                |r| Ok(r.get::<_, String>(0)?),
            )?;
            let mut merged = job.clone();
            if let Some(prev) = &existing {
                if let Ok(prev_row) = serde_json::from_str::<JobRow>(prev) {
                    merged.arg = merge_args(&prev_row.arg, &job.arg);
                    merged.priority = prev_row.priority.max(job.priority);
                }
            }
            let held = job.kind == JobKind::ColdExtract && {
                let active = active_cold_root(&db)?;
                active.is_some_and(|r| r != job.root)
            };
            let run_at = if held { HOLD_RUN_AT } else { now };
            let mut meta = json!({
                "key": merged.key,
                "kind": merged.kind.as_str(),
                "root": merged.root,
                "req_id": merged.req_id,
                "enqueued_at": now,
            });
            if held {
                meta["hold"] = json!(1);
            }
            let payload = serde_json::to_string(&merged)?;
            let id = ulid::Ulid::new().to_string();
            db.exec_params(
                "Jobs",
                "INSERT INTO Jobs(job, id, job_type, status, attempts, max_attempts, run_at, \
                   last_result, lock_at, lock_by, done_at, priority, metadata, idempotency_key) \
                 VALUES(CAST(?1 AS BLOB), ?2, ?3, 'Pending', 0, ?4, ?5, \
                   NULL, NULL, NULL, NULL, ?6, ?7, ?8) \
                 ON CONFLICT(job_type, idempotency_key) DO UPDATE SET \
                   job = CAST(?1 AS BLOB), \
                   priority = max(priority, ?6), \
                   metadata = CASE WHEN status IN ('Queued','Running') \
                     THEN json_set(metadata, '$.dirty', 1) ELSE ?7 END, \
                   attempts = CASE WHEN status IN ('Queued','Running') THEN attempts ELSE 0 END, \
                   run_at = CASE WHEN status IN ('Queued','Running') THEN run_at ELSE ?5 END, \
                   lock_at = CASE WHEN status IN ('Queued','Running') THEN lock_at ELSE NULL END, \
                   lock_by = CASE WHEN status IN ('Queued','Running') THEN lock_by ELSE NULL END, \
                   done_at = CASE WHEN status IN ('Queued','Running') THEN done_at ELSE NULL END, \
                   last_result = CASE WHEN status IN ('Queued','Running') \
                     THEN last_result ELSE NULL END, \
                   status = CASE WHEN status IN ('Queued','Running') THEN status ELSE 'Pending' END",
                &[
                    payload.into(),
                    id.into(),
                    queue.into(),
                    MAX_ATTEMPTS.into(),
                    run_at.into(),
                    merged.priority.into(),
                    meta.to_string().into(),
                    merged.key.clone().into(),
                ],
            )?;
            // A fresh enqueue for the ACTIVE cold root unholds its own held
            // stragglers (keeps the one-unheld-live-root invariant simple).
            if job.kind == JobKind::ColdExtract && !held {
                db.exec_params(
                    "Jobs",
                    "UPDATE Jobs SET run_at=?2, metadata=json_remove(metadata, '$.hold') \
                     WHERE job_type=?1 AND json_extract(metadata, '$.hold')=1 \
                       AND json_extract(metadata, '$.root')=?3",
                    &[COLD_QUEUE.into(), now.into(), job.root.clone().into()],
                )?;
            }
            Ok(())
        })();
        finish_tx(&db, &body)?;
        drop(db);
        if body.is_ok() {
            // A re-enqueue clears any stale cancellation flag (the old queue's
            // `cancelled = 0` on upsert).
            plock(&self.cancel_flags).remove(&job.key);
        }
        body
    }

    /// The maintenance pass, run by the shell reconciler task (doorbell +
    /// 500ms backstop) and the idle loop. One write transaction, all set-ops:
    ///   1. dirty reruns — `Done` rows a re-request hit mid-run reopen
    ///      `Pending` (the coalesce promise the old `finish` kept inline);
    ///   2. cold promotion — when no unheld cold row is runnable, the
    ///      earliest-seeded held root is promoted (also the old
    ///      `active_cold_root` fall-through past a fully backed-off root);
    ///   3. budget deferral — the write-volume lever defers ready cold rows
    ///      to the next window when the current window's budget is spent;
    ///   4. retention — aged `Done` rows deleted.
    pub(crate) fn reconcile(&self) -> Result<ReconcileReport> {
        let now = now_secs();
        let db = plock(&self.db);
        db.begin_immediate()?;
        let body = (|| -> Result<ReconcileReport> {
            let dirty_reruns = db.exec_params(
                "Jobs",
                "UPDATE Jobs SET status='Pending', attempts=0, run_at=?1, lock_at=NULL, \
                   lock_by=NULL, done_at=NULL, last_result=NULL, \
                   metadata=json_remove(metadata, '$.dirty') \
                 WHERE status='Done' AND json_extract(metadata, '$.dirty')=1",
                &[now.into()],
            )?;
            // Cold promotion: nothing unheld is runnable (claimable now or
            // already in flight) -> promote the earliest-seeded held root
            // (ULID ids are time-ordered, so min(id) is seed order).
            let runnable_unheld: Option<i64> = db.query_opt(
                "Jobs",
                "SELECT 1 FROM Jobs WHERE job_type=?1 \
                   AND json_extract(metadata, '$.hold') IS NULL \
                   AND (status IN ('Queued','Running') \
                        OR (status='Pending' AND run_at<=?2) \
                        OR (status='Failed' AND attempts<max_attempts AND run_at<=?2)) \
                 LIMIT 1",
                &[COLD_QUEUE.into(), now.into()],
                |r| Ok(r.get::<_, i64>(0)?),
            )?;
            let mut cold_promoted = 0usize;
            if runnable_unheld.is_none() {
                let next_root: Option<String> = db.query_opt(
                    "Jobs",
                    "SELECT json_extract(metadata, '$.root') AS held_root FROM Jobs \
                     WHERE job_type=?1 AND json_extract(metadata, '$.hold')=1 \
                       AND status='Pending' \
                     GROUP BY held_root ORDER BY min(id) LIMIT 1",
                    &[COLD_QUEUE.into()],
                    |r| Ok(r.get::<_, String>(0)?),
                )?;
                if let Some(root) = next_root {
                    cold_promoted = db.exec_params(
                        "Jobs",
                        "UPDATE Jobs SET run_at=?2, metadata=json_remove(metadata, '$.hold') \
                         WHERE job_type=?1 AND json_extract(metadata, '$.hold')=1 \
                           AND json_extract(metadata, '$.root')=?3",
                        &[COLD_QUEUE.into(), now.into(), root.into()],
                    )?;
                }
            }
            let mut budget_deferred = 0usize;
            if let Some(budget) = &self.budget {
                if let Some(resume_at) = budget.deferral_until(now) {
                    budget_deferred = db.exec_params(
                        "Jobs",
                        "UPDATE Jobs SET run_at=?2 WHERE job_type=?1 AND status='Pending' \
                           AND run_at<=?3 AND json_extract(metadata, '$.hold') IS NULL",
                        &[COLD_QUEUE.into(), resume_at.into(), now.into()],
                    )?;
                }
            }
            let trimmed = db.exec_params(
                "Jobs",
                "DELETE FROM Jobs WHERE status='Done' AND done_at IS NOT NULL AND done_at<?1 \
                   AND json_extract(metadata, '$.dirty') IS NULL",
                &[(now - DONE_RETAIN_SECS).into()],
            )?;
            Ok(ReconcileReport { dirty_reruns, cold_promoted, budget_deferred, trimmed })
        })();
        finish_tx(&db, &body)?;
        body
    }

    /// Stamp the jittered backoff `run_at` for a just-failed attempt. Called
    /// by the worker's failure path BEFORE apalis acks the row `Failed` (the
    /// ack writes status/attempts/last_result, never `run_at`, so the stamp
    /// survives). Returns the delay in seconds.
    pub(crate) fn note_failure_backoff(&self, task_id: &str, key: &str, attempts: i64) -> Result<i64> {
        let now = now_secs();
        let delay = jittered_backoff(attempts, jitter_seed(key, now));
        let db = plock(&self.db);
        db.exec_params(
            "Jobs",
            "UPDATE Jobs SET run_at=?2 WHERE id=?1",
            &[task_id.into(), (now + delay).into()],
        )?;
        Ok(delay)
    }

    /// The cold-extract write-volume lever, if armed.
    pub(crate) fn write_budget(&self) -> Option<&Arc<WriteBudget>> {
        self.budget.as_ref()
    }

    /// Cancel every job a client request caused (class-18 residual: a
    /// disconnected client's queued/running work). Two halves:
    ///   - `Pending` rows are killed outright (`Killed`, never fetched);
    ///   - `Queued`/`Running` rows get a durable `$.cancel` metadata mark and
    ///     an in-process flag the worker consults at the next job boundary
    ///     (`take_cancel` -> `AbortError` -> `Killed`, no retry burn).
    ///
    /// Returns `(killed_pending, flagged_in_flight)`.
    pub(crate) fn cancel_req(&self, req_id: &str) -> Result<(usize, usize)> {
        let now = now_secs();
        let db = plock(&self.db);
        db.begin_immediate()?;
        let body = (|| -> Result<(usize, Vec<String>)> {
            let killed = db.exec_params(
                "Jobs",
                "UPDATE Jobs SET status='Killed', done_at=?2, \
                   last_result='{\"Err\":\"cancelled: client request gone\"}' \
                 WHERE json_extract(metadata, '$.req_id')=?1 AND status='Pending'",
                &[req_id.into(), now.into()],
            )?;
            let in_flight: Vec<String> = db.query_rows(
                "Jobs",
                "SELECT idempotency_key FROM Jobs \
                 WHERE json_extract(metadata, '$.req_id')=?1 AND status IN ('Queued','Running') \
                   AND idempotency_key IS NOT NULL",
                &[req_id.into()],
                |r| Ok(r.get::<_, String>(0)?),
            )?;
            if !in_flight.is_empty() {
                db.exec_params(
                    "Jobs",
                    "UPDATE Jobs SET metadata=json_set(metadata, '$.cancel', 1) \
                     WHERE json_extract(metadata, '$.req_id')=?1 \
                       AND status IN ('Queued','Running')",
                    &[req_id.into()],
                )?;
            }
            Ok((killed, in_flight))
        })();
        finish_tx(&db, &body)?;
        drop(db);
        let (killed, in_flight) = body?;
        let flagged = in_flight.len();
        if flagged > 0 {
            let mut flags = plock(&self.cancel_flags);
            for key in in_flight {
                flags.insert(key);
            }
        }
        if killed > 0 || flagged > 0 {
            tracing::info!(req_id, killed, flagged, "[jobq] cancelled request's jobs");
        }
        Ok((killed, flagged))
    }

    /// Consume a cancellation flag for `key` (worker job-boundary check).
    pub(crate) fn take_cancel(&self, key: &str) -> bool {
        plock(&self.cancel_flags).remove(key)
    }

    /// Non-consuming peek at `key`'s cancellation flag — the mid-run probe
    /// (`crate::cancel`) the worker installs around each job so a RUNNING
    /// tick's checkpoints observe the flag; `take_cancel` still consumes it
    /// at the job boundary / abort ack.
    pub(crate) fn is_cancelled(&self, key: &str) -> bool {
        plock(&self.cancel_flags).contains(key)
    }

    /// Boot crash recovery, run before the workers spawn: any row left
    /// `Queued`/`Running` by a previous (crashed/killed) process is reset to
    /// `Pending` so a worker picks it up again — instantly, without waiting
    /// for apalis's heartbeat-based `reenqueue_orphaned` window (which still
    /// runs live and covers mid-life orphans). Returns the count reset.
    pub(crate) fn reset_orphaned_on_boot(&self) -> Result<usize> {
        let db = plock(&self.db);
        let n = db.exec_on(
            "Jobs",
            "UPDATE Jobs SET status='Pending', lock_at=NULL, lock_by=NULL, done_at=NULL \
             WHERE status IN ('Queued','Running')",
        )?;
        Ok(n)
    }

    /// Every job, newest first, off a read-only connection — the lock-free
    /// `dl daemon jobs` read (never the engine lock, never the queue mutex).
    /// States are apalis's, lowercased for the CLI surface (`pending`,
    /// `queued`, `running`, `done`, `failed`, `killed`).
    pub(crate) fn list(&self) -> Result<Vec<JobListRow>> {
        let db = crate::db::ReadDb::open(&self.db_path.to_string_lossy())?;
        let rows = db.query_rows(
            "Jobs",
            "SELECT COALESCE(idempotency_key, id), \
                    COALESCE(json_extract(metadata, '$.kind'), job_type), \
                    COALESCE(json_extract(metadata, '$.root'), ''), \
                    lower(status), priority, attempts, run_at, \
                    json_extract(metadata, '$.enqueued_at'), lock_at, done_at, \
                    json_extract(last_result, '$.Err') \
             FROM Jobs ORDER BY json_extract(metadata, '$.enqueued_at') DESC, id DESC",
            &[],
            |r| {
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
            },
        )?;
        Ok(rows)
    }

    /// Test-only raw row peek by key:
    /// `(status, dirty, payload_json, priority, attempts, run_at)`.
    #[cfg(test)]
    pub(crate) fn peek(&self, key: &str) -> Option<(String, i64, String, i64, i64, i64)> {
        let db = plock(&self.db);
        db.query_opt(
            "Jobs",
            "SELECT status, COALESCE(json_extract(metadata, '$.dirty'), 0), \
                    CAST(job AS TEXT), priority, attempts, run_at \
             FROM Jobs WHERE idempotency_key=?1",
            &[key.into()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .ok()
        .flatten()
    }

    /// Test-only direct SQL poke on the admission connection.
    #[cfg(test)]
    pub(crate) fn poke(&self, sql: &str, params: &[crate::db::SqlVal]) -> Result<usize> {
        let db = plock(&self.db);
        db.exec_params("Jobs", sql, params)
    }
}

/// The one root with live UNHELD cold work (`Pending`/`Queued`/`Running`, or
/// `Failed` with attempts left). The enqueue-side half of root serialization:
/// a job for any OTHER root is pushed held. At most one such root exists by
/// construction (enqueue holds newcomers, `reconcile` promotes one root).
fn active_cold_root(db: &Db) -> Result<Option<String>> {
    db.query_opt(
        "Jobs",
        "SELECT json_extract(metadata, '$.root') FROM Jobs WHERE job_type=?1 \
           AND json_extract(metadata, '$.hold') IS NULL \
           AND (status IN ('Pending','Queued','Running') \
                OR (status='Failed' AND attempts<max_attempts)) \
         LIMIT 1",
        &[COLD_QUEUE.into()],
        |r| Ok(r.get::<_, String>(0)?),
    )
}

/// Commit on `Ok`, roll back on `Err` — keeps a failed mid-transaction op from
/// leaving the shared connection stuck inside a transaction.
fn finish_tx<T>(db: &Db, body: &Result<T>) -> Result<()> {
    match body {
        Ok(_) => db.commit()?,
        Err(_) => {
            let _ = db.rollback();
        }
    }
    Ok(())
}

/// Union a Tick job's `paths` array across a coalescing re-request, order-
/// preserving and deduped. Non-path args (SinkDrain's `{}`, ColdExtract's
/// `{family, shard}`) pass the incoming value through.
fn merge_args(prev: &Value, incoming: &Value) -> Value {
    let mut paths = paths_of(prev);
    for p in paths_of(incoming) {
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    if paths.is_empty() {
        incoming.clone()
    } else {
        json!({ "paths": paths })
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

/// Test-only open that first runs the apalis migrations (the daemon does this
/// through `workers::open_pool` at boot) on a throwaway current-thread
/// runtime, then opens the sync admission seam.
#[cfg(test)]
pub(crate) fn test_open(home: &Path) -> Arc<JobQueue> {
    test_open_with_budget(home, None)
}

#[cfg(test)]
pub(crate) fn test_open_with_budget(
    home: &Path,
    budget: Option<Arc<WriteBudget>>,
) -> Arc<JobQueue> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    let pool = rt
        .block_on(workers::open_pool(&home.join("jobs.sqlite")))
        .expect("apalis migrations");
    rt.block_on(async { pool.close().await });
    JobQueue::open_with_budget(home, budget).expect("open job queue")
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_cancel;
