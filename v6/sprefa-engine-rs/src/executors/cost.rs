//! @comment-ok: the executor's contract, the one doc site for its columns.
//! `dl_tick_cost(bucket)` answers what the PREVIOUS interval of this process
//! cost: one row per trace label plus one `wall` row, columns
//! (bucket, executor, wall_ms, sqlite_ms, calls, rows_out, bytes, rss_kb).
//! Every number is a delta since the last call, so a program sampling once a
//! tick reads that tick rather than the fold's running total. `bytes` is what
//! `pulls.rs` moved over the wire; a 304 adds zero.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::hosts::{HostError, IHostExecutor};
use crate::trace::{summary_rows, Label, Stat};
use crate::types::HostRow;

use super::{required_input, row};

static WIRE_BYTES: AtomicU64 = AtomicU64::new(0);
static REQUESTS: AtomicU64 = AtomicU64::new(0);
static NOT_MODIFIED: AtomicU64 = AtomicU64::new(0);

/// Called by every conditional GET a watched program makes. A 304 carries no
/// body, so it moves the request count and leaves the byte count alone.
pub fn count_request(bytes: u64, not_modified: bool) {
    WIRE_BYTES.fetch_add(bytes, Ordering::Relaxed);
    REQUESTS.fetch_add(1, Ordering::Relaxed);
    if not_modified {
        NOT_MODIFIED.fetch_add(1, Ordering::Relaxed);
    }
}

/// The counter readings the previous call consumed. Subtracting them is what
/// makes a row a per-tick cost rather than a lifetime total.
#[derive(Default)]
struct Consumed {
    labels: BTreeMap<String, Stat>,
    bytes: u64,
    requests: u64,
    not_modified: u64,
}

static CONSUMED: LazyLock<Mutex<Consumed>> = LazyLock::new(|| Mutex::new(Consumed::default()));

/// One `System` for the process's life: constructing one per call re-reads the
/// whole process table, which is the cost this rel exists to keep small.
static PROBE: LazyLock<Mutex<System>> = LazyLock::new(|| Mutex::new(System::new()));

/// Resident set of THIS process in kibibytes. `sysinfo` is already in the lock
/// through soopy, so no new crate reaches the tree for one number.
pub fn resident_kb() -> u64 {
    let pid = Pid::from_u32(std::process::id());
    let mut probe = PROBE.lock().expect("the rss probe");
    probe.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    probe.process(pid).map(|found| found.memory() / 1024).unwrap_or(0)
}

pub struct TickCostExecutor;

impl IHostExecutor for TickCostExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let bucket = required_input(host, env, &["bucket", "at_bucket", "tick"])?
            .parse::<i64>()
            .unwrap_or(0);
        Ok(rows_for(bucket))
    }
}

/// The trace table only records under `DL_TRACE_SUMMARY` or `trace::force_summary`,
/// so without either the per-label rows are empty and the `wall` row still answers.
fn rows_for(bucket: i64) -> Vec<HostRow> {
    let mut consumed = CONSUMED.lock().expect("the cost baseline");

    let bytes = WIRE_BYTES.load(Ordering::Relaxed);
    let requests = REQUESTS.load(Ordering::Relaxed);
    let not_modified = NOT_MODIFIED.load(Ordering::Relaxed);
    let interval_bytes = bytes.saturating_sub(consumed.bytes);
    let interval_requests = requests.saturating_sub(consumed.requests);
    let interval_304 = not_modified.saturating_sub(consumed.not_modified);
    consumed.bytes = bytes;
    consumed.requests = requests;
    consumed.not_modified = not_modified;

    let rss_kb = resident_kb();
    let mut rows = Vec::new();
    let mut wall_nanos = 0u64;
    let mut sqlite_nanos = 0u64;

    for (label, stat) in summary_rows() {
        let key = label_key(&label);
        let previous = consumed.labels.get(&key).copied().unwrap_or_default();
        let step = subtract(&stat, &previous);
        consumed.labels.insert(key.clone(), stat);
        wall_nanos += step.wall_nanos;
        sqlite_nanos += step.nanos;
        if step.calls == 0 && step.wall_nanos == 0 {
            continue;
        }
        rows.push(cost_row(bucket, &key, &step, 0, rss_kb));
    }

    let total = Stat {
        calls: interval_requests,
        rows_out: interval_304,
        rows_changed: 0,
        nanos: sqlite_nanos,
        wall_nanos: wall_nanos.max(sqlite_nanos),
        scopes: 0,
    };
    rows.push(cost_row(bucket, "wall", &total, interval_bytes, rss_kb));
    rows
}

/// The label as a program reads it: the verb alone for a phase, `verb/relation`
/// where the seam attributed a statement to a named rel.
fn label_key(label: &Label) -> String {
    if label.relation.as_ref() == "-" {
        return label.verb.to_string();
    }
    format!("{}/{}", label.verb, label.relation)
}

fn subtract(now: &Stat, before: &Stat) -> Stat {
    Stat {
        calls: now.calls.saturating_sub(before.calls),
        rows_out: now.rows_out.saturating_sub(before.rows_out),
        rows_changed: now.rows_changed.saturating_sub(before.rows_changed),
        nanos: now.nanos.saturating_sub(before.nanos),
        wall_nanos: now.wall_nanos.saturating_sub(before.wall_nanos),
        scopes: now.scopes.saturating_sub(before.scopes),
    }
}

fn cost_row(bucket: i64, executor: &str, stat: &Stat, bytes: u64, rss_kb: u64) -> HostRow {
    row([
        ("bucket", serde_json::json!(bucket)),
        ("executor", serde_json::json!(executor)),
        ("wall_ms", serde_json::json!((stat.wall_nanos / 1_000_000) as i64)),
        ("sqlite_ms", serde_json::json!((stat.nanos / 1_000_000) as i64)),
        ("calls", serde_json::json!(stat.calls as i64)),
        ("rows_out", serde_json::json!(stat.rows_out as i64)),
        ("bytes", serde_json::json!(bytes as i64)),
        ("rss_kb", serde_json::json!(rss_kb as i64)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// @comment-ok: TEST header carrying the fail-first receipt.
    /// The counters are deltas. Fail-pre-fix: reading the atomics without the
    /// consumed baseline answered the lifetime total on every tick, so a `bytes`
    /// series over 30 ticks only ever climbed.
    #[test]
    fn a_second_read_answers_only_what_moved_since_the_first() {
        let _ = rows_for(0);
        count_request(4096, false);
        let first = rows_for(1);
        let wall = wall_row(&first);
        assert_eq!(wall.get("bytes"), Some(&serde_json::json!(4096)));

        count_request(0, true);
        let second = rows_for(2);
        let wall = wall_row(&second);
        assert_eq!(wall.get("bytes"), Some(&serde_json::json!(0)));
        assert_eq!(wall.get("calls"), Some(&serde_json::json!(1)));
        assert_eq!(wall.get("rows_out"), Some(&serde_json::json!(1)));
    }

    #[test]
    fn resident_set_is_a_real_reading() {
        assert!(resident_kb() > 0, "this process has a resident set");
    }

    fn wall_row(rows: &[HostRow]) -> &HostRow {
        rows.iter()
            .find(|row| row.get("executor") == Some(&serde_json::json!("wall")))
            .expect("the wall row is always answered")
    }
}
