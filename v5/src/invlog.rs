//! Global invocation log: one row per `dl` process, in a machine-global
//! SQLite db at `<daemon_home>/invocations.db`. This is the access-log twin of
//! `why.jsonl`'s self-diagnosis trail (`src/why.rs`): `why.jsonl` answers "what
//! was the DAEMON doing"; `invocation` answers "who ran `dl`, when, with what
//! args, and did it exit cleanly." An open row (`ts_end_ms IS NULL`) whose pid
//! is no longer alive is direct evidence of a kill — the same "boot with no
//! matching shutdown marker" idiom `why.rs` uses, one level up (process, not
//! daemon tick).
//!
//! Written by the incident that motivated this module: six daemons were
//! serially spawned and killed on one root in seven minutes by an external
//! agent's `dl --check` autostart, and nothing on disk recorded WHO spawned
//! them or that a one-shot client even ran. `dl daemon why` could say what the
//! dead daemon was doing; nothing could say who kept starting it.
//!
//! Best-effort everywhere: every fallible operation here swallows its error
//! and returns `None`/does nothing. A logging failure must never fail or slow
//! down the CLI — this module exists to explain an incident after the fact,
//! not to gate anything live. `DL_INVLOG=0` disables recording entirely.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

// A separate machine-global process log (invocations.db), not the engine's
// schema; `Db::open` would inject a "[db] opened..." verdict line and a
// lock-contention warning into every `dl` invocation's stderr — unacceptable
// for a module whose whole contract is best-effort silence.
// @rusqlite-ok: foreign-purpose db; the seam's side effects don't fit here.
use rusqlite::{params, Connection};

/// Rows older than this are swept on every `record_start` (cheap, bounded —
/// mirrors the retention policy on `jobq`'s `done` rows).
const RETENTION_DAYS: i64 = 14;
/// `ancestry` walks at most this many `ppid` hops (the doc'd depth<=5 cap).
const ANCESTRY_MAX_DEPTH: usize = 5;

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS invocation (
  id INTEGER PRIMARY KEY,
  ts_start_ms INTEGER NOT NULL,
  ts_end_ms INTEGER,
  pid INTEGER NOT NULL,
  ppid INTEGER NOT NULL,
  ancestry TEXT,
  argv TEXT NOT NULL,
  cwd TEXT,
  exe_mtime_ms INTEGER,
  build TEXT,
  exit_code INTEGER,
  cpu_ms INTEGER, io_read_mb REAL, io_write_mb REAL, rss_mb REAL
);
CREATE INDEX IF NOT EXISTS invocation_pid ON invocation(pid);
CREATE INDEX IF NOT EXISTS invocation_ts_start ON invocation(ts_start_ms);";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `DL_INVLOG=0` opts a process out of recording entirely.
fn enabled() -> bool {
    std::env::var("DL_INVLOG").ok().as_deref() != Some("0")
}

fn db_path() -> PathBuf {
    crate::daemon::daemon_home().join("invocations.db")
}

/// Open (creating) the invocation db with the constraints the spec calls for:
/// WAL so a daemon writer and a one-shot client reader never block each other,
/// `busy_timeout(5000)` so a burst of concurrent one-shot invocations queues
/// instead of erroring. `None` on any failure (best-effort).
fn open() -> Option<Connection> {
    let path = db_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = Connection::open(&path).ok()?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))
        .ok()?;
    conn.execute_batch("PRAGMA journal_mode=WAL;").ok()?;
    conn.execute_batch(SCHEMA).ok()?;
    Some(conn)
}

fn retain(conn: &Connection) {
    let cutoff = now_ms() - RETENTION_DAYS * 24 * 3600 * 1000;
    // @rusqlite-ok: best-effort retention sweep; a failed DELETE just means next call's sweep tries again.
    let _ = conn.execute(
        "DELETE FROM invocation WHERE ts_start_ms < ?1",
        params![cutoff],
    );
}

#[cfg(unix)]
fn parent_pid() -> u32 {
    // SAFETY: getppid() has no failure mode; it always returns a valid pid.
    unsafe { libc::getppid() as u32 }
}
#[cfg(not(unix))]
fn parent_pid() -> u32 {
    0
}

/// One `ps -eo pid=,ppid=,comm=` snapshot, then walk `ppid` links from
/// `start_ppid` up to `ANCESTRY_MAX_DEPTH` hops or pid 1/0 (init/none).
/// `cfg(unix)`; empty string on any failure (no `ps`, non-unix, parse error) —
/// this is diagnostic best-effort, never critical for anything else.
#[cfg(unix)]
fn ancestry_line(start_ppid: u32) -> String {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-eo", "pid=,ppid=,comm="])
        .output()
    else {
        return String::new();
    };
    if !out.status.success() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut table: HashMap<u32, (u32, String)> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        let mut parts = line.splitn(3, char::is_whitespace);
        let Some(pid_s) = parts.next() else { continue };
        let Some(rest) = parts.next() else { continue };
        let Ok(pid) = pid_s.parse::<u32>() else {
            continue;
        };
        let rest = rest.trim_start();
        let mut it2 = rest.splitn(2, char::is_whitespace);
        let Some(ppid_s) = it2.next() else { continue };
        let Ok(ppid) = ppid_s.parse::<u32>() else {
            continue;
        };
        let comm = it2.next().unwrap_or("").trim().to_string();
        table.insert(pid, (ppid, comm));
    }
    let mut chain: Vec<String> = Vec::new();
    let mut cur = start_ppid;
    for _ in 0..ANCESTRY_MAX_DEPTH {
        if cur == 0 {
            break;
        }
        let Some((ppid, comm)) = table.get(&cur) else {
            chain.push(format!("{cur}:?"));
            break;
        };
        chain.push(format!("{cur}:{comm}"));
        if cur == 1 || *ppid == cur {
            break;
        }
        cur = *ppid;
    }
    chain.join(" <- ")
}
#[cfg(not(unix))]
fn ancestry_line(_start_ppid: u32) -> String {
    String::new()
}

fn exe_mtime_ms() -> Option<i64> {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
}

/// Record the start of this process's invocation. Returns the row id to hand
/// back to `record_end`, or `None` if logging is disabled (`DL_INVLOG=0`) or
/// the write failed — in which case `record_end` is a no-op too.
pub fn record_start(argv: &[String]) -> Option<i64> {
    *CURRENT_ID.lock().unwrap() = None;
    if !enabled() {
        return None;
    }
    let conn = open()?;
    retain(&conn);
    let pid = std::process::id();
    let ppid = parent_pid();
    let ancestry = ancestry_line(ppid);
    let argv_json = serde_json::to_string(argv).unwrap_or_default();
    let cwd = std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    let build = crate::daemon::build_id();
    conn.execute(
        "INSERT INTO invocation(ts_start_ms, pid, ppid, ancestry, argv, cwd, exe_mtime_ms, build) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            now_ms(),
            pid,
            ppid,
            ancestry,
            argv_json,
            cwd,
            exe_mtime_ms(),
            build
        ],
    )
    .ok()?;
    let id = conn.last_insert_rowid();
    *CURRENT_ID.lock().unwrap() = Some(id);
    Some(id)
}

/// The current process's open invocation row id, for exit paths that
/// `std::process::exit` before `cli::run`'s own `record_end` (the `--hook`
/// dispatch arm). Set by `record_start`, taken by `record_end_current`.
static CURRENT_ID: Mutex<Option<i64>> = Mutex::new(None);

/// Close the invocation row from an exit path that bypasses `cli::run`'s
/// `record_end` (a `std::process::exit` inside a dispatch arm). `take`s the
/// stored id, so a second close attempt is a no-op; the row UPDATE itself is
/// idempotent by id regardless.
pub fn record_end_current(exit_code: i32) {
    let id = CURRENT_ID
        .lock()
        .ok()
        .and_then(|mut current| current.take());
    record_end(id, exit_code);
}

/// Finalize the row `record_start` opened: end timestamp, exit code, and the
/// process's cumulative CPU/IO/RSS counters (same `read_self_usage` reader
/// `why.jsonl` samples use). No-op if `id` is `None` (disabled, or the start
/// write already failed) or this write fails.
pub fn record_end(id: Option<i64>, exit_code: i32) {
    let Some(id) = id else { return };
    if !enabled() {
        return;
    }
    let Some(conn) = open() else { return };
    let u = crate::why::read_self_usage();
    // @rusqlite-ok: best-effort finalize; a failed UPDATE just leaves the row open (reported KILLED/RUNNING-forever).
    let _ = conn.execute(
        "UPDATE invocation SET ts_end_ms=?2, exit_code=?3, cpu_ms=?4, io_read_mb=?5, \
         io_write_mb=?6, rss_mb=?7 WHERE id=?1",
        params![
            id,
            now_ms(),
            exit_code,
            u.cpu_ms as i64,
            crate::why::mb(u.io_read_bytes),
            crate::why::mb(u.io_write_bytes),
            crate::why::mb(u.rss_bytes),
        ],
    );
}

/// One row as read back for `dl daemon invocations` / the `why` join.
#[derive(Debug, Clone)]
pub struct InvocationRow {
    pub id: i64,
    pub ts_start_ms: i64,
    pub ts_end_ms: Option<i64>,
    pub pid: i64,
    pub ppid: i64,
    pub ancestry: Option<String>,
    pub argv: String,
    pub cwd: Option<String>,
    pub exit_code: Option<i64>,
}

const SELECT_COLS: &str = "id, ts_start_ms, ts_end_ms, pid, ppid, ancestry, argv, cwd, exit_code";

fn row_from(r: &crate::db::SqlRow) -> crate::db::SqlRowResult<InvocationRow> {
    Ok(InvocationRow {
        id: r.get(0)?,
        ts_start_ms: r.get(1)?,
        ts_end_ms: r.get(2)?,
        pid: r.get(3)?,
        ppid: r.get(4)?,
        ancestry: r.get(5)?,
        argv: r.get(6)?,
        cwd: r.get(7)?,
        exit_code: r.get(8)?,
    })
}

/// The most recent rows, newest first — `dl daemon invocations [--limit N]`.
pub fn recent(limit: usize) -> Vec<InvocationRow> {
    let Some(conn) = open() else {
        return Vec::new();
    };
    let sql = format!("SELECT {SELECT_COLS} FROM invocation ORDER BY ts_start_ms DESC LIMIT ?1");
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map(params![limit as i64], row_from) else {
        return Vec::new();
    };
    rows.filter_map(|r| r.ok()).collect()
}

/// The most recent invocation row for `pid` (prefers a still-open row, since
/// that is the one `why`'s dead-daemon join cares about). `None` if invlog is
/// disabled, unreachable, or has never seen this pid.
pub fn lookup_by_pid(pid: u32) -> Option<InvocationRow> {
    let conn = open()?;
    let sql = format!(
        "SELECT {SELECT_COLS} FROM invocation WHERE pid=?1 \
         ORDER BY (ts_end_ms IS NULL) DESC, ts_start_ms DESC LIMIT 1"
    );
    conn.query_row(&sql, params![pid], row_from).ok()
}

fn fmt_ts_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let now = now_ms() / 1000;
    let ago = now - secs;
    match ago {
        a if a < 0 => "in the future(?)".into(),
        a if a < 120 => format!("{a}s ago"),
        a if a < 7200 => format!("{}m ago", a / 60),
        a => format!("{}h ago", a / 3600),
    }
}

/// Render `dl daemon invocations`: recent rows newest first, an open row
/// flagged `RUNNING` (pid alive) or `KILLED` (pid dead — the kill evidence).
pub fn report_recent(limit: usize) -> String {
    let rows = recent(limit);
    if rows.is_empty() {
        return format!(
            "no invocations recorded at {} (DL_INVLOG=0, or none run yet)\n",
            db_path().display()
        );
    }
    let mut out = String::new();
    for r in rows {
        let state = match r.ts_end_ms {
            Some(_) => format!(
                "exit={}",
                r.exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "?".into())
            ),
            None if crate::why::pid_alive(r.pid as u32) => "RUNNING".to_string(),
            None => "KILLED".to_string(),
        };
        let argv: Vec<String> = serde_json::from_str(&r.argv).unwrap_or_default();
        out += &format!(
            "{} pid={} {state} argv={} cwd={}\n",
            fmt_ts_ms(r.ts_start_ms),
            r.pid,
            argv.join(" "),
            r.cwd.as_deref().unwrap_or(""),
        );
    }
    out
}

/// The "spawned by ..." line `why`'s dead-pid report appends when it can join
/// the pid against an invocation row: ancestry chain + the exact argv that
/// spawned it. Empty string if no row is found (invlog was off, or this pid
/// predates the invlog arc).
pub fn spawned_by_line(pid: u32) -> String {
    let Some(row) = lookup_by_pid(pid) else {
        return String::new();
    };
    let argv: Vec<String> = serde_json::from_str(&row.argv).unwrap_or_default();
    let ancestry = row
        .ancestry
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "?".into());
    format!("spawned by {ancestry} | argv: {}\n", argv.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Unlike `tests/it/*` (which sandbox `XDG_STATE_HOME` on a spawned CHILD
    // process's env — hermetic for free, one process per test), these are
    // in-process unit tests calling `record_start`/`record_end` directly, so
    // they must mutate THIS test binary's own process-global env vars.
    // `cargo test --lib` runs tests in parallel by default; without
    // serializing among themselves, two of these tests interleaving their
    // `set_var("XDG_STATE_HOME", ...)` calls would race (test A's writes
    // landing in test B's sandbox). No other module's unit tests touch
    // `XDG_STATE_HOME`, so this lock only needs to cover invlog's own tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn sandbox(tag: &str) -> (PathBuf, std::sync::MutexGuard<'static, ()>) {
        let guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = std::env::temp_dir().join(format!("dl_invlog_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // SAFETY: serialized by ENV_LOCK above, so no other test in this
        // module observes a torn env value while this guard is held.
        unsafe { std::env::set_var("XDG_STATE_HOME", &dir) };
        (dir, guard)
    }

    #[test]
    fn record_start_then_end_round_trips() {
        let (_dir, _guard) = sandbox("roundtrip");
        unsafe { std::env::remove_var("DL_INVLOG") };
        let argv = vec!["dl".to_string(), "p.dl".to_string()];
        let id = record_start(&argv).expect("record_start should succeed in a writable sandbox");
        record_end(Some(id), 0);

        let row = recent(10)
            .into_iter()
            .find(|r| r.id == id)
            .expect("row present");
        assert_eq!(row.exit_code, Some(0));
        assert!(row.ts_end_ms.is_some(), "ts_end_ms set after record_end");
        assert_eq!(row.pid, std::process::id() as i64);
        let parsed_argv: Vec<String> = serde_json::from_str(&row.argv).unwrap();
        assert_eq!(parsed_argv, argv);
    }

    #[test]
    fn dl_invlog_0_disables_recording() {
        let (_dir, _guard) = sandbox("disabled");
        unsafe { std::env::set_var("DL_INVLOG", "0") };
        let before = recent(1000).len();
        let id = record_start(&["dl".to_string()]);
        assert!(
            id.is_none(),
            "record_start must return None when DL_INVLOG=0"
        );
        assert_eq!(recent(1000).len(), before, "no row inserted when disabled");
        unsafe { std::env::remove_var("DL_INVLOG") };
    }

    #[test]
    fn open_row_with_dead_pid_reports_killed() {
        let (_dir, _guard) = sandbox("killed");
        unsafe { std::env::remove_var("DL_INVLOG") };
        let conn = open().unwrap();
        let dead_pid = 4_000_000i64; // beyond real pid ranges
        conn.execute(
            "INSERT INTO invocation(ts_start_ms, pid, ppid, ancestry, argv, cwd) \
             VALUES (?1, ?2, 1, 'x', ?3, '/tmp')",
            params![now_ms(), dead_pid, r#"["dl","--watch"]"#],
        )
        .unwrap();
        let report = report_recent(10);
        assert!(
            report.contains("KILLED"),
            "dead open row must be flagged KILLED:\n{report}"
        );
    }
}
