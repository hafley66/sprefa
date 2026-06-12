use anyhow::Result;
use rusqlite::Connection;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::ast::Value;

/// Profile mode: `--profile` or `DL_PROFILE=1`. When on, every SQL statement
/// over the slow threshold logs with its wall time (via SQLite's profile hook,
/// so the `.conn()` escape hatches are covered too), each repo×rev scan logs
/// files/ms, and the tick prints a phase + statement-count breakdown.
static PROFILE: OnceLock<bool> = OnceLock::new();

/// Force profile mode on (the `--profile` flag). Call before the first
/// `profiling()` read; after that the mode is fixed for the process.
pub fn set_profile(on: bool) {
    if on { let _ = PROFILE.set(true); }
}

pub fn profiling() -> bool {
    *PROFILE.get_or_init(|| std::env::var("DL_PROFILE").is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// Slow-statement threshold for the profile log. `DL_PROFILE_SQL_MS` overrides
/// (default 25ms); statements under it still count into the tick aggregate.
fn slow_sql() -> Duration {
    static MS: OnceLock<u64> = OnceLock::new();
    Duration::from_millis(*MS.get_or_init(|| std::env::var("DL_PROFILE_SQL_MS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(25)))
}

static SQL_COUNT: AtomicU64 = AtomicU64::new(0);
static SQL_NANOS: AtomicU64 = AtomicU64::new(0);

/// Take-and-reset the per-tick SQL aggregate fed by the profile hook:
/// (statements executed, total wall ms inside SQLite).
pub fn sql_stats_take() -> (u64, f64) {
    let n = SQL_COUNT.swap(0, Ordering::Relaxed);
    let ms = SQL_NANOS.swap(0, Ordering::Relaxed) as f64 / 1e6;
    (n, ms)
}

/// SQLite profile hook (fn pointer, no closure state): aggregate everything,
/// log statements over the slow threshold with compacted SQL.
fn profile_hook(sql: &str, dur: Duration) {
    SQL_COUNT.fetch_add(1, Ordering::Relaxed);
    SQL_NANOS.fetch_add(dur.as_nanos() as u64, Ordering::Relaxed);
    if dur >= slow_sql() {
        let compact: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        let head: String = compact.chars().take(160).collect();
        let ellipsis = if compact.chars().count() > 160 { "…" } else { "" };
        eprintln!("[sql {:.1}ms] {head}{ellipsis}", dur.as_secs_f64() * 1000.0);
    }
}

/// The single owner of the SQLite `Connection` — the one place SQL is issued, so
/// the backend stays swappable and every interaction is counted. Methods are
/// PLURAL by construction: each issues one logical statement regardless of N
/// (chunked multi-row `VALUES`), so a per-row write cannot hide behind this
/// boundary. A per-tick statement counter screams when one statement runs more
/// than `N1_THRESHOLD` times in a tick — the runtime N+1 detector that replaces
/// the (unworkable) static sniff.
///
/// `conn()` is the migration escape hatch: call sites still on the raw
/// `Connection` bypass the counter; their number (grep `.conn()`) is the
/// remaining SQL-seam debt to burn down.
pub struct Db {
    conn: Connection,
    counts: RefCell<HashMap<String, u32>>,
}

/// One statement running more than this many times in a tick is almost certainly
/// a per-row loop that escaped the plural API. Chunked batches stay well under it
/// (a 5k-row insert is ~20 chunks).
const N1_THRESHOLD: u32 = 64;

/// Rows per multi-row `VALUES` statement. 256 * (cols ≤ ~8) stays under SQLite's
/// bound-parameter limit with margin.
const CHUNK: usize = 256;

pub fn open(path: Option<&str>) -> Result<Db> {
    let mut conn = match path {
        Some(p) => Connection::open(p)?,
        None => Connection::open_in_memory()?,
    };
    // busy_timeout: a hook `--check` and a resident `--lsp` share .dl/cache.db
    // across processes; a write collision should wait, not fail "locked".
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;")?;
    register_regexp(&conn)?;
    if profiling() { conn.profile(Some(profile_hook)); }
    Ok(Db { conn, counts: RefCell::new(HashMap::new()) })
}

/// Register the `regexp(pattern, value)` SQL function so the `=~` constraint
/// (lowered to `value REGEXP pattern`) works. `GLOB` is native to SQLite, so the
/// `~~` constraint needs nothing. Compiled patterns are cached process-wide: a
/// `=~` filter over a large relation calls this once per ROW, and recompiling
/// the same regex millions of times was a quadratic CPU sink at scale.
fn register_regexp(conn: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    static CACHE: OnceLock<Mutex<HashMap<String, regex::Regex>>> = OnceLock::new();
    conn.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let pattern = ctx.get::<String>(0)?;
            let value = ctx.get::<String>(1)?;
            let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
            let mut map = cache.lock().unwrap();
            let re = match map.get(&pattern) {
                Some(re) => re.clone(),
                None => {
                    let re = regex::Regex::new(&pattern)
                        .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
                    map.insert(pattern, re.clone());
                    re
                }
            };
            drop(map);
            Ok(re.is_match(&value))
        },
    )?;
    Ok(())
}

impl Db {
    /// Escape hatch for not-yet-migrated call sites. Uncounted — burn these down.
    pub fn conn(&self) -> &Connection { &self.conn }

    fn bump(&self, key: &str) {
        *self.counts.borrow_mut().entry(key.to_string()).or_insert(0) += 1;
    }

    /// Reset the per-tick statement counter.
    pub fn tick_begin(&self) { self.counts.borrow_mut().clear(); }

    /// If any one statement ran past the threshold this tick, scream: a per-row
    /// loop slipped through the plural API. Returns the (label, count) it flagged.
    /// In profile mode also prints the tick's SQL aggregate and the most-repeated
    /// counted statements (the n+1 shortlist even under the scream threshold).
    pub fn tick_end(&self) -> Option<(String, u32)> {
        if profiling() {
            let (n, ms) = sql_stats_take();
            eprintln!("[profile] sql: {n} statements, {ms:.1}ms inside sqlite");
            let counts = self.counts.borrow();
            let mut top: Vec<(&String, &u32)> = counts.iter().filter(|(_, n)| **n > 1).collect();
            top.sort_by(|a, b| b.1.cmp(a.1));
            for (key, n) in top.iter().take(5) {
                eprintln!("[profile] {n}x {key}");
            }
        }
        let counts = self.counts.borrow();
        let (key, &n) = counts.iter().max_by_key(|(_, n)| **n)?;
        if n > N1_THRESHOLD {
            eprintln!("[n+1] '{key}' ran {n}x this tick — collect the set and call Db::insert_rows once");
            return Some((key.clone(), n));
        }
        None
    }

    /// Whole-statement DDL / bulk op (e.g. `DELETE FROM t`), counted once.
    pub fn exec(&self, sql: &str) -> Result<usize> {
        self.bump(sql);
        Ok(self.conn.execute(sql, [])?)
    }

    /// Insert a set of rows in chunked multi-row `VALUES` statements — ONE logical
    /// op (a few executes for very large N), never one-per-row. `INSERT OR IGNORE`.
    /// The counter keys on `INSERT <table>`, so a caller that loops this with
    /// singletons trips the N+1 scream.
    pub fn insert_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        if rows.is_empty() { return Ok(0); }
        let ncol = cols.len();
        let collist = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        let key = format!("INSERT {table}");
        self.bump(&key);
        let mut total = 0;
        for chunk in rows.chunks(CHUNK) {
            let tuple = format!("({})", vec!["?"; ncol].join(","));
            let values = vec![tuple; chunk.len()].join(", ");
            let sql = format!("INSERT OR IGNORE INTO {table} ({collist}) VALUES {values}");
            let params: Vec<rusqlite::types::Value> = chunk.iter().flatten().map(|v| match v {
                Value::Text(s) => rusqlite::types::Value::Text(s.clone()),
                Value::Int(n) => rusqlite::types::Value::Integer(*n),
            }).collect();
            total += self.conn.execute(&sql, rusqlite::params_from_iter(params))?;
        }
        Ok(total)
    }
}
