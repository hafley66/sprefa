//! Cold-extract write-volume budget lever (resource-aware scheduler plan,
//! steps 1-2 seam — the "write-volume budget lever" the standing ledger routes
//! through this arc). ONLY the lever: a per-window byte budget the queue's
//! admission layer (`JobQueue::reconcile`) enforces by deferring ready cold
//! jobs to the next window. The full scheduler (scope rows, readiness,
//! perf-fed per-shard costs — plan section 6.6 user amendment) replaces the
//! flat per-job estimate when it lands; the seam it needs (`admitted_spend` /
//! `deferral_until`) is what this module fixes.
//!
//! Accounting is estimate-based: a completed cold job records
//! `job_estimate_bytes` (default 512KiB — `COLD_CHUNK_TARGET_BYTES`, the
//! byte-bounded chunk size every sharded family drains in). Enforcement is
//! soft by design: a job fetched between the spend that exhausted the window
//! and the next `reconcile` pass still runs; everything ready after the pass
//! waits for the window to roll.
//!
//! Env surface:
//!   - `DL_COLD_WRITE_BUDGET_BYTES_PER_MIN` — bytes admitted per 60s window;
//!     unset/0 = lever off (today's default posture).
//!   - `DL_COLD_JOB_WRITE_EST_BYTES` — per-job estimate override (tests,
//!     tuning).

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Byte-bounded chunk target (`engine::cold_stage::COLD_CHUNK_TARGET_BYTES`):
/// the honest flat estimate for one cold job's write volume until perf-fed
/// per-(family, shard-bytes) costs ride in with the scheduler arc.
const DEFAULT_JOB_ESTIMATE_BYTES: u64 = 512 * 1024;

/// Window length. One minute: long enough to smooth chunk granularity, short
/// enough that a deferred cold start resumes promptly.
const WINDOW_SECS: i64 = 60;

#[derive(Debug)]
struct WindowState {
    /// Unix second the current window opened.
    window_start: i64,
    /// Estimated bytes admitted (recorded) in the current window.
    spent: u64,
}

/// The per-window write budget. Cheap to consult; all state behind one mutex.
#[derive(Debug)]
pub(crate) struct WriteBudget {
    /// Bytes admitted per window. Never 0 (0/unset = no lever at all).
    bytes_per_window: u64,
    /// Per-job spend estimate.
    job_estimate_bytes: u64,
    state: Mutex<WindowState>,
}

impl WriteBudget {
    /// Read the lever from the environment; `None` = lever off.
    pub(crate) fn from_env() -> Option<Arc<WriteBudget>> {
        let per_min: u64 = std::env::var("DL_COLD_WRITE_BUDGET_BYTES_PER_MIN")
            .ok()?
            .parse()
            .ok()?;
        if per_min == 0 {
            return None;
        }
        let estimate = std::env::var("DL_COLD_JOB_WRITE_EST_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|b| *b > 0)
            .unwrap_or(DEFAULT_JOB_ESTIMATE_BYTES);
        Some(Arc::new(WriteBudget::new(per_min, estimate)))
    }

    pub(crate) fn new(bytes_per_window: u64, job_estimate_bytes: u64) -> WriteBudget {
        WriteBudget {
            bytes_per_window: bytes_per_window.max(1),
            job_estimate_bytes: job_estimate_bytes.max(1),
            state: Mutex::new(WindowState {
                window_start: now_secs(),
                spent: 0,
            }),
        }
    }

    /// Record one completed cold job's estimated write volume.
    pub(crate) fn record_job(&self) {
        self.record_bytes(self.job_estimate_bytes);
    }

    /// Record an explicit spend (the scheduler arc's per-shard costs land
    /// here; today only `record_job` calls it).
    pub(crate) fn record_bytes(&self, bytes: u64) {
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        self.roll(&mut st, now_secs());
        st.spent = st.spent.saturating_add(bytes);
    }

    /// If the current window's budget is spent, the unix second ready cold
    /// jobs should be deferred to (the next window's open). `None` = budget
    /// available, admit freely.
    pub(crate) fn deferral_until(&self, now: i64) -> Option<i64> {
        let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
        self.roll(&mut st, now);
        if st.spent.saturating_add(self.job_estimate_bytes) > self.bytes_per_window {
            Some(st.window_start + WINDOW_SECS)
        } else {
            None
        }
    }

    fn roll(&self, st: &mut WindowState, now: i64) {
        if now >= st.window_start + WINDOW_SECS {
            st.window_start = now - ((now - st.window_start) % WINDOW_SECS);
            st.spent = 0;
        }
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_budget_admits_and_exhausted_budget_defers_to_next_window() {
        // Budget = 3 job-estimates per window.
        let budget = WriteBudget::new(3 * 100, 100);
        let now = now_secs();
        assert_eq!(budget.deferral_until(now), None, "fresh window admits");
        budget.record_job();
        budget.record_job();
        assert_eq!(
            budget.deferral_until(now),
            None,
            "two of three spent still admits"
        );
        budget.record_job();
        let deferred = budget.deferral_until(now);
        assert!(deferred.is_some(), "spent budget defers");
        let resume = deferred.unwrap();
        assert!(
            resume > now && resume <= now + WINDOW_SECS,
            "deferral lands at the next window boundary ({resume} vs now {now})"
        );
    }

    #[test]
    fn window_roll_resets_spend() {
        let budget = WriteBudget::new(100, 100);
        budget.record_job();
        {
            let mut st = budget.state.lock().unwrap();
            st.window_start -= 2 * WINDOW_SECS; // age the window artificially
        }
        assert_eq!(
            budget.deferral_until(now_secs()),
            None,
            "a rolled window starts with zero spend"
        );
    }

    #[test]
    fn from_env_off_when_unset_or_zero() {
        // Serialized by cargo's per-test process env being private enough
        // here: use unlikely var values around the calls.
        std::env::remove_var("DL_COLD_WRITE_BUDGET_BYTES_PER_MIN");
        assert!(WriteBudget::from_env().is_none());
        std::env::set_var("DL_COLD_WRITE_BUDGET_BYTES_PER_MIN", "0");
        assert!(WriteBudget::from_env().is_none());
        std::env::set_var("DL_COLD_WRITE_BUDGET_BYTES_PER_MIN", "1048576");
        let armed = WriteBudget::from_env().expect("armed lever");
        assert_eq!(armed.bytes_per_window, 1_048_576);
        std::env::remove_var("DL_COLD_WRITE_BUDGET_BYTES_PER_MIN");
    }
}
