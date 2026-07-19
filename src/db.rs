use anyhow::{Context, Result};
use rusqlite::{Connection, Row, ToSql};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::ast::Value;

/// Re-exported so a generic row-mapping closure parameter outside the seam
/// (e.g. a tolerant-read helper's `FnMut` bound) can name its argument type
/// without importing `rusqlite` directly — the seam stays the only file that
/// spells `rusqlite::`.
pub type SqlRow<'a> = Row<'a>;

/// Same reasoning as [`SqlRow`]: the raw per-cell `rusqlite::Result` a
/// `Row::get` call returns, named without an outside-the-seam `rusqlite::`
/// mention.
pub type SqlRowResult<T> = rusqlite::Result<T>;

/// Same reasoning as [`SqlRow`]: the zero-copy cell view `Row::get_ref`
/// returns, for a reader that must branch on the SQLite storage type (a
/// `sym`-typed column may store INTEGER, not TEXT) instead of assuming one.
pub type SqlValueRef<'a> = rusqlite::types::ValueRef<'a>;

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
static SQLITE_CACHE_RESERVED_KIB: AtomicU64 = AtomicU64::new(0);
static SQLITE_MMAP_RESERVED_BYTES: AtomicU64 = AtomicU64::new(0);

const DEFAULT_SQLITE_CACHE_BUDGET_MB: u64 = 32;
const DEFAULT_CONNECTION_CACHE_MB: u64 = 16;

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
        let ms = dur.as_secs_f64() * 1000.0;
        tracing::debug!(ms, head = %head, ellipsis = %ellipsis, "[sql {ms:.1}ms] {head}{ellipsis}");
    }
}

/// The single owner of the SQLite `Connection` — the one place SQL is issued, so
/// the backend stays swappable and every interaction is counted. Methods are
/// PLURAL by construction: each issues one logical statement regardless of N
/// (chunked multi-row `VALUES`), so a per-row write cannot hide behind this
/// boundary. A per-tick statement counter screams when one statement runs more
/// than `N1_THRESHOLD` times in a tick — the runtime N+1 detector that replaces
/// the (unworkable) static sniff.
pub struct Db {
    conn: Connection,
    counts: RefCell<HashMap<String, u32>>,
    pending_syms: Arc<Mutex<Vec<String>>>,
    _memory_budget: SqliteMemoryBudget,
    /// Per-tick capture of every write that flows through `insert_rows`:
    /// (relation name, rows actually inserted). Collected in memory and flushed
    /// once at tick end into `_write_ledger`; never a per-row write.
    write_ledger: RefCell<Vec<(String, usize)>>,
}

/// One statement running more than this many times in a tick is almost certainly
/// a per-row loop that escaped the plural API. Chunked batches stay well under it
/// (a 3-col relation flushes ~10k rows per statement, so even 100k rows is ~10).
const N1_THRESHOLD: u32 = 64;

/// Bound-parameter budget per multi-row `VALUES` statement. SQLite's default
/// SQLITE_MAX_VARIABLE_NUMBER is 32766; rows-per-chunk = PARAM_BUDGET / ncol,
/// so a 3-col relation flushes ~10k rows per statement. `pub(crate)` so
/// `storage::retract_rows` chunks its `DELETE ... VALUES` the same way
/// `insert_rows` chunks its `INSERT ... VALUES` — one bound-parameter budget,
/// shared, instead of two constants drifting apart.
pub(crate) const PARAM_BUDGET: usize = 32000;

#[derive(Debug, PartialEq, Eq)]
struct SqliteMemoryBudget {
    cache_kib: u64,
    mmap_bytes: u64,
}

impl Drop for SqliteMemoryBudget {
    fn drop(&mut self) {
        SQLITE_CACHE_RESERVED_KIB.fetch_sub(self.cache_kib, Ordering::AcqRel);
        SQLITE_MMAP_RESERVED_BYTES.fetch_sub(self.mmap_bytes, Ordering::AcqRel);
    }
}

fn positive_env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.trim().parse().ok().filter(|n| *n > 0)
}

fn reserve_process_budget(reserved: &AtomicU64, total: u64, requested: u64) -> u64 {
    let mut used = reserved.load(Ordering::Relaxed);
    loop {
        let grant = requested.min(total.saturating_sub(used));
        if grant == 0 { return 0; }
        match reserved.compare_exchange_weak(
            used,
            used + grant,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => return grant,
            Err(actual) => used = actual,
        }
    }
}

fn sqlite_memory_budget() -> SqliteMemoryBudget {
    // DL_CACHE_MB is now a process-wide ceiling, not a per-connection promise.
    // A connection receives at most DL_CONNECTION_CACHE_MB so opening several
    // roots cannot multiply a 512 MiB setting by the root count.
    let total_cache_kib = positive_env_u64("DL_CACHE_MB")
        .unwrap_or(DEFAULT_SQLITE_CACHE_BUDGET_MB)
        .saturating_mul(1024);
    let per_connection_kib = positive_env_u64("DL_CONNECTION_CACHE_MB")
        .unwrap_or(DEFAULT_CONNECTION_CACHE_MB)
        .saturating_mul(1024);
    let cache_kib = reserve_process_budget(
        &SQLITE_CACHE_RESERVED_KIB,
        total_cache_kib,
        per_connection_kib,
    );

    // Mapped pages count against process footprint too. Keep mmap disabled by
    // default; DL_MMAP_MB explicitly grants a process-wide mapping budget.
    let total_mmap_bytes = positive_env_u64("DL_MMAP_MB").unwrap_or(0).saturating_mul(1024 * 1024);
    let mmap_bytes = reserve_process_budget(
        &SQLITE_MMAP_RESERVED_BYTES,
        total_mmap_bytes,
        total_mmap_bytes,
    );

    SqliteMemoryBudget { cache_kib, mmap_bytes }
}

pub fn open(path: Option<&str>) -> Result<Db> {
    let mut conn = match path {
        Some(p) => Connection::open(p)?,
        None => Connection::open_in_memory()?,
    };
    // Seam statement cache: every Db method prepares via prepare_cached, so
    // hot per-row statements (source-stage completion marks, extract loops)
    // parse once per connection instead of once per call. Default capacity
    // is 16, far below a tick's distinct-statement count.
    conn.set_prepared_statement_cache_capacity(256);
    // busy_timeout: a hook `--check`, a resident `--lsp`, and a daemon can all
    // share the per-root `roots/<key>/db.sqlite` across processes
    // (storage-endgame L2, one db per corpus); a write collision should wait,
    // not fail "locked".
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    // Cache and mapped-page ceilings are process-wide. The former duplicated
    // 512 MiB setup made every connection eligible to retain a huge page cache,
    // while temp_store=MEMORY let sorts/staging escape any byte budget. A root
    // with no remaining cache lease gets cache_size=0. SQLite still has small
    // unavoidable connection bookkeeping, but no unaccounted page-cache grant;
    // correctness never depends on receiving a lease.
    let memory = sqlite_memory_budget();
    let configured_cache_kib = memory.cache_kib;
    conn.execute_batch(&format!(
        "PRAGMA cache_size=-{configured_cache_kib}; \
         PRAGMA mmap_size={}; \
         PRAGMA temp_store=FILE;",
        memory.mmap_bytes))?;
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
        // @rusqlite-ok: probe setup; failure only means the real busy_timeout stays in effect.
        let _ = conn.execute_batch("PRAGMA busy_timeout=50;");
        if conn.execute_batch("BEGIN IMMEDIATE;").is_err() {
            tracing::warn!(
                path = p,
                "[dl] warning: another process appears to hold a write lock on {p} \
                 (a resident daemon, or a concurrent dl run) — this process may block \
                 on writes or serve stale reads; use --no-daemon to isolate"
            );
        } else {
            // @rusqlite-ok: rollback the probe transaction; errors here are harmless.
            let _ = conn.execute_batch("ROLLBACK;");
        }
    }
    // Custom busy handler (replaces the plain `PRAGMA busy_timeout` above):
    // same 5s deadline and 20ms backoff, but a stretch of retries past
    // `SQLITE_BUSY_WARN_MS` also gets a `[sqlite]` verdict line + perf.jsonl
    // row (once per blocked statement) — otherwise a contended shared
    // root db under the daemon retries silently with no observable signal.
    install_busy_verdict_handler(&conn)?;
    register_regexp(&conn)?;
    register_split(&conn)?;
    register_string_fns(&conn)?;
    let pending_syms = Arc::new(Mutex::new(Vec::new()));
    {
        let pending = pending_syms.clone();
        conn.create_scalar_function("sprf_sym_intern", 1,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8,
            move |ctx| {
                let Some(text) = ctx.get::<Option<String>>(0)? else { return Ok(None); };
                if !text.is_empty() {
                    pending.lock().expect("pending symbol queue poisoned").push(text.clone());
                }
                Ok(Some(crate::spine::StringId::of(&text).sqlite()))
            })?;
    }
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
    Ok(Db {
        conn,
        counts: RefCell::new(HashMap::new()),
        pending_syms,
        _memory_budget: memory,
        write_ledger: RefCell::new(Vec::new()),
    })
}

/// Open a READ-ONLY connection on an existing on-disk db, for the daemon's
/// lock-free read path (`crate::daemon_read`). The main daemon connection keeps
/// the db in WAL mode (`open`, above), so a reader opened here NEVER blocks on
/// the writer and always sees the last COMMITTED state — the property the read
/// path relies on to answer row/query RPCs without taking the engine mutex.
///
/// Registers the same pure SQL helper functions the writer connection carries
/// (regexp/split/string fns) so lowered query SQL evaluates identically.
/// `sprf_sym_intern` is registered in a READ-ONLY form here: it returns the
/// deterministic `StringId` for the text WITHOUT queueing an intern, because a
/// read connection never flushes the pending-symbol queue (the aggregate-query
/// path, whose lowering is the only user of this function, is kept on the
/// engine lock by `daemon_read` for exactly the newly-interned-string case this
/// cannot persist).
pub fn open_read_only(path: &str) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    // A WAL reader does not contend for the write lock, so this timeout should
    // never fire; keep it small so an unexpected -shm race on open surfaces fast
    // rather than eating into the read SLA.
    conn.busy_timeout(Duration::from_millis(1000))?;
    register_regexp(&conn)?;
    register_split(&conn)?;
    register_string_fns(&conn)?;
    conn.create_scalar_function(
        "sprf_sym_intern",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8,
        |ctx| {
            let Some(text) = ctx.get::<Option<String>>(0)? else { return Ok(None); };
            Ok(Some(crate::spine::StringId::of(&text).sqlite()))
        },
    )?;
    Ok(conn)
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
            let (Some(pattern), Some(value)) =
                (ctx.get::<Option<String>>(0)?, ctx.get::<Option<String>>(1)?)
            else { return Ok(None); };
            let re = compiled_regex(&pattern)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
            Ok(Some(re.is_match(&value)))
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
            let (Some(text), Some(sep), Some(idx)) = (
                ctx.get::<Option<String>>(0)?,
                ctx.get::<Option<String>>(1)?,
                ctx.get::<Option<i64>>(2)?,
            ) else { return Ok(None); };
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

    // Every text fn propagates NULL (SQL semantics: NULL in, NULL out), same
    // as SQLite's own lower()/upper(). A NON-Option `ctx.get::<String>` on a
    // NULL argument aborts the whole statement with "Invalid function
    // parameter type Null at index 0" — the 2026-07-18 cold-extract poison-job
    // incident (failure-modes class 18 follow-on): one NULL fed to sprf_norm
    // failed a 109s ColdExtract job on every retry.
    conn.create_scalar_function("sprf_lower", 1, det,
        |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|s| s.to_lowercase())))?;
    conn.create_scalar_function("sprf_upper", 1, det,
        |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|s| s.to_uppercase())))?;
    conn.create_scalar_function("sprf_lcfirst", 1, det,
        |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|s| map_first(&s, false))))?;
    conn.create_scalar_function("sprf_ucfirst", 1, det,
        |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|s| map_first(&s, true))))?;
    conn.create_scalar_function("sprf_trim", 1, det,
        |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|s| s.trim().to_string())))?;
    // Same normalization the ref-spine folds into `_strings.norm` / the
    // `string(id,text,norm)` rel: ASCII-alnum only, lowercased. Exposed as a
    // scalar so `norm(a) = norm(b)` is a punctuation/case-blind compare, and
    // arbitrary text joins against `string.norm`.
    conn.create_scalar_function("sprf_norm", 1, det,
        |ctx| Ok(ctx.get::<Option<String>>(0)?.map(|s| crate::spine::normalize(&s))))?;

    conn.create_scalar_function("sprf_strip_prefix", 2, det, |ctx| {
        let (Some(s), Some(p)) = (ctx.get::<Option<String>>(0)?, ctx.get::<Option<String>>(1)?)
        else { return Ok(None); };
        Ok(Some(s.strip_prefix(&p).map(str::to_string).unwrap_or(s)))
    })?;
    conn.create_scalar_function("sprf_strip_suffix", 2, det, |ctx| {
        let (Some(s), Some(p)) = (ctx.get::<Option<String>>(0)?, ctx.get::<Option<String>>(1)?)
        else { return Ok(None); };
        Ok(Some(s.strip_suffix(&p).map(str::to_string).unwrap_or(s)))
    })?;

    // Content-addressed intern id of a text (StringId::of as i64) — lets a
    // sym column compare against a runtime-computed text by hashing the TEXT
    // side (one hash per candidate row, zero `_strings` lookups) instead of
    // decoding the sym side.
    conn.create_scalar_function("sprf_sym", 1, det,
        |ctx| Ok(ctx.get::<Option<String>>(0)?
            .map(|s| crate::spine::StringId::of(&s).sqlite())))?;

    // Line count of a text value (newline count + 1, 0 for empty) — feeds the
    // file-size rail (`lines(content)`).
    conn.create_scalar_function("sprf_lines", 1, det, |ctx| {
        let Some(s) = ctx.get::<Option<String>>(0)? else { return Ok(None); };
        Ok(Some(if s.is_empty() { 0i64 } else { s.lines().count() as i64 }))
    })?;

    conn.create_scalar_function("sprf_replace_re", 3, det, |ctx| {
        let (Some(s), Some(pattern), Some(repl)) = (
            ctx.get::<Option<String>>(0)?,
            ctx.get::<Option<String>>(1)?,
            ctx.get::<Option<String>>(2)?,
        ) else { return Ok(None); };
        let re = compiled_regex(&pattern)
            .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
        Ok(Some(re.replace_all(&s, repl.as_str()).into_owned()))
    })?;
    Ok(())
}

/// One bound parameter or result cell, seam-owned. Call sites build these;
/// `rusqlite::types::Value` never leaves db.rs.
#[derive(Clone, Debug, PartialEq)]
pub enum SqlVal {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl SqlVal {
    /// Borrow the text payload, if this cell is text.
    pub fn as_text(&self) -> Option<&str> {
        match self { SqlVal::Text(s) => Some(s), _ => None }
    }

    /// Borrow the integer payload, if this cell is an integer.
    pub fn as_int(&self) -> Option<i64> {
        match self { SqlVal::Int(i) => Some(*i), _ => None }
    }

    /// The param mapping engine/rpc.rs::query_sql and daemon_read.rs::json_rows
    /// duplicate today: String->Text, i64->Int, f64->Real, Null->Null,
    /// other->Text(v.to_string()).
    pub fn from_json(v: &serde_json::Value) -> SqlVal {
        match v {
            serde_json::Value::Null => SqlVal::Null,
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    SqlVal::Int(i)
                } else if let Some(r) = n.as_f64() {
                    SqlVal::Real(r)
                } else {
                    SqlVal::Text(v.to_string())
                }
            }
            serde_json::Value::String(s) => SqlVal::Text(s.clone()),
            serde_json::Value::Bool(b) => SqlVal::Int(i64::from(*b)),
            other => SqlVal::Text(other.to_string()),
        }
    }

    /// Text->String, Int->Number, Real->Number, Null->Null, Blob->"<blob NB>"
    /// (the rpc.rs::query_sql flavor).
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            SqlVal::Null => serde_json::Value::Null,
            SqlVal::Int(i) => serde_json::Value::Number((*i).into()),
            SqlVal::Real(r) => serde_json::Value::Number(
                serde_json::Number::from_f64(*r).unwrap_or_else(|| 0.into())),
            SqlVal::Text(s) => serde_json::Value::String(s.clone()),
            SqlVal::Blob(_) => serde_json::Value::String("<blob NB>".to_string()),
        }
    }

    /// The engine/mod.rs::cell_as_string flavor: Null->"", Int/Real->to_string,
    /// Text->clone, Blob->lossy utf8.
    pub fn to_lossy_string(&self) -> String {
        match self {
            SqlVal::Null => String::new(),
            SqlVal::Int(i) => i.to_string(),
            SqlVal::Real(r) => r.to_string(),
            SqlVal::Text(s) => s.clone(),
            SqlVal::Blob(b) => String::from_utf8_lossy(b).into_owned(),
        }
    }
}

impl From<&str> for SqlVal {
    fn from(value: &str) -> Self { SqlVal::Text(value.to_string()) }
}

impl From<String> for SqlVal {
    fn from(value: String) -> Self { SqlVal::Text(value) }
}

impl From<&String> for SqlVal {
    fn from(value: &String) -> Self { SqlVal::Text(value.clone()) }
}

impl From<i64> for SqlVal {
    fn from(value: i64) -> Self { SqlVal::Int(value) }
}

impl From<i32> for SqlVal {
    fn from(value: i32) -> Self { SqlVal::Int(i64::from(value)) }
}

impl From<u32> for SqlVal {
    fn from(value: u32) -> Self { SqlVal::Int(i64::from(value)) }
}

impl From<usize> for SqlVal {
    fn from(value: usize) -> Self { SqlVal::Int(value as i64) }
}

impl From<f64> for SqlVal {
    fn from(value: f64) -> Self { SqlVal::Real(value) }
}

impl From<&[u8]> for SqlVal {
    fn from(value: &[u8]) -> Self { SqlVal::Blob(value.to_vec()) }
}

impl From<Vec<u8>> for SqlVal {
    fn from(value: Vec<u8>) -> Self { SqlVal::Blob(value) }
}

impl<T: Into<SqlVal>> From<Option<T>> for SqlVal {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(v) => v.into(),
            None => SqlVal::Null,
        }
    }
}

impl From<&Value> for SqlVal {
    fn from(value: &Value) -> Self {
        match value {
            Value::Text(s) => SqlVal::Text(s.clone()),
            Value::Int(n) => SqlVal::Int(*n),
            Value::Null => SqlVal::Null,
        }
    }
}

impl From<Value> for SqlVal {
    fn from(value: Value) -> Self {
        match value {
            Value::Text(s) => SqlVal::Text(s),
            Value::Int(n) => SqlVal::Int(n),
            Value::Null => SqlVal::Null,
        }
    }
}

impl From<SqlVal> for rusqlite::types::Value {
    fn from(value: SqlVal) -> Self {
        match value {
            SqlVal::Null => rusqlite::types::Value::Null,
            SqlVal::Int(i) => rusqlite::types::Value::Integer(i),
            SqlVal::Real(r) => rusqlite::types::Value::Real(r),
            SqlVal::Text(s) => rusqlite::types::Value::Text(s),
            SqlVal::Blob(b) => rusqlite::types::Value::Blob(b),
        }
    }
}

impl From<&SqlVal> for rusqlite::types::Value {
    fn from(value: &SqlVal) -> Self {
        match value {
            SqlVal::Null => rusqlite::types::Value::Null,
            SqlVal::Int(i) => rusqlite::types::Value::Integer(*i),
            SqlVal::Real(r) => rusqlite::types::Value::Real(*r),
            SqlVal::Text(s) => rusqlite::types::Value::Text(s.clone()),
            SqlVal::Blob(b) => rusqlite::types::Value::Blob(b.clone()),
        }
    }
}

impl From<rusqlite::types::Value> for SqlVal {
    fn from(value: rusqlite::types::Value) -> Self {
        match value {
            rusqlite::types::Value::Null => SqlVal::Null,
            rusqlite::types::Value::Integer(i) => SqlVal::Int(i),
            rusqlite::types::Value::Real(r) => SqlVal::Real(r),
            rusqlite::types::Value::Text(s) => SqlVal::Text(s),
            rusqlite::types::Value::Blob(b) => SqlVal::Blob(b),
        }
    }
}

impl rusqlite::types::ToSql for SqlVal {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Owned(self.into()))
    }
}

impl rusqlite::types::FromSql for SqlVal {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(match value {
            rusqlite::types::ValueRef::Null => SqlVal::Null,
            rusqlite::types::ValueRef::Integer(i) => SqlVal::Int(i),
            rusqlite::types::ValueRef::Real(r) => SqlVal::Real(r),
            rusqlite::types::ValueRef::Text(t) => SqlVal::Text(String::from_utf8_lossy(t).into_owned()),
            rusqlite::types::ValueRef::Blob(b) => SqlVal::Blob(b.to_vec()),
        })
    }
}

/// Wrapper so an `anyhow::Error` can ride inside `rusqlite::Error::UserFunctionError`
/// when a row-mapping closure fails. `anyhow::Error` itself does not implement
/// `std::error::Error` (by design), so we delegate Display/Debug to it.
#[derive(Debug)]
struct RowMapError(anyhow::Error);

impl std::fmt::Display for RowMapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for RowMapError {}

fn map_row<T>(result: Result<T>, rel: &str, head: &str) -> rusqlite::Result<T> {
    result
        .with_context(|| format!("row map on {rel}: {head}"))
        .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(RowMapError(e))))
}

/// Statement head for logs and errors: whitespace-compacted, first 80 chars.
fn stmt_head(sql: &str) -> String {
    let compact: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    let head: String = compact.chars().take(80).collect();
    let ellipsis = if compact.chars().count() > 80 { "…" } else { "" };
    format!("{head}{ellipsis}")
}

/// THE error-context wrapper. Every statement failure leaves the seam as:
///   sql failed on <rel>: <stmt_head> — <rusqlite error>
fn sql_err(rel: &str, sql: &str, e: rusqlite::Error) -> anyhow::Error {
    anyhow::anyhow!("sql failed on {rel}: {} — {e}", stmt_head(sql))
}

/// "?, ?, …" n times — IN-list rendering for the *_in_chunks helpers.
pub fn holes(n: usize) -> String {
    std::iter::repeat("?").take(n).collect::<Vec<_>>().join(", ")
}

fn to_sql_params(params: &[SqlVal]) -> Vec<&dyn ToSql> {
    params.iter().map(|v| v as &dyn ToSql).collect()
}

impl Db {
    /// Escape hatch for not-yet-migrated call sites. Deleted in the final
    /// burn-down commit once every call site routes through a named seam method.
    pub fn conn(&self) -> &Connection { &self.conn }

    /// Whether this connection is outside a transaction, without exposing the
    /// raw connection through the storage seam.
    pub fn is_autocommit(&self) -> bool { self.conn.is_autocommit() }

    pub fn flush_pending_syms(&self) -> Result<usize> {
        let texts = std::mem::take(&mut *self.pending_syms.lock().expect("pending symbol queue poisoned"));
        if texts.is_empty() { return Ok(0); }
        let mut sink = crate::spine::SymSink::new();
        for text in texts { sink.sym(&text); }
        self.flush_syms(&mut sink)
    }

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
            tracing::debug!(n, ms, "[profile] sql: {n} statements, {ms:.1}ms inside sqlite");
            let counts = self.counts.borrow();
            let mut top: Vec<(&String, &u32)> = counts.iter().filter(|(_, n)| **n > 1).collect();
            top.sort_by(|a, b| b.1.cmp(a.1));
            for (key, n) in top.iter().take(5) {
                let key = *key;
                let count = **n;
                tracing::debug!(key = %key, count, "[profile] {count}x {key}");
            }
        }
        let counts = self.counts.borrow();
        // Reads bump under a "SELECT {rel}" key and stay out of the scream:
        // the fixpoint legitimately queries a shared head rel once per rule,
        // which is O(program), never O(rows). The scream is for write loops.
        let (key, &n) = counts
            .iter()
            .filter(|(key, _)| !key.starts_with("SELECT "))
            .max_by_key(|(_, n)| **n)?;
        if n > N1_THRESHOLD {
            tracing::warn!(key = %key, n, "[n+1] '{key}' ran {n}x this tick — collect the set and call Db::insert_rows once");
            return Some((key.clone(), n));
        }
        None
    }

    /// Drain the in-memory source-side write ledger. Called once per tick by the
    /// engine's flush.
    pub fn take_write_ledger(&self) -> Vec<(String, usize)> {
        std::mem::take(&mut *self.write_ledger.borrow_mut())
    }

    /// Clear any stale source-side ledger entries (e.g. after an error aborted a
    /// tick before flush).
    pub fn clear_write_ledger(&self) {
        self.write_ledger.borrow_mut().clear();
    }

    /// Whole-statement DDL / bulk op (e.g. `DELETE FROM t`), counted once.
    /// `rel` names the table the statement chiefly touches; it feeds error
    /// context and the N+1 counter key.
    pub fn exec_on(&self, rel: &str, sql: &str) -> Result<usize> {
        self.bump(rel);
        self.conn.execute(sql, [])
            .map_err(|e| sql_err(rel, sql, e))
    }

    /// Fixpoint executor: one lowered statement per derived rule. Bumps the
    /// statement's own SQL head (distinct per rule) instead of the shared
    /// head rel, so O(rules) legitimate statements against one rel don't
    /// read as a per-row write loop — while a real per-row loop (the same
    /// SQL repeated O(rows) times) still trips the scream. Error context
    /// still names the rel.
    pub fn exec_derived(&self, rel: &str, sql: &str) -> Result<usize> {
        // Key on the FULL SQL: rules lowering into the same head rel share
        // any fixed-length prefix (same INSERT head, different SELECT body),
        // while a per-row loop repeats one statement verbatim.
        self.bump(sql);
        self.conn.prepare_cached(sql).and_then(|mut stmt| stmt.execute([]))
            .map_err(|e| sql_err(rel, sql, e))
    }

    /// Deprecated wrapper: old call sites supply only SQL; the rel is implicit.
    /// Kept until the final burn-down commit removes all remaining callers.
    pub fn exec(&self, sql: &str) -> Result<usize> {
        self.bump(sql);
        self.conn.prepare_cached(sql).and_then(|mut stmt| stmt.execute([]))
            .map_err(|e| sql_err("_exec", sql, e))
    }

    /// One prepared execute with bound params; returns rows-affected.
    pub fn exec_params(&self, rel: &str, sql: &str, params: &[SqlVal]) -> Result<usize> {
        self.bump(rel);
        let refs = to_sql_params(params);
        self.conn.prepare_cached(sql).and_then(|mut stmt| stmt.execute(refs.as_slice()))
            .map_err(|e| sql_err(rel, sql, e))
    }

    /// UPDATE/DELETE … WHERE k IN (<holes>), chunked like query_in_chunks; sums
    /// rows-affected; one logical bump.
    pub fn exec_in_chunks(&self, rel: &str, sql: impl Fn(usize) -> String,
                          head: &[SqlVal], keys: &[SqlVal]) -> Result<usize> {
        if keys.is_empty() { return Ok(0); }
        self.bump(rel);
        let head_params: Vec<rusqlite::types::Value> = head.iter().map(Into::into).collect();
        let chunk_size = PARAM_BUDGET.saturating_sub(head.len()).max(1);
        let mut total = 0usize;
        for chunk in keys.chunks(chunk_size) {
            let sql = sql(chunk.len());
            let mut params: Vec<rusqlite::types::Value> = head_params.clone();
            params.extend(chunk.iter().map(Into::into));
            let refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
            total += self.conn.execute(&sql, refs.as_slice())
                .map_err(|e| sql_err(rel, &sql, e))?;
        }
        Ok(total)
    }

    /// prepare; bind params; query_row. Prepare/query errors -> sql_err(rel, sql).
    /// read-closure errors -> context "row map on {rel}: {head}". Exactly one row
    /// required: QueryReturnedNoRows propagates, context-wrapped like the rest.
    pub fn query_one<T>(&self, rel: &str, sql: &str, params: &[SqlVal],
                        read: impl FnOnce(&Row<'_>) -> Result<T>) -> Result<T> {
        self.bump(&format!("SELECT {rel}"));
        let refs = to_sql_params(params);
        let head = stmt_head(sql);
        let mut stmt = self.conn.prepare_cached(sql)
            .map_err(|e| sql_err(rel, sql, e))?;
        stmt.query_row(refs.as_slice(), |row| {
            map_row(read(row), rel, &head)
        }).map_err(|e| sql_err(rel, sql, e))
    }

    /// Same as query_one; zero-or-one row. Absorbs every `OptionalExtension` site.
    pub fn query_opt<T>(&self, rel: &str, sql: &str, params: &[SqlVal],
                        read: impl FnOnce(&Row<'_>) -> Result<T>) -> Result<Option<T>> {
        self.bump(&format!("SELECT {rel}"));
        let refs = to_sql_params(params);
        let head = stmt_head(sql);
        let mut stmt = self.conn.prepare_cached(sql)
            .map_err(|e| sql_err(rel, sql, e))?;
        match stmt.query_row(refs.as_slice(), |row| {
            map_row(read(row), rel, &head)
        }) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(sql_err(rel, sql, e)),
        }
    }

    /// prepare; query; map each row; collect with `?` — NEVER filter_map(ok).
    pub fn query_rows<T>(&self, rel: &str, sql: &str, params: &[SqlVal],
                         mut read: impl FnMut(&Row<'_>) -> Result<T>) -> Result<Vec<T>> {
        self.bump(&format!("SELECT {rel}"));
        let refs = to_sql_params(params);
        let head = stmt_head(sql);
        let mut stmt = self.conn.prepare_cached(sql)
            .map_err(|e| sql_err(rel, sql, e))?;
        let rows = stmt.query_map(refs.as_slice(), |row| {
            map_row(read(row), rel, &head)
        }).map_err(|e| sql_err(rel, sql, e))?;
        rows.map(|r| r.map_err(|e| sql_err(rel, sql, e))).collect()
    }

    /// Streaming fold for scans that must not materialize a Vec.
    pub fn for_each_row(&self, rel: &str, sql: &str, params: &[SqlVal],
                        mut f: impl FnMut(&Row<'_>) -> Result<()>) -> Result<()> {
        self.bump(&format!("SELECT {rel}"));
        let refs = to_sql_params(params);
        let mut stmt = self.conn.prepare_cached(sql)
            .map_err(|e| sql_err(rel, sql, e))?;
        let head = stmt_head(sql);
        let mut rows = stmt.query(refs.as_slice())
            .map_err(|e| sql_err(rel, sql, e))?;
        while let Some(row) = rows.next().map_err(|e| sql_err(rel, sql, e))? {
            f(row).with_context(|| format!("row map on {rel}: {head}"))?;
        }
        Ok(())
    }

    /// Rows as seam values — the rpc.rs::query_sql / daemon_read.rs::json_rows /
    /// string_rows shape. Callers map each cell with SqlVal::to_json or
    /// to_lossy_string.
    pub fn query_values(&self, rel: &str, sql: &str, params: &[SqlVal])
        -> Result<Vec<Vec<SqlVal>>> {
        self.query_rows(rel, sql, params, |row| {
            let ncol = row.as_ref().column_count();
            let mut cells = Vec::with_capacity(ncol);
            for col_index in 0..ncol {
                cells.push(row.get::<_, SqlVal>(col_index)?);
            }
            Ok(cells)
        })
    }

    /// SELECT … WHERE k IN (<n holes>), chunked so head.len() + n <= PARAM_BUDGET.
    pub fn query_in_chunks<T>(&self, rel: &str, sql: impl Fn(usize) -> String,
                              head: &[SqlVal], keys: &[SqlVal],
                              mut read: impl FnMut(&Row<'_>) -> Result<T>) -> Result<Vec<T>> {
        if keys.is_empty() { return Ok(Vec::new()); }
        self.bump(&format!("SELECT {rel}"));
        let head_params: Vec<rusqlite::types::Value> = head.iter().map(Into::into).collect();
        let chunk_size = PARAM_BUDGET.saturating_sub(head.len()).max(1);
        let mut out = Vec::new();
        for chunk in keys.chunks(chunk_size) {
            let sql = sql(chunk.len());
            let mut params: Vec<rusqlite::types::Value> = head_params.clone();
            params.extend(chunk.iter().map(Into::into));
            let refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
            let mut stmt = self.conn.prepare(&sql)
                .map_err(|e| sql_err(rel, &sql, e))?;
            let head_str = stmt_head(&sql);
            let rows = stmt.query_map(refs.as_slice(), |row| {
                map_row(read(row), rel, &head_str)
            }).map_err(|e| sql_err(rel, &sql, e))?;
            for row in rows { out.push(row.map_err(|e| sql_err(rel, &sql, e))?); }
        }
        Ok(out)
    }

    /// Moves engine/meta.rs::digest_of_query into the seam, byte-identical.
    pub fn digest_rows(&self, rel: &str, sql: &str, params: &[SqlVal]) -> Result<[u8; 32]> {
        let mut acc = [0u8; 32];
        self.for_each_row(rel, sql, params, |row| {
            let mut hasher = blake3::Hasher::new();
            let ncol = row.as_ref().column_count();
            for col_index in 0..ncol {
                let cell: SqlVal = row.get(col_index)?;
                let tag_bytes: &[u8] = match cell {
                    SqlVal::Null => b"n",
                    SqlVal::Int(i) => { hasher.update(b"i"); hasher.update(&i.to_le_bytes()); b"" }
                    SqlVal::Real(r) => { hasher.update(b"r"); hasher.update(&r.to_le_bytes()); b"" }
                    SqlVal::Text(ref s) => { hasher.update(b"t"); hasher.update(s.as_bytes()); b"" }
                    SqlVal::Blob(ref b) => { hasher.update(b"b"); hasher.update(b); b"" }
                };
                hasher.update(tag_bytes);
                hasher.update(&[0x00]);
            }
            let row_hash = hasher.finalize();
            let bytes = row_hash.as_bytes();
            for (i, byte) in acc.iter_mut().enumerate() {
                *byte ^= bytes[i];
            }
            Ok(())
        })?;
        Ok(acc)
    }

    /// Chunked multi-row INSERT INTO table (cols) VALUES … ON CONFLICT(key_cols)
    /// DO UPDATE SET update_cols = excluded.* (empty update_cols -> DO NOTHING).
    pub fn upsert_rows(&self, table: &str, cols: &[&str], key_cols: &[&str],
                       update_cols: &[&str], rows: &[Vec<SqlVal>]) -> Result<usize> {
        if rows.is_empty() { return Ok(0); }
        let ncol = cols.len();
        anyhow::ensure!(ncol > 0, "upsert_rows requires at least one column");
        let collist = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        let key_list = key_cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        let on_conflict = if update_cols.is_empty() {
            "DO NOTHING".to_string()
        } else {
            let updates = update_cols.iter()
                .map(|c| format!("\"{c}\" = excluded.\"{c}\""))
                .collect::<Vec<_>>().join(", ");
            format!("DO UPDATE SET {updates}")
        };
        let key = format!("UPSERT {table}");
        self.bump(&key);
        let chunk_rows = (PARAM_BUDGET / ncol.max(1)).max(1);
        let owns_tx = rows.len() > chunk_rows && self.conn.is_autocommit();
        if owns_tx { self.conn.execute_batch("BEGIN").map_err(|e| sql_err(table, "BEGIN", e))?; }
        let mut total = 0;
        let tuple = format!("({})", vec!["?"; ncol].join(","));
        for chunk in rows.chunks(chunk_rows) {
            let values = vec![tuple.clone(); chunk.len()].join(", ");
            let sql = format!(
                "INSERT INTO {table} ({collist}) VALUES {values} \
                 ON CONFLICT({key_list}) {on_conflict}");
            let params: Vec<rusqlite::types::Value> = chunk.iter().flatten().map(Into::into).collect();
            let refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
            match self.conn.execute(&sql, refs.as_slice()) {
                Ok(n) => total += n,
                Err(e) => {
                    if owns_tx {
                        // @rusqlite-ok: best-effort rollback of helper-owned tx; original error wins.
                        let _ = self.conn.execute_batch("ROLLBACK");
                    }
                    return Err(sql_err(table, &sql, e));
                }
            }
        }
        if owns_tx { self.conn.execute_batch("COMMIT").map_err(|e| sql_err(table, "COMMIT", e))?; }
        Ok(total)
    }

    /// Structural + on-disk stats for a rel's backing table.
    pub fn rel_stats(&self, rel: &str) -> Result<serde_json::Value> {
        let table = crate::lower::tbl(rel);
        let rows: i64 = self.query_one(&table, &format!("SELECT count(*) FROM {table}"), &[], |r| Ok(r.get(0)?))?;
        let mut ncol = 0i64;
        let mut pk: Vec<(i64, String)> = Vec::new();
        let col_info: Vec<(String, i64)> = self.query_rows(
            &table,
            &format!("PRAGMA table_info({table})"),
            &[],
            |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)),
        )?;
        for (name, pk_pos) in col_info {
            if name == "__src" { continue; }
            ncol += 1;
            if pk_pos > 0 { pk.push((pk_pos, name)); }
        }
        pk.sort_by_key(|(pos, _)| *pos);
        let pk: Vec<String> = pk.into_iter().map(|(_, c)| c).collect();
        let indexes: Vec<String> = self.query_rows(
            "sqlite_master",
            "SELECT name FROM sqlite_master WHERE tbl_name = ?1 AND type = 'index' \
             AND sql IS NOT NULL ORDER BY name",
            &[table.as_str().into()],
            |r| Ok(r.get::<_, String>(0)?),
        )?;
        let mut bytes = serde_json::Map::new();
        let obj_bytes = |name: &str| -> Result<Option<i64>> {
            self.query_one("dbstat", "SELECT sum(pgsize) FROM dbstat WHERE name = ?1",
                &[name.into()], |r| Ok(r.get::<_, Option<i64>>(0)?))
        };
        if let Some(b) = obj_bytes(&table)? { bytes.insert(table.clone(), b.into()); }
        let pk_autoindex = format!("sqlite_autoindex_{table}_1");
        if let Some(b) = obj_bytes(&pk_autoindex)? { bytes.insert(pk_autoindex, b.into()); }
        for ix in &indexes {
            if let Some(b) = obj_bytes(ix)? { bytes.insert(ix.clone(), b.into()); }
        }
        Ok(serde_json::json!({
            "rel": rel,
            "rows": rows,
            "ncol": ncol,
            "pk": pk,
            "indexes": indexes,
            "bytes": bytes,
        }))
    }

    /// Counted prepare — wraps `Connection::prepare`. Kept until final burn-down
    /// for not-yet-migrated call sites; new code uses query_*/exec_* instead.
    pub fn prepare(&self, sql: &str) -> Result<rusqlite::Statement<'_>> {
        self.bump(sql);
        Ok(self.conn.prepare(sql)?)
    }

    /// Counted query_row — single-row scalar lookup. Kept until final burn-down.
    pub fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        self.bump(sql);
        Ok(self.conn.query_row(sql, params, f)?)
    }

    /// Counted execute_batch — multi-statement SQL script (DDL). `rel` names the
    /// first object the script creates or drops.
    pub fn execute_batch_on(&self, rel: &str, sql: &str) -> Result<()> {
        self.bump(rel);
        self.conn.execute_batch(sql)
            .map_err(|e| sql_err(rel, sql, e))
    }

    /// Deprecated wrapper: old call sites supply only SQL.
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        self.bump(sql);
        self.conn.execute_batch(sql)
            .map_err(|e| sql_err("_ddl", sql, e))
    }

    /// SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2 via query_opt.
    pub fn column_exists(&self, table: &str, column: &str) -> Result<bool> {
        self.query_opt(
            table,
            "SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2",
            &[table.into(), column.into()],
            |_| Ok(()),
        ).map(|o| o.is_some())
    }

    /// column_exists -> Ok(false); else exec(table, alter_ddl) -> Ok(true).
    pub fn ensure_column(&self, table: &str, column: &str, alter_ddl: &str) -> Result<bool> {
        if self.column_exists(table, column)? { return Ok(false); }
        self.exec_on(table, alter_ddl)?;
        Ok(true)
    }

    /// (name, sql) FROM sqlite_master WHERE tbl_name=?1 AND type='index'
    ///   AND sql IS NOT NULL ORDER BY name.
    pub fn secondary_indexes(&self, table: &str) -> Result<Vec<(String, String)>> {
        self.query_rows(
            "sqlite_master",
            "SELECT name, sql FROM sqlite_master \
             WHERE tbl_name = ?1 AND type = 'index' AND sql IS NOT NULL ORDER BY name",
            &[table.into()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
    }

    /// (name, type) FROM sqlite_master WHERE type IN ('table','view') AND name
    /// LIKE any(patterns).
    pub fn schema_objects(&self, name_like: &[&str]) -> Result<Vec<(String, String)>> {
        if name_like.is_empty() { return Ok(Vec::new()); }
        let mut sql = String::from(
            "SELECT name, type FROM sqlite_master \
             WHERE type IN ('table','view') AND ("
        );
        let mut parts = Vec::new();
        for _ in name_like { parts.push("name LIKE ?".to_string()); }
        sql.push_str(&parts.join(" OR "));
        sql.push_str(") ORDER BY name");
        let params: Vec<SqlVal> = name_like.iter().map(|p| (*p).into()).collect();
        self.query_rows("sqlite_master", &sql, &params, |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
    }

    /// PRAGMA <name> as one i64. PRAGMAs can't bind params; `name` is a
    /// compile-time literal at every call site.
    pub fn pragma_i64(&self, name: &str) -> Result<i64> {
        let sql = format!("PRAGMA {name}");
        self.query_one("_pragma", &sql, &[], |r| Ok(r.get(0)?))
    }

    /// execute_batch("_tx", "BEGIN")
    pub fn begin(&self) -> Result<()> {
        self.execute_batch_on("_tx", "BEGIN")
    }

    /// execute_batch("_tx", "BEGIN IMMEDIATE")
    pub fn begin_immediate(&self) -> Result<()> {
        self.execute_batch_on("_tx", "BEGIN IMMEDIATE")
    }

    /// execute_batch("_tx", "COMMIT")
    pub fn commit(&self) -> Result<()> {
        self.execute_batch_on("_tx", "COMMIT")
    }

    /// execute_batch("_tx", "ROLLBACK")
    pub fn rollback(&self) -> Result<()> {
        self.execute_batch_on("_tx", "ROLLBACK")
    }

    /// If not autocommit, run work() (caller owns outer tx). Else begin_immediate,
    /// commit on Ok, rollback on Err; on panic, rollback then resume_unwind.
    pub fn transact<T>(&self, work: impl FnOnce() -> Result<T>) -> Result<T> {
        if !self.is_autocommit() {
            return work();
        }
        self.begin_immediate()?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        match result {
            Ok(Ok(value)) => {
                self.commit()?;
                Ok(value)
            }
            Ok(Err(error)) => {
                // @rusqlite-ok: rollback failure is unactionable; the original error wins.
                let _ = self.rollback();
                Err(error)
            }
            Err(payload) => {
                // @rusqlite-ok: rollback failure is unactionable; the panic wins.
                let _ = self.rollback();
                std::panic::resume_unwind(payload);
            }
        }
    }

    /// Counted prepare — wraps `Connection::prepare`. Call sites that need a
    /// `Statement` (e.g. multi-row `query_map`) migrate off `conn().prepare(...)`.
    /// Kept until final burn-down.

    /// Whole-table reload: `DELETE`, then bulk-insert `rows`.
    pub fn reload_rel(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        const INDEX_DROP_MIN: usize = 4096;
        tracing::debug!(table, rows = rows.len(), "[rel] wipe + reload");
        self.exec_on(table, &format!("DELETE FROM {table}"))?;
        if rows.len() < INDEX_DROP_MIN {
            return self.insert_rows(table, cols, rows);
        }
        let sidx: Vec<(String, String)> = self.secondary_indexes(table)?;
        for (name, _) in &sidx { self.exec_on(table, &format!("DROP INDEX \"{name}\""))?; }
        let res = self.insert_rows(table, cols, rows);
        for (_, sql) in &sidx { self.exec_on(table, sql)?; }
        res
    }

    /// Insert a set of rows in chunked multi-row `VALUES` statements — ONE logical
    /// op (a few executes for very large N), never one-per-row. `INSERT OR IGNORE`.
    pub fn insert_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        self.insert_rows_keyed(table, &format!("INSERT {table}"), cols, rows)
    }

    /// `insert_rows` with a caller-chosen N+1 counter key. For the rare call
    /// site that legitimately issues O(program) batched set-inserts per tick
    /// (one completion mark per dependency component, crash-ordering
    /// required) and would misread as a per-row loop under the shared
    /// `INSERT {table}` key — the same exemption class as `exec_derived`'s
    /// full-SQL keying. A true per-row loop (the same key repeated O(rows)
    /// times) still trips the scream.
    pub fn insert_rows_keyed(
        &self,
        table: &str,
        bump_key: &str,
        cols: &[&str],
        rows: &[Vec<Value>],
    ) -> Result<usize> {
        if rows.is_empty() { return Ok(0); }
        let ncol = cols.len();
        let collist = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        self.bump(bump_key);
        let chunk_rows = (PARAM_BUDGET / ncol.max(1)).max(1);
        let multi = rows.len() > chunk_rows;
        let owns_tx = multi && self.conn.is_autocommit();
        if owns_tx { self.conn.execute_batch("BEGIN").map_err(|e| sql_err(table, "BEGIN", e))?; }
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
            let refs: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();
            match self.conn.execute(&sql, refs.as_slice()) {
                Ok(n) => total += n,
                Err(e) => {
                    if owns_tx {
                        // @rusqlite-ok: rollback helper-owned tx on chunk failure; original error wins.
                        let _ = self.conn.execute_batch("ROLLBACK");
                    }
                    return Err(sql_err(table, &sql, e));
                }
            }
        }
        if owns_tx { self.conn.execute_batch("COMMIT").map_err(|e| sql_err(table, "COMMIT", e))?; }
        if total > 0 && table != "_write_ledger" {
            let rel = table
                .strip_prefix("rel_")
                .unwrap_or(table)
                .to_string();
            self.write_ledger.borrow_mut().push((rel, total));
        }
        Ok(total)
    }

    /// Drain a `SymSink`'s queued interns into ONE batched `_strings` insert.
    pub fn flush_syms(&self, sink: &mut crate::spine::SymSink) -> Result<usize> {
        use std::collections::BTreeMap;
        let pending = sink.drain();
        if pending.is_empty() { return Ok(0); }
        let mut by_id: BTreeMap<i64, String> = BTreeMap::new();
        for (id, text) in pending {
            let cell = id.sqlite();
            match by_id.entry(cell) {
                std::collections::btree_map::Entry::Vacant(e) => { e.insert(text); }
                std::collections::btree_map::Entry::Occupied(e) => {
                    if *e.get() != text {
                        anyhow::bail!(
                            "StringId collision at intern time: {:?} and {:?} both hash to {cell}",
                            e.get(), text
                        );
                    }
                }
            }
        }
        let rows: Vec<Vec<Value>> = by_id.into_iter()
            .map(|(id, text)| vec![Value::Int(id), Value::Text(text)])
            .collect();
        self.insert_rows("_strings", &["id", "content"], &rows)
    }
}

/// Read-only connection owner for daemon_read. Counted into the process-wide
/// profile hook; not part of per-tick N+1 counting (a read RPC has no tick).
pub struct ReadDb { conn: Connection }

impl ReadDb {
    /// Was db::open_read_only: READ_ONLY|NO_MUTEX flags, 1s busy_timeout, the
    /// same scalar-fn registry (read-only sprf_sym_intern: id of text, no
    /// intern queue).
    pub fn open(path: &str) -> Result<ReadDb> {
        let mut conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_millis(1000))?;
        register_regexp(&conn)?;
        register_split(&conn)?;
        register_string_fns(&conn)?;
        conn.create_scalar_function(
            "sprf_sym_intern",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8,
            |ctx| {
                let Some(text) = ctx.get::<Option<String>>(0)? else { return Ok(None); };
                Ok(Some(crate::spine::StringId::of(&text).sqlite()))
            },
        )?;
        if profiling() { conn.profile(Some(profile_hook)); }
        Ok(ReadDb { conn })
    }

    pub fn query_one<T>(&self, rel: &str, sql: &str, params: &[SqlVal],
                        read: impl FnOnce(&Row<'_>) -> Result<T>) -> Result<T> {
        let refs = to_sql_params(params);
        let head = stmt_head(sql);
        let mut stmt = self.conn.prepare_cached(sql)
            .map_err(|e| sql_err(rel, sql, e))?;
        stmt.query_row(refs.as_slice(), |row| {
            map_row(read(row), rel, &head)
        }).map_err(|e| sql_err(rel, sql, e))
    }

    pub fn query_opt<T>(&self, rel: &str, sql: &str, params: &[SqlVal],
                        read: impl FnOnce(&Row<'_>) -> Result<T>) -> Result<Option<T>> {
        let refs = to_sql_params(params);
        let head = stmt_head(sql);
        let mut stmt = self.conn.prepare_cached(sql)
            .map_err(|e| sql_err(rel, sql, e))?;
        match stmt.query_row(refs.as_slice(), |row| {
            map_row(read(row), rel, &head)
        }) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(sql_err(rel, sql, e)),
        }
    }

    pub fn query_rows<T>(&self, rel: &str, sql: &str, params: &[SqlVal],
                         mut read: impl FnMut(&Row<'_>) -> Result<T>) -> Result<Vec<T>> {
        let refs = to_sql_params(params);
        let head = stmt_head(sql);
        let mut stmt = self.conn.prepare_cached(sql)
            .map_err(|e| sql_err(rel, sql, e))?;
        let rows = stmt.query_map(refs.as_slice(), |row| {
            map_row(read(row), rel, &head)
        }).map_err(|e| sql_err(rel, sql, e))?;
        rows.map(|r| r.map_err(|e| sql_err(rel, sql, e))).collect()
    }

    pub fn query_values(&self, rel: &str, sql: &str, params: &[SqlVal])
        -> Result<Vec<Vec<SqlVal>>> {
        self.query_rows(rel, sql, params, |row| {
            let ncol = row.as_ref().column_count();
            let mut cells = Vec::with_capacity(ncol);
            for col_index in 0..ncol {
                cells.push(row.get::<_, SqlVal>(col_index)?);
            }
            Ok(cells)
        })
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
        // @rusqlite-ok: best-effort checkpoint on drop; WAL cleanup is advisory.
        let _ = self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_budget_reservation_never_exceeds_ceiling() {
        let reserved = AtomicU64::new(0);
        assert_eq!(reserve_process_budget(&reserved, 32, 16), 16);
        assert_eq!(reserve_process_budget(&reserved, 32, 20), 16);
        assert_eq!(reserve_process_budget(&reserved, 32, 1), 0);
        assert_eq!(reserved.load(Ordering::Relaxed), 32);
    }

    /// Fail-pre-fix for the 2026-07-18 poison-job incident: every registered
    /// scalar takes NULL and yields NULL. Before the fix each call aborted the
    /// statement with "Invalid function parameter type Null at index 0",
    /// failing whole ColdExtract jobs into retry loops.
    #[test]
    fn scalar_fns_propagate_null() {
        let db = open(None).unwrap();
        for expr in [
            "sprf_lower(NULL)", "sprf_upper(NULL)", "sprf_lcfirst(NULL)",
            "sprf_ucfirst(NULL)", "sprf_trim(NULL)", "sprf_norm(NULL)",
            "sprf_strip_prefix(NULL, 'p')", "sprf_strip_prefix('s', NULL)",
            "sprf_strip_suffix(NULL, 'p')", "sprf_sym(NULL)",
            "sprf_sym_intern(NULL)", "sprf_lines(NULL)",
            "sprf_replace_re(NULL, 'a', 'b')", "sprf_replace_re('s', NULL, 'b')",
            "sprf_split(NULL, ',', 0)", "sprf_split('a,b', NULL, 0)",
            "NULL REGEXP 'a'", "'a' REGEXP NULL",
        ] {
            let got: Option<String> = db
                .query_one("_scalar", &format!("SELECT {expr}"), &[], |r| Ok(r.get::<_, Option<String>>(0)?))
                .unwrap_or_else(|e| panic!("{expr} errored instead of NULL: {e}"));
            assert_eq!(got, None, "{expr} must be NULL");
        }
    }

    #[test]
    fn sqlite_temp_work_is_file_backed() {
        let db = open(None).unwrap();
        let temp_store: i64 = db.pragma_i64("temp_store").unwrap();
        assert_eq!(temp_store, 1, "TEMP tables and sorts must not consume unbounded heap");
    }

    #[test]
    fn insert_rows_chunks_by_param_budget() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER, b TEXT)").unwrap();
        let rows: Vec<Vec<Value>> = (0..33_000)
            .map(|i| vec![Value::Int(i), Value::Text(format!("r{i}"))])
            .collect();
        db.tick_begin();
        let n = db.insert_rows("t", &["a", "b"], &rows).unwrap();
        assert_eq!(n, 33_000);
        assert!(db.tick_end().is_none(), "chunked insert must not trip the n+1 counter");
        let count: i64 = db.query_one("t", "SELECT COUNT(*) FROM t", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(count, 33_000);
    }

    #[test]
    fn multi_chunk_insert_owns_transaction_when_autocommit() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        let rows: Vec<Vec<Value>> = (0..32_001).map(|i| vec![Value::Int(i)]).collect();

        assert!(db.is_autocommit());
        assert_eq!(db.insert_rows("t", &["a"], &rows).unwrap(), rows.len());
        assert!(db.is_autocommit(), "helper-owned transaction must be committed");
        let count: i64 = db.query_one("t", "SELECT COUNT(*) FROM t", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(count, rows.len() as i64);
    }

    #[test]
    fn multi_chunk_insert_composes_with_begin_immediate() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        let rows: Vec<Vec<Value>> = (0..32_001).map(|i| vec![Value::Int(i)]).collect();

        db.begin_immediate().unwrap();
        assert_eq!(db.insert_rows("t", &["a"], &rows).unwrap(), rows.len());
        assert!(!db.is_autocommit(), "caller must still own the transaction");
        db.commit().unwrap();

        let count: i64 = db.query_one("t", "SELECT COUNT(*) FROM t", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(count, rows.len() as i64);
    }

    #[test]
    fn failed_second_chunk_leaves_outer_transaction_for_caller_rollback() {
        let db = open(None).unwrap();
        db.exec_on("marker", "CREATE TABLE marker (value TEXT)").unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        db.exec_on(
            "t",
            "CREATE TRIGGER fail_second_chunk BEFORE INSERT ON t \
             WHEN NEW.a = 32000 BEGIN SELECT RAISE(ABORT, 'second chunk failed'); END",
        ).unwrap();
        let rows: Vec<Vec<Value>> = (0..32_001).map(|i| vec![Value::Int(i)]).collect();

        db.begin_immediate().unwrap();
        db.exec_on("marker", "INSERT INTO marker VALUES ('before helper')").unwrap();
        let err = db.insert_rows("t", &["a"], &rows).unwrap_err();
        assert!(err.to_string().contains("second chunk failed"));
        assert!(!db.is_autocommit(), "helper must not roll back its caller");
        let marker_in_tx: i64 = db.query_one("marker", "SELECT COUNT(*) FROM marker", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(marker_in_tx, 1);

        db.rollback().unwrap();
        let marker_after: i64 = db.query_one("marker", "SELECT COUNT(*) FROM marker", &[], |r| Ok(r.get(0)?)).unwrap();
        let rows_after: i64 = db.query_one("t", "SELECT COUNT(*) FROM t", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(marker_after, 0, "caller rollback must include its earlier unrelated write");
        assert_eq!(rows_after, 0, "caller rollback must include the helper's first chunk");
    }

    #[test]
    fn insert_rows_never_commits_caller_transaction() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        let rows: Vec<Vec<Value>> = (0..32_001).map(|i| vec![Value::Int(i)]).collect();

        db.begin_immediate().unwrap();
        db.insert_rows("t", &["a"], &rows).unwrap();
        assert!(!db.is_autocommit());
        db.rollback().unwrap();

        let count: i64 = db.query_one("t", "SELECT COUNT(*) FROM t", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(count, 0, "caller rollback must undo every helper-written chunk");
    }

    // Build a table shaped like a df rel: a wide full-row PRIMARY KEY over an
    // int id + text columns, plus a per-column secondary index. `n` rows of the
    // given text width, so dbstat sizes are non-trivial (multi-page).
    fn wide_rel(db: &Db, rel: &str, n: i64, text_width: usize) {
        let table = crate::lower::tbl(rel);
        db.exec_on(&table, &format!(
            "CREATE TABLE {table} (\"id\" INTEGER, \"fn\" TEXT, \"file\" TEXT, \"line\" INTEGER, \
             __src TEXT DEFAULT '', PRIMARY KEY (\"id\", \"fn\", \"file\", \"line\"))")).unwrap();
        db.exec_on(&table, &format!("CREATE INDEX \"idx_{rel}_fn\" ON {table}(\"fn\")")).unwrap();
        let pad = "x".repeat(text_width);
        let rows: Vec<Vec<Value>> = (0..n).map(|i| vec![
            Value::Int(i), Value::Text(format!("fn_{i}_{pad}")),
            Value::Text(format!("src/mod_{}.rs", i % 40)), Value::Int(i % 500),
        ]).collect();
        db.insert_rows(&table, &["id", "fn", "file", "line"], &rows).unwrap();
    }

    #[test]
    fn rel_stats_snapshot_of_schema() {
        let db = open(None).unwrap();
        wide_rel(&db, "widenode", 2000, 32);
        let stats = db.rel_stats("widenode").unwrap();
        insta::assert_json_snapshot!(stats, { ".bytes" => "[dbstat-bytes]" });
    }

    #[test]
    fn full_row_pk_autoindex_exceeds_table_the_fat_pk_smell() {
        let db = open(None).unwrap();
        wide_rel(&db, "fatpk", 4000, 48);
        let stats = db.rel_stats("fatpk").unwrap();
        let bytes = stats["bytes"].as_object().unwrap();
        let table = bytes["rel_fatpk"].as_i64().unwrap();
        let pk = bytes["sqlite_autoindex_rel_fatpk_1"].as_i64().unwrap();
        assert!(pk >= table,
            "a full-row PK autoindex duplicates every column, so it should be >= the table heap; \
             pk={pk} table={table} (dbstat)");
    }

    #[test]
    fn reload_rel_keeps_secondary_indexes() {
        let db = open(None).unwrap();
        wide_rel(&db, "reloadme", 1, 4);
        let table = crate::lower::tbl("reloadme");
        let before = db.rel_stats("reloadme").unwrap()["indexes"].clone();
        let pad = "y".repeat(16);
        let rows: Vec<Vec<Value>> = (0..5000).map(|i| vec![
            Value::Int(i), Value::Text(format!("fn_{i}_{pad}")),
            Value::Text("src/x.rs".into()), Value::Int(i),
        ]).collect();
        db.reload_rel(&table, &["id", "fn", "file", "line"], &rows).unwrap();
        let after = db.rel_stats("reloadme").unwrap()["indexes"].clone();
        assert_eq!(before, after, "secondary indexes must survive a reload_rel drop/rebuild");
        let count: i64 = db.query_one(&table, &format!("SELECT count(*) FROM {table}"), &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(count, 5000);
    }

    #[test]
    fn insert_rows_or_ignore_holds_across_chunks() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        let mut rows: Vec<Vec<Value>> = (0..40_000).map(|i| vec![Value::Int(i)]).collect();
        rows.push(vec![Value::Int(7)]);
        let n = db.insert_rows("t", &["a"], &rows).unwrap();
        assert_eq!(n, 40_000, "duplicate ignored, everything else lands");
        let count: i64 = db.query_one("t", "SELECT COUNT(*) FROM t", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(count, 40_000);
    }

    #[test]
    fn bundled_sqlite_accepts_order_by_inside_aggregate() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (g TEXT, k TEXT, v INTEGER)").unwrap();
        let rows = vec![
            vec![Value::Text("a".into()), Value::Text("z".into()), Value::Int(1)],
            vec![Value::Text("a".into()), Value::Text("x".into()), Value::Int(2)],
            vec![Value::Text("a".into()), Value::Text("y".into()), Value::Int(3)],
        ];
        db.insert_rows("t", &["g", "k", "v"], &rows).unwrap();
        let arr: String = db.query_one("t",
            "SELECT json_group_array(k ORDER BY k) FROM t GROUP BY g", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(arr, r#"["x","y","z"]"#);
        let obj: String = db.query_one("t",
            "SELECT json_group_object(k, v ORDER BY k) FROM t GROUP BY g", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(obj, r#"{"x":2,"y":3,"z":1}"#);
    }

    #[test]
    fn prepare_and_query_map_works() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER, b TEXT)").unwrap();
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
        db.exec_on("t", "CREATE TABLE t (a INTEGER)").unwrap();
        let rows: Vec<Vec<Value>> = (0..7).map(|i| vec![Value::Int(i)]).collect();
        db.insert_rows("t", &["a"], &rows).unwrap();
        db.tick_begin();
        let count: i64 = db.query_one("t", "SELECT COUNT(*) FROM t", &[], |r| Ok(r.get::<_, i64>(0)?)).unwrap();
        assert!(db.tick_end().is_none());
        assert_eq!(count, 7);
    }

    #[test]
    fn execute_batch_runs_multi_statement_ddl() {
        let db = open(None).unwrap();
        db.tick_begin();
        db.execute_batch_on(
            "_ddl",
            "CREATE TABLE a (x INTEGER); CREATE TABLE b (y TEXT);",
        )
        .unwrap();
        assert!(db.tick_end().is_none());
        let na: i64 = db.query_one("a", "SELECT COUNT(*) FROM a", &[], |r| Ok(r.get(0)?)).unwrap();
        let nb: i64 = db.query_one("b", "SELECT COUNT(*) FROM b", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(na, 0);
        assert_eq!(nb, 0);
    }

    #[test]
    fn sql_val_round_trip_and_conversions() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (i INTEGER, r REAL, t TEXT, b BLOB, n INTEGER)").unwrap();
        db.exec_params("t",
            "INSERT INTO t VALUES (?, ?, ?, ?, ?)",
            &[42i64.into(), 3.14.into(), "hello".into(), vec![1u8, 2, 3].into(), SqlVal::Null],
        ).unwrap();

        let vals = db.query_values("t", "SELECT i, r, t, b, n FROM t", &[]).unwrap();
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0], vec![
            SqlVal::Int(42),
            SqlVal::Real(3.14),
            SqlVal::Text("hello".into()),
            SqlVal::Blob(vec![1, 2, 3]),
            SqlVal::Null,
        ]);

        let one: i64 = db.query_one("t", "SELECT i FROM t", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(one, 42);

        let opt: Option<i64> = db.query_opt("t", "SELECT i FROM t WHERE i = ?", &[99i64.into()], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(opt, None);

        let rows: Vec<i64> = db.query_rows("t", "SELECT i FROM t ORDER BY i", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(rows, vec![42]);

        let mut sum = 0i64;
        db.for_each_row("t", "SELECT i FROM t", &[], |r| { sum += r.get::<_, i64>(0)?; Ok(()) }).unwrap();
        assert_eq!(sum, 42);
    }

    #[test]
    fn query_opt_absorbs_optional_extension() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER)").unwrap();
        let present: Option<i64> = db.query_opt("t", "SELECT a FROM t WHERE a = ?", &[1i64.into()], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(present, None);
        db.exec_params("t", "INSERT INTO t VALUES (?)", &[1i64.into()]).unwrap();
        let present: Option<i64> = db.query_opt("t", "SELECT a FROM t WHERE a = ?", &[1i64.into()], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(present, Some(1));
    }

    #[test]
    fn exec_in_chunks_sums_affected_rows() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (k INTEGER PRIMARY KEY, v INTEGER)").unwrap();
        let keys: Vec<SqlVal> = (0..10).map(|i| SqlVal::Int(i)).collect();
        for key in &keys { db.exec_params("t", "INSERT INTO t VALUES (?, 0)", &[key.clone()]).unwrap(); }
        let affected = db.exec_in_chunks(
            "t",
            |n| format!("UPDATE t SET v = 1 WHERE k IN ({})", holes(n)),
            &[],
            &keys,
        ).unwrap();
        assert_eq!(affected, 10);
        let count: i64 = db.query_one("t", "SELECT COUNT(*) FROM t WHERE v = 1", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(count, 10);
    }

    #[test]
    fn query_in_chunks_collects_chunked_results() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (k INTEGER PRIMARY KEY)").unwrap();
        let keys: Vec<SqlVal> = (0..10).map(|i| SqlVal::Int(i)).collect();
        for key in &keys { db.exec_params("t", "INSERT INTO t VALUES (?)", &[key.clone()]).unwrap(); }
        let results: Vec<i64> = db.query_in_chunks(
            "t",
            |n| format!("SELECT k FROM t WHERE k IN ({}) ORDER BY k", holes(n)),
            &[],
            &keys,
            |r| Ok(r.get(0)?),
        ).unwrap();
        assert_eq!(results, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn upsert_rows_insert_and_update() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (k INTEGER PRIMARY KEY, v INTEGER)").unwrap();
        let rows: Vec<Vec<SqlVal>> = vec![
            vec![1i64.into(), 10i64.into()],
            vec![2i64.into(), 20i64.into()],
        ];
        assert_eq!(db.upsert_rows("t", &["k", "v"], &["k"], &["v"], &rows).unwrap(), 2);
        let rows2: Vec<Vec<SqlVal>> = vec![vec![1i64.into(), 100i64.into()]];
        assert_eq!(db.upsert_rows("t", &["k", "v"], &["k"], &["v"], &rows2).unwrap(), 1);
        let v: i64 = db.query_one("t", "SELECT v FROM t WHERE k = 1", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(v, 100);
    }

    #[test]
    fn ensure_column_idempotent() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER)").unwrap();
        assert!(db.ensure_column("t", "b", "ALTER TABLE t ADD COLUMN b TEXT").unwrap());
        assert!(!db.ensure_column("t", "b", "ALTER TABLE t ADD COLUMN b TEXT").unwrap());
        let cols: i64 = db.query_one("t", "SELECT COUNT(*) FROM pragma_table_info('t')", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(cols, 2);
    }

    #[test]
    fn column_exists_and_secondary_indexes() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER, b TEXT)").unwrap();
        db.exec_on("t", "CREATE INDEX idx_t_b ON t(b)").unwrap();
        assert!(db.column_exists("t", "a").unwrap());
        assert!(!db.column_exists("t", "c").unwrap());
        let idxs = db.secondary_indexes("t").unwrap();
        assert_eq!(idxs.len(), 1);
        assert_eq!(idxs[0].0, "idx_t_b");
    }

    #[test]
    fn schema_objects_filters_by_like_patterns() {
        let db = open(None).unwrap();
        db.exec_on("a", "CREATE TABLE alpha (x INTEGER)").unwrap();
        db.exec_on("b", "CREATE TABLE beta (x INTEGER)").unwrap();
        db.exec_on("g", "CREATE VIEW gamma AS SELECT * FROM alpha").unwrap();
        let objs = db.schema_objects(&["alph%", "gamm%"]).unwrap();
        assert_eq!(objs.len(), 2);
    }

    #[test]
    fn pragma_i64_reads_pragmas() {
        let db = open(None).unwrap();
        let user_version = db.pragma_i64("user_version").unwrap();
        assert_eq!(user_version, 0);
    }

    #[test]
    fn digest_rows_xor_fold() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER, b TEXT)").unwrap();
        db.exec_params("t", "INSERT INTO t VALUES (?, ?)", &[1i64.into(), "x".into()]).unwrap();
        db.exec_params("t", "INSERT INTO t VALUES (?, ?)", &[2i64.into(), "y".into()]).unwrap();
        let d1 = db.digest_rows("t", "SELECT a, b FROM t ORDER BY a", &[]).unwrap();
        let d2 = db.digest_rows("t", "SELECT a, b FROM t ORDER BY a", &[]).unwrap();
        assert_eq!(d1, d2);
        // Different content gives a different digest.
        db.exec_params("t", "INSERT INTO t VALUES (?, ?)", &[3i64.into(), "z".into()]).unwrap();
        let d3 = db.digest_rows("t", "SELECT a, b FROM t ORDER BY a", &[]).unwrap();
        assert_ne!(d1, d3);
    }

    #[test]
    fn transact_commits_ok_and_rolls_back_err() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        let got = db.transact(|| {
            db.exec_params("t", "INSERT INTO t VALUES (?)", &[1i64.into()])?;
            Ok(42i64)
        }).unwrap();
        assert_eq!(got, 42);
        assert!(db.is_autocommit());
        let count: i64 = db.query_one("t", "SELECT COUNT(*) FROM t", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(count, 1);

        let err = db.transact(|| -> Result<()> {
            db.exec_params("t", "INSERT INTO t VALUES (?)", &[2i64.into()])?;
            anyhow::bail!("boom")
        }).unwrap_err();
        assert!(err.to_string().contains("boom"));
        assert!(db.is_autocommit());
        let count: i64 = db.query_one("t", "SELECT COUNT(*) FROM t", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(count, 1);
    }

    /// Fail-pre-fix for the 2026-07-18 poison-job incident class: a failing
    /// statement must surface the table name AND the statement head.
    #[test]
    fn sql_error_includes_rel_and_statement_head() {
        let db = open(None).unwrap();
        db.exec_on("t", "CREATE TABLE t (a INTEGER NOT NULL)").unwrap();
        let err = db.exec_params("t", "INSERT INTO t VALUES (?)", &[SqlVal::Null]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("t"), "error must name the table: {msg}");
        assert!(msg.contains("INSERT INTO t VALUES"), "error must include statement head: {msg}");
    }

    #[test]
    fn read_db_open_and_query() {
        let path = format!("/tmp/sprefa_test_read_db_{}.db", std::process::id());
        let _ = std::fs::remove_file(&path);
        {
            let db = open(Some(&path)).unwrap();
            db.exec_on("t", "CREATE TABLE t (a INTEGER)").unwrap();
            db.exec_params("t", "INSERT INTO t VALUES (?)", &[7i64.into()]).unwrap();
        }
        let rdb = ReadDb::open(&path).unwrap();
        let a: i64 = rdb.query_one("t", "SELECT a FROM t", &[], |r| Ok(r.get(0)?)).unwrap();
        assert_eq!(a, 7);
        let vals = rdb.query_values("t", "SELECT a FROM t", &[]).unwrap();
        assert_eq!(vals[0][0], SqlVal::Int(7));
        let _ = std::fs::remove_file(&path);
    }
}
