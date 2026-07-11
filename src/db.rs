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
/// (a 3-col relation flushes ~10k rows per statement, so even 100k rows is ~10).
const N1_THRESHOLD: u32 = 64;

/// Bound-parameter budget per multi-row `VALUES` statement. SQLite's default
/// SQLITE_MAX_VARIABLE_NUMBER is 32766; rows-per-chunk = PARAM_BUDGET / ncol,
/// so a 3-col relation flushes ~10k rows per statement.
const PARAM_BUDGET: usize = 32000;

pub fn open(path: Option<&str>) -> Result<Db> {
    let mut conn = match path {
        Some(p) => Connection::open(p)?,
        None => Connection::open_in_memory()?,
    };
    // busy_timeout: a hook `--check` and a resident `--lsp` share .dl/cache.db
    // across processes; a write collision should wait, not fail "locked".
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    // Page cache: SQLite's 2MB default thrashes this workload — the 2026-07-11
    // cold-tick profile put ~half the derived-phase wall in
    // BtreeIndexMoveto -> pread (cache-miss page reads on 500k-row index
    // probes) and another chunk in pagerStress dirty-page spill to the WAL.
    // Negative cache_size = KiB. DL_CACHE_MB overrides (integer megabytes);
    // default 512 — pages are allocated only as touched, so an idle db pays
    // nothing. mmap_size lets read pages come off the page cache entirely.
    let cache_mb: i64 = std::env::var("DL_CACHE_MB").ok()
        .and_then(|mb| mb.trim().parse().ok())
        .filter(|mb: &i64| *mb > 0)
        .unwrap_or(512);
    conn.execute_batch(&format!(
        "PRAGMA cache_size=-{kib}; PRAGMA mmap_size={bytes}; PRAGMA temp_store=MEMORY;",
        kib = cache_mb * 1024, bytes = cache_mb * 1024 * 1024))?;
    // Page cache: the 2MB SQLite default thrashes on this workload — a cold
    // tick's derived joins probe 500k-row indexes and the 2026-07-11 profile
    // showed ~half the wall in BtreeIndexMoveto -> pread (cache-miss page
    // reads) plus pagerStress dirty-page spill. Negative value = KiB.
    // DL_CACHE_MB overrides (integer MB); default 512MB — RAM is cheaper
    // than a 40s tick, and SQLite only allocates pages it actually touches.
    let cache_mb: i64 = std::env::var("DL_CACHE_MB").ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|mb| *mb > 0)
        .unwrap_or(512);
    conn.execute_batch(&format!(
        "PRAGMA cache_size=-{}; PRAGMA mmap_size={};",
        cache_mb * 1024, cache_mb * 1024 * 1024))?;
    // P2 (--check perf defect ledger): a loud, best-effort heads-up when
    // another live process already holds a write lock on this same db —
    // almost always a resident daemon. A one-shot `--check`/`dl` run that
    // silently piggybacks on (or blocks behind) a daemon's writes is a
    // confusing surprise otherwise. Probe with a throwaway `BEGIN IMMEDIATE`
    // under a short busy_timeout so the probe itself stays cheap; the real
    // 5s timeout above is restored right after. Never fatal: a probe that
    // fails for ANY reason (lock contention or otherwise) just prints and
    // moves on; an in-memory db has no `path` and is skipped entirely.
    if let Some(p) = path {
        let _ = conn.execute_batch("PRAGMA busy_timeout=50;");
        if conn.execute_batch("BEGIN IMMEDIATE;").is_err() {
            eprintln!(
                "[dl] warning: another process appears to hold a write lock on {p} \
                 (a resident daemon, or a concurrent dl run) — this process may block \
                 on writes or serve stale reads; use --no-daemon to isolate"
            );
        } else {
            let _ = conn.execute_batch("ROLLBACK;");
        }
    }
    // Custom busy handler (replaces the plain `PRAGMA busy_timeout` above):
    // same 5s deadline and 20ms backoff, but a stretch of retries past
    // `SQLITE_BUSY_WARN_MS` also gets a `[sqlite]` verdict line + perf.jsonl
    // row (once per blocked statement) — otherwise a contended shared
    // cache.db under the daemon retries silently with no observable signal.
    install_busy_verdict_handler(&conn)?;
    register_regexp(&conn)?;
    register_split(&conn)?;
    register_string_fns(&conn)?;
    if profiling() { conn.profile(Some(profile_hook)); }
    if let Some(p) = path {
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let journal_mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap_or_default();
        crate::verdict::verdict(
            "run-header",
            &format!("[db] opened {p} ({size} bytes, journal={journal_mode})"),
            &[("path", p), ("bytes", &size.to_string()), ("journal_mode", &journal_mode)],
        );
    }
    Ok(Db { conn, counts: RefCell::new(HashMap::new()) })
}

/// Install a busy handler that mimics the prior fixed 5000ms `busy_timeout`
/// (20ms sleep between retries, give up past 5000ms — same behavior SQLite's
/// own `busy_timeout` pragma implements) but ALSO surfaces a verdict when one
/// blocked statement's retry stretch passes `SQLITE_BUSY_WARN_MS`. SQLite
/// only supports one active busy strategy per connection (a custom handler
/// REPLACES `busy_timeout`, it does not compose with it), hence the
/// reimplementation here rather than pairing both.
fn install_busy_verdict_handler(conn: &Connection) -> Result<()> {
    use std::cell::Cell;
    use std::time::Instant;
    thread_local! {
        static BUSY_START: Cell<Option<Instant>> = const { Cell::new(None) };
        static BUSY_WARNED: Cell<bool> = const { Cell::new(false) };
    }
    conn.busy_handler(Some(|count: i32| {
        let start = BUSY_START.with(|cell| {
            if count == 0 {
                let now = Instant::now();
                cell.set(Some(now));
                BUSY_WARNED.with(|w| w.set(false));
                now
            } else {
                cell.get().unwrap_or_else(Instant::now)
            }
        });
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if elapsed_ms >= crate::verdict::SQLITE_BUSY_WARN_MS {
            let already_warned = BUSY_WARNED.with(|w| w.replace(true));
            if !already_warned {
                crate::verdict::verdict(
                    "sqlite-busy",
                    &format!("[sqlite] busy retry {elapsed_ms}ms (attempt {count})"),
                    &[("elapsed_ms", &elapsed_ms.to_string()), ("attempt", &count.to_string())],
                );
            }
        }
        if elapsed_ms >= 5000 { return false; }
        std::thread::sleep(Duration::from_millis(20));
        true
    }))?;
    Ok(())
}

/// Register the `regexp(pattern, value)` SQL function so the `=~` constraint
/// (lowered to `value REGEXP pattern`) works. `GLOB` is native to SQLite, so the
/// `~~` constraint needs nothing. Compiled patterns are cached process-wide: a
/// `=~` filter over a large relation calls this once per ROW, and recompiling
/// the same regex millions of times was a quadratic CPU sink at scale.
fn register_regexp(conn: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let pattern = ctx.get::<String>(0)?;
            let value = ctx.get::<String>(1)?;
            let re = compiled_regex(&pattern)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
            Ok(re.is_match(&value))
        },
    )?;
    Ok(())
}

/// Process-wide regex compile cache. A `=~` filter or `replace_re(..)` over a
/// large relation calls this once per ROW; recompiling the same pattern millions
/// of times was a quadratic CPU sink at scale.
fn compiled_regex(pattern: &str) -> Result<regex::Regex, regex::Error> {
    static CACHE: OnceLock<Mutex<HashMap<String, regex::Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().unwrap();
    if let Some(re) = map.get(pattern) { return Ok(re.clone()); }
    let re = regex::Regex::new(pattern)?;
    map.insert(pattern.to_string(), re.clone());
    Ok(re)
}

/// Register the `sprf_split(text, sep, idx)` SQL function so the `split(...)`
/// term lowers to a per-row SQL call. Idx is 0-based; negative counts from the
/// end (`-1` = last segment). Out-of-range or empty sep returns NULL, which
/// drops the row from a SELECT (the desired filter semantics — a split that
/// misses is no row, not an empty string). `split` is the only caller today;
/// the `sprf_` prefix avoids colliding with a user rel of the same name.
fn register_split(conn: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function(
        "sprf_split",
        3,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let text = ctx.get::<String>(0)?;
            let sep = ctx.get::<String>(1)?;
            let idx = ctx.get::<i64>(2)?;
            if sep.is_empty() { return Ok(None); }
            let parts: Vec<&str> = text.split(&sep).collect();
            let n = parts.len() as i64;
            let i = if idx >= 0 { idx } else { idx + n };
            if i < 0 || i >= n { return Ok(None); }
            Ok(Some(parts[i as usize].to_string()))
        },
    )?;
    Ok(())
}

/// First char lower/upper-cased, the rest untouched (Unicode-aware). Empty in,
/// empty out. `getUser`/`GetUser` round-trips ride these — the RTKQ op name.
fn map_first(s: &str, up: bool) -> String {
    let mut it = s.chars();
    match it.next() {
        None => String::new(),
        Some(f) => {
            let head: String = if up { f.to_uppercase().collect() } else { f.to_lowercase().collect() };
            head + it.as_str()
        }
    }
}

/// Register the pass-through string builtins. All are text->text, deterministic.
/// `strip_prefix`/`strip_suffix` return the input UNCHANGED when the affix is
/// absent (an idempotent cleanup, not a filter — pair with `=~ /^p/` if you want
/// drop-on-miss). `replace_re(s, pattern, repl)` is regex replace-all with `$1`
/// group refs; its pattern share the process-wide compile cache.
fn register_string_fns(conn: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    let det = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;

    conn.create_scalar_function("sprf_lower", 1, det,
        |ctx| Ok(ctx.get::<String>(0)?.to_lowercase()))?;
    conn.create_scalar_function("sprf_upper", 1, det,
        |ctx| Ok(ctx.get::<String>(0)?.to_uppercase()))?;
    conn.create_scalar_function("sprf_lcfirst", 1, det,
        |ctx| Ok(map_first(&ctx.get::<String>(0)?, false)))?;
    conn.create_scalar_function("sprf_ucfirst", 1, det,
        |ctx| Ok(map_first(&ctx.get::<String>(0)?, true)))?;
    conn.create_scalar_function("sprf_trim", 1, det,
        |ctx| Ok(ctx.get::<String>(0)?.trim().to_string()))?;
    // Same normalization the ref-spine folds into `_strings.norm` / the
    // `string(id,text,norm)` rel: ASCII-alnum only, lowercased. Exposed as a
    // scalar so `norm(a) = norm(b)` is a punctuation/case-blind compare, and
    // arbitrary text joins against `string.norm`.
    conn.create_scalar_function("sprf_norm", 1, det,
        |ctx| Ok(crate::spine::normalize(&ctx.get::<String>(0)?)))?;

    conn.create_scalar_function("sprf_strip_prefix", 2, det, |ctx| {
        let s = ctx.get::<String>(0)?;
        let p = ctx.get::<String>(1)?;
        Ok(s.strip_prefix(&p).map(str::to_string).unwrap_or(s))
    })?;
    conn.create_scalar_function("sprf_strip_suffix", 2, det, |ctx| {
        let s = ctx.get::<String>(0)?;
        let p = ctx.get::<String>(1)?;
        Ok(s.strip_suffix(&p).map(str::to_string).unwrap_or(s))
    })?;

    conn.create_scalar_function("sprf_replace_re", 3, det, |ctx| {
        let s = ctx.get::<String>(0)?;
        let pattern = ctx.get::<String>(1)?;
        let repl = ctx.get::<String>(2)?;
        let re = compiled_regex(&pattern)
            .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
        Ok(re.replace_all(&s, repl.as_str()).into_owned())
    })?;
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

    /// Counted prepare — wraps `Connection::prepare`. Call sites that need a
    /// `Statement` (e.g. multi-row `query_map`) migrate off `conn().prepare(...)`.
    pub fn prepare(&self, sql: &str) -> Result<rusqlite::Statement<'_>> {
        self.bump(sql);
        Ok(self.conn.prepare(sql)?)
    }

    /// Counted query_row — single-row scalar lookup. Wraps
    /// `Connection::query_row` so common `SELECT COUNT(*)` / metadata queries
    /// don't bypass the counter.
    pub fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        self.bump(sql);
        Ok(self.conn.query_row(sql, params, f)?)
    }

    /// Counted execute_batch — multi-statement SQL script (DDL like
    /// `CREATE TABLE t (...); CREATE INDEX ...`). Wraps
    /// `Connection::execute_batch`.
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        self.bump(sql);
        Ok(self.conn.execute_batch(sql)?)
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
        let chunk_rows = (PARAM_BUDGET / ncol.max(1)).max(1);
        let multi = rows.len() > chunk_rows;
        if multi { self.conn.execute_batch("BEGIN")?; }
        let mut total = 0;
        for chunk in rows.chunks(chunk_rows) {
            let tuple = format!("({})", vec!["?"; ncol].join(","));
            let values = vec![tuple; chunk.len()].join(", ");
            let sql = format!("INSERT OR IGNORE INTO {table} ({collist}) VALUES {values}");
            let params: Vec<rusqlite::types::Value> = chunk.iter().flatten().map(|v| match v {
                Value::Text(s) => rusqlite::types::Value::Text(s.clone()),
                Value::Int(n) => rusqlite::types::Value::Integer(*n),
                Value::Null => rusqlite::types::Value::Null,
            }).collect();
            let res = self.conn.execute(&sql, rusqlite::params_from_iter(params));
            match res {
                Ok(n) => total += n,
                Err(e) => {
                    if multi { let _ = self.conn.execute_batch("ROLLBACK"); }
                    return Err(e.into());
                }
            }
        }
        if multi { self.conn.execute_batch("COMMIT")?; }
        Ok(total)
    }
}

/// P2 (--check perf defect ledger): on a clean close, shrink the WAL back
/// into the main db file so a long-lived daemon (or a burst of one-shot
/// runs) doesn't leave an ever-growing `-wal` file behind. Best-effort —
/// `TRUNCATE` can fail (e.g. another connection still holds an open read
/// transaction pinning old WAL frames); errors are ignored, never fatal, and
/// never block the process from exiting.
impl Drop for Db {
    fn drop(&mut self) {
        let _ = self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_rows_chunks_by_param_budget() {
        let db = open(None).unwrap();
        db.exec("CREATE TABLE t (a INTEGER, b TEXT)").unwrap();
        // 2 cols -> 16000 rows per statement; 33k rows = 3 chunks under one
        // transaction, still ONE counted logical op (no n+1 scream).
        let rows: Vec<Vec<Value>> = (0..33_000)
            .map(|i| vec![Value::Int(i), Value::Text(format!("r{i}"))])
            .collect();
        db.tick_begin();
        let n = db.insert_rows("t", &["a", "b"], &rows).unwrap();
        assert_eq!(n, 33_000);
        assert!(db.tick_end().is_none(), "chunked insert must not trip the n+1 counter");
        let count: i64 = db.conn().query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 33_000);
    }

    #[test]
    fn insert_rows_or_ignore_holds_across_chunks() {
        // OR IGNORE semantics survive the chunk/transaction plumbing: a
        // constraint-violating row anywhere in a multi-chunk batch is skipped,
        // never an error, never a partial abort.
        let db = open(None).unwrap();
        db.exec("CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        let mut rows: Vec<Vec<Value>> = (0..40_000).map(|i| vec![Value::Int(i)]).collect();
        rows.push(vec![Value::Int(7)]); // duplicate key, lands in the last chunk
        let n = db.insert_rows("t", &["a"], &rows).unwrap();
        assert_eq!(n, 40_000, "duplicate ignored, everything else lands");
        let count: i64 = db.conn().query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 40_000);
    }

    #[test]
    fn bundled_sqlite_accepts_order_by_inside_aggregate() {
        // json_agg determinism rides `ORDER BY` inside the aggregate call (SQLite
        // >= 3.44). If the bundled SQLite ever regresses below that, this fails and
        // the fallback (a windowed ordered subquery) must ship instead. Also proves
        // json_group_array / json_group_object are present (core since 3.38).
        let db = open(None).unwrap();
        db.exec("CREATE TABLE t (g TEXT, k TEXT, v INTEGER)").unwrap();
        let rows = vec![
            vec![Value::Text("a".into()), Value::Text("z".into()), Value::Int(1)],
            vec![Value::Text("a".into()), Value::Text("x".into()), Value::Int(2)],
            vec![Value::Text("a".into()), Value::Text("y".into()), Value::Int(3)],
        ];
        db.insert_rows("t", &["g", "k", "v"], &rows).unwrap();
        // ORDER BY inside json_group_array makes the element order deterministic.
        let arr: String = db.conn().query_row(
            "SELECT json_group_array(k ORDER BY k) FROM t GROUP BY g", [], |r| r.get(0)).unwrap();
        assert_eq!(arr, r#"["x","y","z"]"#);
        // ORDER BY the key inside json_group_object.
        let obj: String = db.conn().query_row(
            "SELECT json_group_object(k, v ORDER BY k) FROM t GROUP BY g", [], |r| r.get(0)).unwrap();
        assert_eq!(obj, r#"{"x":2,"y":3,"z":1}"#);
    }

    #[test]
    fn prepare_and_query_map_works() {
        let db = open(None).unwrap();
        db.exec("CREATE TABLE t (a INTEGER, b TEXT)").unwrap();
        let rows: Vec<Vec<Value>> = (0..5)
            .map(|i| vec![Value::Int(i), Value::Text(format!("r{i}"))])
            .collect();
        db.insert_rows("t", &["a", "b"], &rows).unwrap();
        db.tick_begin();
        let mut stmt = db.prepare("SELECT a, b FROM t ORDER BY a").unwrap();
        let got: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(db.tick_end().is_none());
        assert_eq!(got.len(), 5);
        assert_eq!(got[0], (0, "r0".to_string()));
        assert_eq!(got[4], (4, "r4".to_string()));
    }

    #[test]
    fn query_row_returns_scalar() {
        let db = open(None).unwrap();
        db.exec("CREATE TABLE t (a INTEGER)").unwrap();
        let rows: Vec<Vec<Value>> = (0..7).map(|i| vec![Value::Int(i)]).collect();
        db.insert_rows("t", &["a"], &rows).unwrap();
        db.tick_begin();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get::<_, i64>(0))
            .unwrap();
        assert!(db.tick_end().is_none());
        assert_eq!(count, 7);
    }

    #[test]
    fn execute_batch_runs_multi_statement_ddl() {
        let db = open(None).unwrap();
        db.tick_begin();
        db.execute_batch(
            "CREATE TABLE a (x INTEGER); CREATE TABLE b (y TEXT);",
        )
        .unwrap();
        assert!(db.tick_end().is_none());
        let na: i64 = db.conn().query_row("SELECT COUNT(*) FROM a", [], |r| r.get(0)).unwrap();
        let nb: i64 = db.conn().query_row("SELECT COUNT(*) FROM b", [], |r| r.get(0)).unwrap();
        assert_eq!(na, 0);
        assert_eq!(nb, 0);
    }
}
