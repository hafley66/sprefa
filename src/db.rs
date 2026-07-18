use anyhow::Result;
use rusqlite::Connection;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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
    // busy_timeout: a hook `--check` and a resident `--lsp` share .dl/cache.db
    // across processes; a write collision should wait, not fail "locked".
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

impl Db {
    /// Escape hatch for not-yet-migrated call sites. Uncounted — burn these down.
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
    pub fn exec(&self, sql: &str) -> Result<usize> {
        self.bump(sql);
        Ok(self.conn.execute(sql, [])?)
    }

    /// Structural + on-disk stats for a rel's backing table, straight from
    /// SQLite's own introspection — the "where did the bytes and the write time
    /// go" surface behind a slow whole-table refresh:
    ///   - `rows`, `ncol`, `pk` (PRIMARY KEY columns), `indexes` (secondary),
    ///   - `bytes`: per-object on-disk size (the table, its PK autoindex, and
    ///     each secondary index) from the `dbstat` vtab, in bytes.
    /// A full-row PK shows here as an autoindex LARGER than the table (it
    /// duplicates every column); a fat un-interned text column shows as an
    /// oversized per-column index. Snapshot this (see the perf tests) and a
    /// regression in table size / index count / PK shape is a snapshot diff, not
    /// a hand-run `sqlite3` session. `dbstat` sizes are best-effort: the map is
    /// empty if the vtab is unavailable in this build.
    pub fn rel_stats(&self, rel: &str) -> Result<serde_json::Value> {
        let table = crate::lower::tbl(rel);
        let rows: i64 = self.conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap_or(0);
        let mut ncol = 0i64;
        let mut pk: Vec<(i64, String)> = Vec::new();
        {
            let mut s = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
            for row in s.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)))?.flatten() {
                if row.0 == "__src" { continue; }
                ncol += 1;
                if row.1 > 0 { pk.push((row.1, row.0)); }
            }
        }
        pk.sort_by_key(|(pos, _)| *pos);
        let pk: Vec<String> = pk.into_iter().map(|(_, c)| c).collect();
        let mut indexes: Vec<String> = Vec::new();
        {
            let mut s = self.conn.prepare(
                "SELECT name FROM sqlite_master WHERE tbl_name = ?1 AND type = 'index' \
                 AND sql IS NOT NULL ORDER BY name")?;
            for n in s.query_map([table.as_str()], |r| r.get::<_, String>(0))?.flatten() {
                indexes.push(n);
            }
        }
        // Per-object bytes from dbstat, keyed by object name (table + every index,
        // including the PK autoindex). Best-effort: skip silently if dbstat is
        // absent. One grouped scan over the objects belonging to this table.
        let mut bytes = serde_json::Map::new();
        let obj_bytes = |name: &str| -> Option<i64> {
            self.conn.query_row(
                "SELECT sum(pgsize) FROM dbstat WHERE name = ?1",
                [name], |r| r.get::<_, Option<i64>>(0)).ok().flatten()
        };
        if let Some(b) = obj_bytes(&table) { bytes.insert(table.clone(), b.into()); }
        // The PK autoindex has no sqlite_master.sql; its name is the conventional
        // sqlite_autoindex_<table>_1 (present only when the table has a PK).
        let pk_autoindex = format!("sqlite_autoindex_{table}_1");
        if let Some(b) = obj_bytes(&pk_autoindex) { bytes.insert(pk_autoindex, b.into()); }
        for ix in &indexes {
            if let Some(b) = obj_bytes(ix) { bytes.insert(ix.clone(), b.into()); }
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
    /// Whole-table reload: `DELETE`, then bulk-insert `rows`. For a large load,
    /// the secondary indexes are DROPPED first and rebuilt in one pass after —
    /// maintaining a table's index B-trees on every one of N inserts dominates a
    /// bulk load (df_node: 154k rows across 5 secondary indexes + a wide
    /// composite PK was ~925ms of per-row upkeep). A bulk index build over the
    /// finished table is far cheaper. The PRIMARY KEY autoindex (its
    /// `sqlite_master.sql IS NULL`) is KEPT, so `INSERT OR IGNORE` still dedups
    /// during the load. Rebuild runs BEFORE any insert error propagates, so a
    /// failure never strands the table without its indexes.
    ///
    /// `INDEX_DROP_MIN` gates the drop/rebuild: below it the two `sqlite_master`
    /// reads + drop/create round-trips cost more than they save, so a small rel
    /// takes the plain `DELETE`+insert path.
    pub fn reload_rel(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        const INDEX_DROP_MIN: usize = 4096;
        tracing::debug!(table, rows = rows.len(), "[rel] wipe + reload");
        self.exec(&format!("DELETE FROM {table}"))?;
        if rows.len() < INDEX_DROP_MIN {
            return self.insert_rows(table, cols, rows);
        }
        // Secondary indexes only: the PK autoindex has a NULL `sql` and is skipped.
        let sidx: Vec<(String, String)> = {
            let mut stmt = self.conn.prepare(
                "SELECT name, sql FROM sqlite_master \
                 WHERE tbl_name = ?1 AND type = 'index' AND sql IS NOT NULL")?;
            let found = stmt.query_map([table], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            found.flatten().collect()
        };
        for (name, _) in &sidx { self.exec(&format!("DROP INDEX \"{name}\""))?; }
        let res = self.insert_rows(table, cols, rows);
        for (_, sql) in &sidx { self.exec(sql)?; }
        res
    }

    pub fn insert_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        if rows.is_empty() { return Ok(0); }
        let ncol = cols.len();
        let collist = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        let key = format!("INSERT {table}");
        self.bump(&key);
        let chunk_rows = (PARAM_BUDGET / ncol.max(1)).max(1);
        let multi = rows.len() > chunk_rows;
        // A bulk insert is atomic on its own, but it must compose with a wider
        // semantic-generation transaction. Only open (and therefore only
        // commit or roll back) the transaction when this helper owns it.
        let owns_tx = multi && self.conn.is_autocommit();
        if owns_tx { self.conn.execute_batch("BEGIN")?; }
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
                    if owns_tx { let _ = self.conn.execute_batch("ROLLBACK"); }
                    return Err(e.into());
                }
            }
        }
        if owns_tx { self.conn.execute_batch("COMMIT")?; }
        if total > 0 && table != "_write_ledger" {
            // Record the logical rel name: `rel_<name>` -> `<name>`; raw internal
            // tables like `_file` pass through verbatim.
            let rel = table
                .strip_prefix("rel_")
                .unwrap_or(table)
                .to_string();
            self.write_ledger.borrow_mut().push((rel, total));
        }
        Ok(total)
    }

    /// Drain a `SymSink`'s queued interns into ONE batched `_strings` insert —
    /// the turnkey emit-side API every dataflow/spine refresh routes through,
    /// so no call site open-codes `StringId::of(text).sqlite()` + a bespoke
    /// insert. Collision guard: two different texts hashing to the same id
    /// within this ONE drain is a loud bail (a silent 64-bit collision would
    /// corrupt every join keyed on the id); a collision across separate
    /// flushes is accepted as negligible at 64-bit and resolved by
    /// `INSERT OR IGNORE` (first writer wins).
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
        let conn = db.conn();
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
            let got: Option<String> = conn
                .query_row(&format!("SELECT {expr}"), [], |r| r.get(0))
                .unwrap_or_else(|e| panic!("{expr} errored instead of NULL: {e}"));
            assert_eq!(got, None, "{expr} must be NULL");
        }
    }

    #[test]
    fn sqlite_temp_work_is_file_backed() {
        let db = open(None).unwrap();
        let temp_store: i64 = db.conn().query_row("PRAGMA temp_store", [], |r| r.get(0)).unwrap();
        assert_eq!(temp_store, 1, "TEMP tables and sorts must not consume unbounded heap");
    }

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
    fn multi_chunk_insert_owns_transaction_when_autocommit() {
        let db = open(None).unwrap();
        db.exec("CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        let rows: Vec<Vec<Value>> = (0..32_001).map(|i| vec![Value::Int(i)]).collect();

        assert!(db.conn().is_autocommit());
        assert_eq!(db.insert_rows("t", &["a"], &rows).unwrap(), rows.len());
        assert!(db.conn().is_autocommit(), "helper-owned transaction must be committed");
        let count: i64 = db.conn().query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(count, rows.len() as i64);
    }

    #[test]
    fn multi_chunk_insert_composes_with_begin_immediate() {
        let db = open(None).unwrap();
        db.exec("CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        let rows: Vec<Vec<Value>> = (0..32_001).map(|i| vec![Value::Int(i)]).collect();

        db.conn().execute_batch("BEGIN IMMEDIATE").unwrap();
        assert_eq!(db.insert_rows("t", &["a"], &rows).unwrap(), rows.len());
        assert!(!db.conn().is_autocommit(), "caller must still own the transaction");
        db.conn().execute_batch("COMMIT").unwrap();

        let count: i64 = db.conn().query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(count, rows.len() as i64);
    }

    #[test]
    fn failed_second_chunk_leaves_outer_transaction_for_caller_rollback() {
        let db = open(None).unwrap();
        db.exec("CREATE TABLE marker (value TEXT)").unwrap();
        db.exec("CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        db.exec(
            "CREATE TRIGGER fail_second_chunk BEFORE INSERT ON t \
             WHEN NEW.a = 32000 BEGIN SELECT RAISE(ABORT, 'second chunk failed'); END",
        ).unwrap();
        let rows: Vec<Vec<Value>> = (0..32_001).map(|i| vec![Value::Int(i)]).collect();

        db.conn().execute_batch("BEGIN IMMEDIATE").unwrap();
        db.exec("INSERT INTO marker VALUES ('before helper')").unwrap();
        let err = db.insert_rows("t", &["a"], &rows).unwrap_err();
        assert!(err.to_string().contains("second chunk failed"));
        assert!(!db.conn().is_autocommit(), "helper must not roll back its caller");
        let marker_in_tx: i64 = db.conn().query_row("SELECT COUNT(*) FROM marker", [], |r| r.get(0)).unwrap();
        assert_eq!(marker_in_tx, 1);

        db.conn().execute_batch("ROLLBACK").unwrap();
        let marker_after: i64 = db.conn().query_row("SELECT COUNT(*) FROM marker", [], |r| r.get(0)).unwrap();
        let rows_after: i64 = db.conn().query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(marker_after, 0, "caller rollback must include its earlier unrelated write");
        assert_eq!(rows_after, 0, "caller rollback must include the helper's first chunk");
    }

    #[test]
    fn insert_rows_never_commits_caller_transaction() {
        let db = open(None).unwrap();
        db.exec("CREATE TABLE t (a INTEGER PRIMARY KEY)").unwrap();
        let rows: Vec<Vec<Value>> = (0..32_001).map(|i| vec![Value::Int(i)]).collect();

        db.conn().execute_batch("BEGIN IMMEDIATE").unwrap();
        db.insert_rows("t", &["a"], &rows).unwrap();
        assert!(!db.conn().is_autocommit());
        db.conn().execute_batch("ROLLBACK").unwrap();

        let count: i64 = db.conn().query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "caller rollback must undo every helper-written chunk");
    }

    // Build a table shaped like a df rel: a wide full-row PRIMARY KEY over an
    // int id + text columns, plus a per-column secondary index. `n` rows of the
    // given text width, so dbstat sizes are non-trivial (multi-page).
    fn wide_rel(db: &Db, rel: &str, n: i64, text_width: usize) {
        let table = crate::lower::tbl(rel);
        db.exec(&format!(
            "CREATE TABLE {table} (\"id\" INTEGER, \"fn\" TEXT, \"file\" TEXT, \"line\" INTEGER, \
             __src TEXT DEFAULT '', PRIMARY KEY (\"id\", \"fn\", \"file\", \"line\"))")).unwrap();
        db.exec(&format!("CREATE INDEX \"idx_{rel}_fn\" ON {table}(\"fn\")")).unwrap();
        let pad = "x".repeat(text_width);
        let rows: Vec<Vec<Value>> = (0..n).map(|i| vec![
            Value::Int(i), Value::Text(format!("fn_{i}_{pad}")),
            Value::Text(format!("src/mod_{}.rs", i % 40)), Value::Int(i % 500),
        ]).collect();
        db.insert_rows(&table, &["id", "fn", "file", "line"], &rows).unwrap();
    }

    // rel_stats reports the schema (rows, ncol, PK columns, secondary indexes).
    // The dbstat byte map is redacted in the snapshot (page sizes drift across
    // SQLite builds); the STRUCTURE is the regression guard.
    #[test]
    fn rel_stats_snapshot_of_schema() {
        let db = open(None).unwrap();
        wide_rel(&db, "widenode", 2000, 32);
        let stats = db.rel_stats("widenode").unwrap();
        insta::assert_json_snapshot!(stats, { ".bytes" => "[dbstat-bytes]" });
    }

    // The full-row PK "smell": an autoindex that duplicates every column ends up
    // LARGER than the table heap itself. This is the exact regression that made
    // df_node's write slow; assert dbstat catches it so a future fat-PK rel trips
    // a test instead of a production spike. (Needs enough rows to exceed one
    // page; 4000 wide rows do.)
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

    // Correctness guard for reload_rel's drop/rebuild path: after a large reload,
    // every secondary index the table started with is still present (and usable),
    // never left dropped.
    #[test]
    fn reload_rel_keeps_secondary_indexes() {
        let db = open(None).unwrap();
        wide_rel(&db, "reloadme", 1, 4); // create shape + its idx
        let table = crate::lower::tbl("reloadme");
        let before = db.rel_stats("reloadme").unwrap()["indexes"].clone();
        // Reload above the INDEX_DROP_MIN threshold so the drop/rebuild path runs.
        let pad = "y".repeat(16);
        let rows: Vec<Vec<Value>> = (0..5000).map(|i| vec![
            Value::Int(i), Value::Text(format!("fn_{i}_{pad}")),
            Value::Text("src/x.rs".into()), Value::Int(i),
        ]).collect();
        db.reload_rel(&table, &["id", "fn", "file", "line"], &rows).unwrap();
        let after = db.rel_stats("reloadme").unwrap()["indexes"].clone();
        assert_eq!(before, after, "secondary indexes must survive a reload_rel drop/rebuild");
        let count: i64 = db.conn().query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap();
        assert_eq!(count, 5000);
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
