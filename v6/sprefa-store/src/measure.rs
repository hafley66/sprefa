//! The ONE measurement harness: plain `tracing`, collected like normal people.
//!
//! Instrument code with run-of-the-mill tracing — `info_span!("phase")` for the
//! code that ran (and its timing), `info!(target: "measure", rss_kb = …)` for a
//! metric sample. [`collect`] runs a workload under one small
//! `tracing_subscriber::Layer` and hands back every span + event as [`Record`]s.
//! [`run_cell`] is the perf-sweep driver built on it: it opens a store, runs a
//! build phase and an op phase, and appends one CSV row of the collected
//! metrics. No `dlsym`, no sqlite3-CLI shell-out, no hand-plumbed sensor structs.

use std::collections::BTreeMap;
use std::future::Future;
use std::io::Write;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use libsqlite3_sys as ffi;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr};
use std::ffi::{c_int, c_void, CStr};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

use crate::relstore::{stamp, Layout, RelStore};

// ============================ the collector ============================
// (plain tracing → Vec<Record>; the one collection mechanism)

/// A span closing (code that ran + how long) or an event (a metric sample).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Span,
    Event,
}

/// One collected tracing record.
#[derive(Clone, Debug)]
pub struct Record {
    pub kind: Kind,
    pub target: String,
    pub name: String,
    pub fields: BTreeMap<String, String>,
    pub elapsed_ns: u64,
}

impl Record {
    /// Parse a field as i64 (metrics are integers).
    pub fn i64(&self, key: &str) -> Option<i64> {
        self.fields.get(key).and_then(|value| value.parse().ok())
    }
    /// Parse a field as f64.
    pub fn f64(&self, key: &str) -> Option<f64> {
        self.fields.get(key).and_then(|value| value.parse().ok())
    }
}

#[derive(Default)]
struct FieldVisitor(BTreeMap<String, String>);

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_string(), format!("{value:?}"));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

struct SpanState {
    started: Instant,
    fields: BTreeMap<String, String>,
}

// One PROCESS-GLOBAL collecting subscriber. Instrumented code runs on more than
// one thread — the block_on thread emits rss/disk/spans, but sqlx runs SQLite on
// its own worker thread where the sqlite3 PROFILE callback fires. A thread-local
// subscriber would miss that trace, so the subscriber is global (captures every
// thread) and `collect` swaps a fresh sink in for the duration of one workload,
// serialized by a lock so concurrent runs never cross-contaminate.
static COLLECT_SINK: Mutex<Option<Arc<Mutex<Vec<Record>>>>> = Mutex::new(None);
static COLLECT_LOCK: Mutex<()> = Mutex::new(());
static INSTALL: std::sync::Once = std::sync::Once::new();

fn push_record(record: Record) {
    if let Some(sink) = COLLECT_SINK.lock().unwrap().as_ref() {
        sink.lock().unwrap().push(record);
    }
}

struct CollectLayer;

impl<S> Layer<S> for CollectLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = FieldVisitor::default();
            attrs.record(&mut visitor);
            span.extensions_mut().insert(SpanState {
                started: Instant::now(),
                fields: visitor.0,
            });
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        push_record(Record {
            kind: Kind::Event,
            target: event.metadata().target().to_string(),
            name: event.metadata().name().to_string(),
            fields: visitor.0,
            elapsed_ns: 0,
        });
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            let extensions = span.extensions();
            let (elapsed_ns, fields) = match extensions.get::<SpanState>() {
                Some(state) => (state.started.elapsed().as_nanos() as u64, state.fields.clone()),
                None => (0, BTreeMap::new()),
            };
            push_record(Record {
                kind: Kind::Span,
                target: span.metadata().target().to_string(),
                name: span.name().to_string(),
                fields,
                elapsed_ns,
            });
        }
    }
}

/// Run `body` under the global collecting subscriber and return its result plus
/// every span/event emitted on ANY thread during it (including SQLite PROFILE
/// events from sqlx's worker thread). Serialized: concurrent calls run one at a
/// time so their records never mix.
pub fn collect<T>(body: impl FnOnce() -> T) -> (T, Vec<Record>) {
    INSTALL.call_once(|| {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(CollectLayer),
        );
    });
    let _run = COLLECT_LOCK.lock().unwrap();
    let sink = Arc::new(Mutex::new(Vec::new()));
    *COLLECT_SINK.lock().unwrap() = Some(sink.clone());
    let result = body();
    *COLLECT_SINK.lock().unwrap() = None;
    let records = std::mem::take(&mut *sink.lock().unwrap());
    (result, records)
}

// ============================ sensors ============================
// Standard syscalls, emitted as tracing events. Not fancy — this is just how
// you read RSS / disk / page-faults on this OS.

/// Peak resident set, KiB. `getrusage.ru_maxrss` (bytes on macOS, KiB on Linux).
fn peak_rss_kb() -> i64 {
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        if cfg!(target_os = "linux") {
            usage.ru_maxrss as i64
        } else {
            usage.ru_maxrss as i64 / 1024
        }
    }
}

/// Major page faults so far — the mmap/page-cache-miss signal. `ru_majflt`.
fn major_faults() -> i64 {
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage.ru_majflt as i64
    }
}

/// Bytes read/written to disk by this process. macOS `proc_pid_rusage`.
fn diskio() -> (i64, i64) {
    #[cfg(target_os = "macos")]
    unsafe {
        let mut usage: libc::rusage_info_v2 = std::mem::zeroed();
        let result = libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            &mut usage as *mut _ as *mut libc::rusage_info_t,
        );
        if result == 0 {
            return (
                usage.ri_diskio_bytesread as i64,
                usage.ri_diskio_byteswritten as i64,
            );
        }
    }
    (0, 0)
}

// ============================ SQLite x-ray ============================
// SQLite's OWN tracing, reached through sqlx's raw handle. sqlite3_trace_v2 in
// PROFILE mode fires per finished statement with the expanded SQL + nanosecond
// runtime; sqlite3_db_status reads page-cache hit/miss. Both emit `target:
// "sqlite"` events, collected by `collect`. This is how you SEE an N+1: N
// identical statements show up in the trace, and `stmt` count spikes.

/// PROFILE callback: `int(unsigned, void*, void* stmt, void* nanos)`.
unsafe extern "C" fn profile_callback(
    _mask: u32,
    _ctx: *mut c_void,
    stmt: *mut c_void,
    elapsed: *mut c_void,
) -> c_int {
    let nanos = if elapsed.is_null() {
        0
    } else {
        *(elapsed as *const i64)
    };
    let expanded = ffi::sqlite3_expanded_sql(stmt as *mut ffi::sqlite3_stmt);
    let sql = if expanded.is_null() {
        String::from("<no sql>")
    } else {
        let text = CStr::from_ptr(expanded).to_string_lossy().into_owned();
        ffi::sqlite3_free(expanded as *mut c_void);
        text
    };
    tracing::info!(target: "sqlite", sql = %sql, elapsed_ns = nanos, "statement");
    0
}

/// Install the PROFILE trace on the store's pinned SQLite connection so every
/// executed statement emits a `target: "sqlite"` event. Handle chain (all public
/// in sqlx 0.9 / sea-orm 2.0): `get_sqlite_connection_pool -> acquire ->
/// lock_handle -> as_raw_handle`. Store pins one connection (min=max=1), so the
/// callback persists for its life.
pub async fn install_sqlite_trace(db: &DatabaseConnection) -> Result<(), DbErr> {
    let pool = db.get_sqlite_connection_pool();
    let mut conn = pool
        .acquire()
        .await
        .map_err(|error| DbErr::Custom(error.to_string()))?;
    let mut locked = conn
        .lock_handle()
        .await
        .map_err(|error| DbErr::Custom(error.to_string()))?;
    let handle = locked.as_raw_handle().as_ptr() as *mut ffi::sqlite3;
    unsafe {
        ffi::sqlite3_trace_v2(
            handle,
            ffi::SQLITE_TRACE_PROFILE,
            Some(profile_callback),
            std::ptr::null_mut(),
        );
    }
    Ok(())
}

/// Read cumulative page-cache hit/miss for the connection via sqlite3_db_status.
pub async fn sqlite_cache_stats(db: &DatabaseConnection) -> Result<(i64, i64), DbErr> {
    let pool = db.get_sqlite_connection_pool();
    let mut conn = pool
        .acquire()
        .await
        .map_err(|error| DbErr::Custom(error.to_string()))?;
    let mut locked = conn
        .lock_handle()
        .await
        .map_err(|error| DbErr::Custom(error.to_string()))?;
    let handle = locked.as_raw_handle().as_ptr() as *mut ffi::sqlite3;
    let (mut hit, mut miss, mut high_water) = (0i32, 0i32, 0i32);
    unsafe {
        ffi::sqlite3_db_status(handle, ffi::SQLITE_DBSTATUS_CACHE_HIT, &mut hit, &mut high_water, 0);
        ffi::sqlite3_db_status(handle, ffi::SQLITE_DBSTATUS_CACHE_MISS, &mut miss, &mut high_water, 0);
    }
    Ok((hit as i64, miss as i64))
}

/// Emit one metric sample as a tracing event. Fields land in the collected
/// [`Record`]s and in the CSV; anyone with their own subscriber sees them too.
fn sample(phase: &'static str, t_ms: f64) {
    let (disk_read, disk_write) = diskio();
    tracing::info!(
        target: "measure",
        phase,
        t_ms,
        rss_kb = peak_rss_kb(),
        disk_read,
        disk_write,
        major_faults = major_faults(),
        stmt = crate::stmt_counter::get() as i64,
    );
}

// ============================ the perf cell ============================

/// Independent variables — one OS process per Cell.
#[derive(Clone, Debug)]
pub struct Cell {
    pub engine: &'static str,
    pub workload: &'static str,
    pub nodes: i64,
    pub edges: i64,
    pub cache_size_kib: i64,
    pub memcap_mb: u64,
}

/// One phase's metrics, derived from its `measure` event.
#[derive(Clone, Debug)]
pub struct PhaseSample {
    pub phase: &'static str,
    pub t_ms: f64,
    pub rss_kb: i64,
    pub disk_read: i64,
    pub disk_write: i64,
    pub major_faults: i64,
    pub stmt: i64,
}

#[derive(Clone, Debug)]
pub struct RunRow {
    pub cell: Cell,
    pub samples: Vec<PhaseSample>,
    pub correct: bool,
    pub out_hash: String,
    pub aborted: bool,
}

const PHASES: [&str; 3] = ["build", "insert", "op"];

/// Drive one cell: open a store, run `build` then `op`, collect the per-phase
/// metrics through tracing, append a CSV row. Synchronous — it owns a
/// current-thread runtime so `collect`'s thread-local subscriber captures every
/// `.await`.
pub fn run_cell<S, O>(cell: Cell, build: S, op: O) -> RunRow
where
    S: for<'a> FnOnce(&'a RelStore) -> Pin<Box<dyn Future<Output = Result<(), DbErr>> + 'a>>,
    O: for<'a> FnOnce(&'a RelStore) -> Pin<Box<dyn Future<Output = Result<Vec<i64>, DbErr>> + 'a>>,
{
    let db_path = std::env::temp_dir().join(format!(
        "sprefa_measure_{}_{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    if cell.memcap_mb != 0 {
        crate::memcap::cap_address_space_mb(cell.memcap_mb);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let path = db_path.clone();
    let cache_size_kib = cell.cache_size_kib;
    let ((correct, out_hash, aborted), records) = collect(|| {
        runtime.block_on(async move {
            let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
            options.max_connections(1).min_connections(1);
            let db = Database::connect(options).await.unwrap();
            let store = RelStore::attach(db.clone()).await.unwrap();
            db.execute_unprepared(&format!("PRAGMA cache_size=-{cache_size_kib};"))
                .await
                .unwrap();
            // Every statement from here on emits a `target: "sqlite"` trace event.
            install_sqlite_trace(&db).await.ok();

            let started = Instant::now();
            let build_result = build(&store).await;
            sample("build", started.elapsed().as_secs_f64() * 1000.0);

            // insert is folded into build for the retract engines — a real no-op,
            // sampled so the phase set stays uniform across engines.
            sample("insert", 0.0);

            let op_started = Instant::now();
            let op_result = op(&store).await;
            sample("op", op_started.elapsed().as_secs_f64() * 1000.0);
            if let Ok((cache_hit, cache_miss)) = sqlite_cache_stats(&db).await {
                tracing::info!(target: "sqlite", phase = "op", cache_hit, cache_miss, "cache");
            }

            let ok = build_result.is_ok() && op_result.is_ok();
            let mut answer = op_result.unwrap_or_default();
            answer.sort_unstable();
            let out_hash = blake3::hash(format!("{answer:?}").as_bytes())
                .to_hex()
                .to_string();
            (ok, out_hash, build_result.is_err())
        })
    });

    let samples: Vec<PhaseSample> = PHASES
        .iter()
        .map(|&phase| phase_sample(phase, &records))
        .collect();

    let db_bytes = db_size_bytes(&db_path);
    append_csv(&cell, &samples, correct, &out_hash, aborted, db_bytes);

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));

    RunRow { cell, samples, correct, out_hash, aborted }
}

/// Build a `PhaseSample` from the collected `measure` event for `phase`.
fn phase_sample(phase: &'static str, records: &[Record]) -> PhaseSample {
    let event = records.iter().find(|record| {
        record.kind == Kind::Event && record.fields.get("phase").map(String::as_str) == Some(phase)
    });
    match event {
        Some(record) => PhaseSample {
            phase,
            t_ms: record.f64("t_ms").unwrap_or(0.0),
            rss_kb: record.i64("rss_kb").unwrap_or(0),
            disk_read: record.i64("disk_read").unwrap_or(0),
            disk_write: record.i64("disk_write").unwrap_or(0),
            major_faults: record.i64("major_faults").unwrap_or(0),
            stmt: record.i64("stmt").unwrap_or(0),
        },
        None => PhaseSample {
            phase,
            t_ms: 0.0,
            rss_kb: 0,
            disk_read: 0,
            disk_write: 0,
            major_faults: 0,
            stmt: 0,
        },
    }
}

/// On-disk size, WAL-aware: the main file plus any uncheckpointed `-wal`.
fn db_size_bytes(db_path: &PathBuf) -> i64 {
    let main = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    let wal = std::fs::metadata(db_path.with_extension("sqlite-wal"))
        .map(|m| m.len())
        .unwrap_or(0);
    (main + wal) as i64
}

fn append_csv(
    cell: &Cell,
    samples: &[PhaseSample],
    correct: bool,
    out_hash: &str,
    aborted: bool,
    db_bytes: i64,
) {
    // Crate-local and git-diffable (`just results` reads it). Overridable via
    // DL_PERF_CSV for a hermetic run.
    let csv = std::env::var("DL_PERF_CSV")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("perf-runs.csv"));
    if let Some(parent) = csv.parent().filter(|p| !p.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(parent);
    }
    let header = "sweep_ns,engine,workload,nodes,edges,cache_size_kib,memcap_mb,correct,out_hash,aborted,db_bytes,\
build_t_ms,build_rss_kb,build_disk_read,build_disk_write,build_major_faults,build_stmt,\
insert_t_ms,insert_rss_kb,insert_disk_read,insert_disk_write,insert_major_faults,insert_stmt,\
op_t_ms,op_rss_kb,op_disk_read,op_disk_write,op_major_faults,op_stmt\n";
    if !csv.exists() {
        let _ = std::fs::write(&csv, header);
    }
    let sweep_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut line = format!(
        "{sweep_ns},{},{},{},{},{},{},{},{out_hash},{},{db_bytes}",
        cell.engine,
        cell.workload,
        cell.nodes,
        cell.edges,
        cell.cache_size_kib,
        cell.memcap_mb,
        correct as i64,
        aborted as i64,
    );
    for sample in samples {
        line.push_str(&format!(
            ",{},{},{},{},{},{}",
            sample.t_ms,
            sample.rss_kb,
            sample.disk_read,
            sample.disk_write,
            sample.major_faults,
            sample.stmt,
        ));
    }
    line.push('\n');
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(&csv) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod harness_tests {
    use super::*;

    #[test]
    fn collects_span_timing_and_event_fields() {
        let (out, records) = collect(|| {
            let span = tracing::info_span!("op", workload = "DAG");
            let _guard = span.enter();
            tracing::info!(target: "measure", rss_kb = 123i64, disk_read = 4096i64, "sample");
            42
        });
        assert_eq!(out, 42);

        let event = records
            .iter()
            .find(|record| record.kind == Kind::Event)
            .expect("an event was collected");
        assert_eq!(event.i64("rss_kb"), Some(123));
        assert_eq!(event.i64("disk_read"), Some(4096));

        let span = records
            .iter()
            .find(|record| record.kind == Kind::Span && record.name == "op")
            .expect("the op span was collected");
        assert_eq!(span.fields.get("workload").map(String::as_str), Some("DAG"));
    }

    #[test]
    fn phase_sample_reads_the_measure_event() {
        let (_out, records) = collect(|| sample("op", 12.5));
        let phase = phase_sample("op", &records);
        assert_eq!(phase.t_ms, 12.5);
        assert!(phase.rss_kb > 0, "RSS should be a real positive number");
    }

    #[test]
    fn sqlite_trace_emits_the_real_sql_per_statement() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "sqlite_trace_test_{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let connect = path.clone();
        let (_out, records) = collect(|| {
            runtime.block_on(async move {
                let mut options =
                    ConnectOptions::new(format!("sqlite://{}?mode=rwc", connect.display()));
                options.max_connections(1).min_connections(1);
                let db = Database::connect(options).await.unwrap();
                install_sqlite_trace(&db).await.unwrap();
                db.execute_unprepared("CREATE TABLE t(x INTEGER)").await.unwrap();
                db.execute_unprepared("INSERT INTO t VALUES (1),(2),(3)").await.unwrap();
                let _ = db.execute_unprepared("SELECT x FROM t").await;
            })
        });
        let _ = std::fs::remove_file(&path);

        let sqlite_events: Vec<_> = records.iter().filter(|r| r.target == "sqlite").collect();
        assert!(
            !sqlite_events.is_empty(),
            "expected SQLite PROFILE trace events, got none"
        );
        assert!(
            sqlite_events
                .iter()
                .any(|r| r.fields.get("sql").is_some_and(|sql| sql.contains("INSERT INTO t"))),
            "expected the real INSERT statement text in the trace, got: {:?}",
            sqlite_events.iter().map(|r| &r.fields).collect::<Vec<_>>()
        );
    }
}

// ============================ GraphStore storage (Epic 1) =====================
// The split-vs-collapsed storage answer. Same corpus, same dense keys, two table
// sets — the only difference is how many tables carry the rows and how many dead
// value columns ride along. No cascade SQL runs on Collapsed (storage cost only);
// the dead g_node columns (digest/changed_at/verified_at) ARE written, which is
// the cost this measurement exists to weigh.

#[derive(Clone, Debug)]
pub struct StorageDelta {
    pub split_bytes: i64,
    pub collapsed_bytes: i64,
}

impl StorageDelta {
    /// Collapsed minus Split. Negative = collapsed wins on bytes.
    pub fn delta(&self) -> i64 {
        self.collapsed_bytes - self.split_bytes
    }
}

/// Load `corpus` under each layout, checkpoint, and read the on-disk bytes. The
/// Split path is the live RelStore (cx_row/cx_dep + rx_memo/rx_dep); the Collapsed
/// path inserts the same (key, weight) + edges straight into g_node/g_edge. Both
/// use the SAME dense key space (tag*STRIDE + id, STRIDE = 1e9), so the comparison
/// is fair — identical logical rows, only the table set differs.
pub async fn measure_storage(corpus: &benchgraph::MultiGraph) -> StorageDelta {
    let split_bytes = storage_bytes(Layout::Split, corpus).await;
    let collapsed_bytes = storage_bytes(Layout::Collapsed, corpus).await;
    StorageDelta { split_bytes, collapsed_bytes }
}

async fn storage_bytes(layout: Layout, corpus: &benchgraph::MultiGraph) -> i64 {
    let path = std::env::temp_dir().join(format!(
        "sprefa_storage_{}_{:x}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    opt.max_connections(1).min_connections(1);
    let db = Database::connect(opt).await.unwrap();

    const STRIDE: i64 = benchgraph::TAG_STRIDE;
    const CHUNK: usize = 4000;
    let key_of = |tag: u32, id: i64| -> i64 { (tag as i64) * STRIDE + id };

    match layout {
        Layout::Split => {
            let store = RelStore::attach_with(db.clone(), Layout::Split).await.unwrap();
            let rows: Vec<(i64, i64, i64)> = corpus
                .rows
                .iter()
                .map(|(tag, id, w)| (*tag as i64, *id, *w))
                .collect();
            let deps: Vec<(i64, i64, i64, i64)> = corpus
                .edges
                .iter()
                .map(|(pt, pi, ct, ci)| (*pt as i64, *pi as i64, *ct as i64, *ci as i64))
                .collect();
            store.add_rows(&rows).await.unwrap();
            store.add_deps(&deps).await.unwrap();
        }
        Layout::Collapsed => {
            stamp(&db, Layout::Collapsed).await.unwrap();
            // g_node(key, weight): the dead value columns keep their DEFAULT 0 —
            // present in the row format (the byte cost we weigh) but not written.
            for chunk in corpus.rows.chunks(CHUNK) {
                let vals: Vec<String> = chunk
                    .iter()
                    .map(|(tag, id, w)| format!("({},{})", key_of(*tag, *id), w))
                    .collect();
                db.execute_unprepared(&format!(
                    "INSERT INTO g_node(key,weight) VALUES {}",
                    vals.join(",")
                ))
                .await
                .unwrap();
            }
            for chunk in corpus.edges.chunks(CHUNK) {
                let vals: Vec<String> = chunk
                    .iter()
                    .map(|(pt, pi, ct, ci)| format!("({},{})", key_of(*pt, *pi), key_of(*ct, *ci)))
                    .collect();
                db.execute_unprepared(&format!(
                    "INSERT INTO g_edge(src,dst) VALUES {}",
                    vals.join(",")
                ))
                .await
                .unwrap();
            }
        }
    }
    // Fold WAL frames into the main file so the byte count is the durable size.
    db.execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE)").await.ok();
    let bytes = std::fs::metadata(&path).map(|m| m.len() as i64).unwrap_or(0);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
    bytes
}

#[cfg(test)]
mod storage_tests {
    use super::*;

    #[tokio::test]
    async fn measure_storage_weighs_both_layouts() {
        let g = benchgraph::gen_multi(3, 4);
        let delta = measure_storage(&g).await;
        assert!(delta.split_bytes > 0, "split bytes should be positive: {delta:?}");
        assert!(delta.collapsed_bytes > 0, "collapsed bytes should be positive: {delta:?}");
    }
}

// ---- folded from memcap.rs / benchgraph.rs (harness helpers) ----
pub mod memcap {
//! OS-protective self-cap for the head-to-head examples. The point is narrow:
//! a runaway scale (fat CLI arg, an accidental extra zero) must make the PROCESS
//! die with an allocation error, never drive the whole machine into swap.
//!
//! macOS reality check (proved with examples/memcap_probe): `setrlimit` does NOT
//! bite here. `RLIMIT_AS` is a documented no-op on Darwin and `RLIMIT_DATA` only
//! governs the `sbrk` segment, but system malloc services large allocations via
//! `mmap`, which neither limit touches. A 128 MB cap let a 512 MB Vec through.
//!
//! So the real enforcement is [`CappedAlloc`], a counting `#[global_allocator]`
//! wrapper: it tracks live bytes and returns null past the cap, which makes Rust
//! abort the process cleanly (SIGABRT) instead of the OS swapping. That works
//! identically on every platform because it intercepts every allocation in the
//! process. `setrlimit` is kept only as a belt-and-suspenders on Linux, where it
//! does bite; it is a no-op safety net on mac, never the guarantee.
//!
//! Each binary opts in by declaring the allocator:
//! ```ignore
//! #[global_allocator]
//! static GLOBAL: sprefa_store::memcap::CappedAlloc = sprefa_store::memcap::CappedAlloc;
//! ```
//! then calling [`cap_address_space_mb`] at the top of `main`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Live bytes currently handed out through [`CappedAlloc`]. Always tracked (even
/// when the cap is unset) so dealloc accounting can never underflow after a cap
/// is installed mid-run.
static LIVE: AtomicUsize = AtomicUsize::new(0);
/// Hard ceiling in bytes; 0 means unlimited (no enforcement).
static CAP: AtomicUsize = AtomicUsize::new(0);
/// High-water mark of [`LIVE`] since the last [`reset_peak`]. This is the honest
/// answer to "did the measured op ever transiently hold a lot of Rust heap?" —
/// reading LIVE after an op only shows what survives, not the peak during it.
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// Bump PEAK to at least `now` (relaxed CAS loop; only runs on the alloc path).
#[inline]
fn bump_peak(now: usize) {
    let mut cur = PEAK.load(Ordering::Relaxed);
    while now > cur {
        match PEAK.compare_exchange_weak(cur, now, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(x) => cur = x,
        }
    }
}

/// A `#[global_allocator]` that refuses to exceed [`cap_address_space_mb`].
/// Delegates every real allocation to the System allocator and only adds a pair
/// of relaxed atomics per call, so the un-capped path stays cheap.
pub struct CappedAlloc;

unsafe impl GlobalAlloc for CappedAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let cap = CAP.load(Ordering::Relaxed);
        // Reserve first, so concurrent allocs can't jointly overshoot the cap.
        let prev = LIVE.fetch_add(size, Ordering::Relaxed);
        if cap != 0 && prev + size > cap {
            LIVE.fetch_sub(size, Ordering::Relaxed);
            return std::ptr::null_mut(); // -> handle_alloc_error -> abort
        }
        let ptr = System.alloc(layout);
        if ptr.is_null() {
            LIVE.fetch_sub(size, Ordering::Relaxed);
        } else {
            bump_peak(prev + size);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let cap = CAP.load(Ordering::Relaxed);
        let prev = LIVE.fetch_add(size, Ordering::Relaxed);
        if cap != 0 && prev + size > cap {
            LIVE.fetch_sub(size, Ordering::Relaxed);
            return std::ptr::null_mut();
        }
        let ptr = System.alloc_zeroed(layout);
        if ptr.is_null() {
            LIVE.fetch_sub(size, Ordering::Relaxed);
        } else {
            bump_peak(prev + size);
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = layout.size();
        let cap = CAP.load(Ordering::Relaxed);
        if new_size > old {
            let grow = new_size - old;
            let prev = LIVE.fetch_add(grow, Ordering::Relaxed);
            if cap != 0 && prev + grow > cap {
                LIVE.fetch_sub(grow, Ordering::Relaxed);
                return std::ptr::null_mut();
            }
            let new_ptr = System.realloc(ptr, layout, new_size);
            if new_ptr.is_null() {
                LIVE.fetch_sub(grow, Ordering::Relaxed);
            } else {
                bump_peak(prev + grow);
            }
            new_ptr
        } else {
            LIVE.fetch_sub(old - new_size, Ordering::Relaxed);
            System.realloc(ptr, layout, new_size)
        }
    }
}

/// Cap this process's heap to `mb` megabytes. The [`CappedAlloc`] global
/// allocator is the real enforcer (aborts the process past the cap on every
/// platform); `setrlimit` is also set as a Linux-only belt-and-suspenders and is
/// a harmless no-op on macOS. Best-effort and idempotent: only tightens.
pub fn cap_address_space_mb(mb: u64) {
    let want = (mb as usize).saturating_mul(1024 * 1024);
    // Real enforcement: only lower an existing cap, never raise it.
    let cur = CAP.load(Ordering::Relaxed);
    if cur == 0 || want < cur {
        CAP.store(want, Ordering::Relaxed);
    }
    // Bonus on Linux (bites there); no-op safety net on macOS.
    set_soft(libc::RLIMIT_AS, want as u64);
    set_soft(libc::RLIMIT_DATA, want as u64);
}

/// Live bytes currently allocated through [`CappedAlloc`]. Test/introspection
/// hook; also lets a caller prove the accounting is wired.
pub fn live_bytes() -> usize {
    LIVE.load(Ordering::Relaxed)
}

/// High-water mark of live Rust heap since the last [`reset_peak`]. This is the
/// honest "peak Rust heap DURING the op" number: `live_bytes()` after an op only
/// shows what survives it, so a transient spike is invisible without this.
pub fn peak_bytes() -> usize {
    PEAK.load(Ordering::Relaxed)
}

/// Reset the high-water to the current live value, so the next [`peak_bytes`]
/// measures only allocations after this call (e.g. bracket the measured op).
pub fn reset_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}

/// The current hard cap in bytes; 0 = unlimited. Deterministic introspection for
/// tests (the enforcement itself can only be observed by aborting a subprocess).
pub fn cap_bytes() -> usize {
    CAP.load(Ordering::Relaxed)
}

fn set_soft(resource: libc::c_int, want: u64) {
    unsafe {
        let mut lim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(resource, &mut lim) != 0 {
            return; // cannot read the current limit; leave it alone
        }
        let want = want as libc::rlim_t;
        // Never raise an existing lower cap; only tighten. RLIM_INFINITY means
        // "unlimited", which is always looser than our finite request.
        if lim.rlim_cur != libc::RLIM_INFINITY && lim.rlim_cur <= want {
            return;
        }
        let target = if lim.rlim_max != libc::RLIM_INFINITY && lim.rlim_max < want {
            lim.rlim_max
        } else {
            want
        };
        lim.rlim_cur = target;
        let _ = libc::setrlimit(resource, &lim); // best-effort; ignore refusal
    }
}

}
pub mod benchgraph {
//! One deterministic DAG generator, shared by both sides of the head-to-head so
//! their INPUTS are byte-identical by construction. Nodes 0 and 1 are roots
//! (no parents); every other node has mixed support so retracting root 0 leaves
//! a non-trivial subset alive.

/// `parents[node]` = the parent node ids. Nodes 0 and 1 are roots.
pub fn gen(layers: usize, width: usize) -> Vec<Vec<i64>> {
    let n = 2 + layers * width;
    let mut parents: Vec<Vec<i64>> = vec![Vec::new(); n];
    for l in 0..layers {
        for w in 0..width {
            let id = 2 + l * width + w;
            if l == 0 {
                parents[id].push(0);
                if w % 3 == 0 {
                    parents[id].push(1);
                }
            } else {
                let prev = 2 + (l - 1) * width;
                parents[id].push((prev + w) as i64);
                parents[id].push((prev + (w + 1) % width) as i64);
            }
        }
    }
    parents
}

/// Flatten to `(parent, child)` edges.
pub fn edges(parents: &[Vec<i64>]) -> Vec<(i64, i64)> {
    let mut e = Vec::new();
    for (id, ps) in parents.iter().enumerate() {
        for &p in ps {
            e.push((p, id as i64));
        }
    }
    e
}

/// A multi-relation reference graph: THREE logical relations so the polymorphic
/// `(tag, id)` key is load-bearing. Local ids deliberately COLLIDE across
/// relations (module 5, fn 5, type 5 are three distinct rows), so `id` alone
/// cannot address a row — only `(tag, id)` can. Edges cross relations
/// (module -> fn -> type), so retracting a module cascades through all three.
///
/// tag 0 = modules  (roots, no parents, weight 1)
/// tag 1 = functions (each depends on 1-2 modules; weight = # module parents)
/// tag 2 = types     (each depends on 1-2 functions; weight = # fn parents)
///
/// Fan-in of 2 on the derived tiers is the point: a function supported by two
/// modules SURVIVES the loss of one (weight 2 -> 1), so this is real Z-set
/// retraction, not naive reachability.
pub struct MultiGraph {
    /// (tag, id, weight)
    pub rows: Vec<(u32, i64, i64)>,
    /// (parent_tag, parent_id, child_tag, child_id)
    pub edges: Vec<(u32, i64, u32, i64)>,
    /// The retract target (a root in relation 0).
    pub seed: (u32, i64),
    /// rows per relation, index = tag.
    pub per_tag: [usize; 3],
}

/// The proven layered DAG, but tiered into THREE relations so `(tag, id)` is
/// load-bearing and one retraction cascades across all three. Tier of a node =
/// its dependency depth; `tag = tier % 3`. Roots (tier 0) are relation 0.
/// Consecutive tiers always differ mod 3, so EVERY edge crosses relations.
/// Local ids restart per relation, so they collide across relations (only
/// `(tag,id)` is unique). Two roots (0 and 1) with mixed support means
/// retracting root 0 kills the 0-lineage while the 1-lineage survives — real
/// Z-set retraction with a non-trivial cross-relation cascade.
pub fn gen_multi(layers: usize, width: usize) -> MultiGraph {
    let parents = gen(layers, width); // parents[g] = global parent ids
    let n = parents.len();

    // tier(g): roots (g<2) = 0; node 2+l*width+w = tier l+1.
    let tier = |g: usize| -> usize {
        if g < 2 { 0 } else { 1 + (g - 2) / width }
    };
    let tag_of = |g: usize| -> u32 { (tier(g) % 3) as u32 };

    // Assign a per-relation local id to every global node, in global order.
    let mut local = vec![0i64; n];
    let mut per_tag = [0usize; 3];
    for g in 0..n {
        let t = tag_of(g) as usize;
        local[g] = per_tag[t] as i64;
        per_tag[t] += 1;
    }

    let mut rows = Vec::with_capacity(n);
    let mut edges = Vec::new();
    for g in 0..n {
        let w = if parents[g].is_empty() { 1 } else { parents[g].len() as i64 };
        rows.push((tag_of(g), local[g], w));
        for &p in &parents[g] {
            let pg = p as usize;
            edges.push((tag_of(pg), local[pg], tag_of(g), local[g]));
        }
    }

    MultiGraph {
        rows,
        edges,
        seed: (tag_of(0), local[0]), // global root 0
        per_tag,
    }
}

/// Encode `(tag, id)` into one dense integer so the resident engines (dd, dbsp)
/// — which only do reachability over opaque node keys — see byte-identical
/// inputs/outputs to the tagged SQLite side. Stride must exceed any local id.
pub const TAG_STRIDE: i64 = 1_000_000_000;

#[inline]
pub fn encode(tag: u32, id: i64) -> i64 {
    tag as i64 * TAG_STRIDE + id
}

/// The proven layered graph, but with CYCLES injected so the counting cascade is
/// provably WRONG and DRed/dd are provably right at scale. Back-edges point from a
/// node to its own layer-`l-1` parent, forming a 2-cycle (parent supports child AND
/// child supports parent). `back_stride` selects which nodes get a back-edge: every
/// node where `(global_id) % back_stride == 0`, so `back_stride=1` makes every
/// derived node cyclic and a large stride makes it sparse. `back_stride=0` = no
/// back-edges (identical to `gen_multi`). Each back-edge adds a support, so the
/// ancestor's weight rises by one — real Z-set weight, not a boolean.
///
/// Correctness consequence: a cycle whose only outside anchor is root 0 dies when
/// root 0 is cut (no surviving anchor). Counting keeps it alive (phantom — the
/// members mutually support each other, weight never reaches 0). DRed and dd kill
/// it. `oracle_survivors` is the independent referee.
pub fn gen_multi_cyclic(layers: usize, width: usize, back_stride: usize) -> MultiGraph {
    let mut g = gen_multi(layers, width);
    if back_stride == 0 {
        return g;
    }
    // Rebuild global structure to find each node's layer-(l-1) parent to point back at.
    let parents = gen(layers, width); // parents[global] = global parent ids
    let n = parents.len();
    let tier = |gid: usize| -> usize { if gid < 2 { 0 } else { 1 + (gid - 2) / width } };
    let tag_of = |gid: usize| -> u32 { (tier(gid) % 3) as u32 };
    // recover the same per-relation local ids gen_multi assigned (global order).
    let mut local = vec![0i64; n];
    let mut per_tag = [0usize; 3];
    for gid in 0..n {
        let t = tag_of(gid) as usize;
        local[gid] = per_tag[t] as i64;
        per_tag[t] += 1;
    }
    // add back-support edges child -> first-parent, and bump the parent's weight.
    let mut extra_weight = std::collections::HashMap::<(u32, i64), i64>::new();
    for gid in 2..n {
        if gid % back_stride != 0 {
            continue;
        }
        let Some(&p) = parents[gid].first() else { continue };
        // Never draw a back-edge INTO a root (global id < 2): a root must stay a
        // true source (in-degree 0), and an edge into the cut node would make "cut"
        // mean node-deleted to the oracle but root-re-derivable to dd/DRed. The
        // interesting cycle is between two DERIVED nodes, anchored to a root only
        // through a forward path — cut the root and the whole cycle must die.
        if (p as usize) < 2 {
            continue;
        }
        let (pt, pi) = (tag_of(p as usize), local[p as usize]);
        let (ct, ci) = (tag_of(gid), local[gid]);
        // child supports parent (the back-edge that closes the cycle).
        g.edges.push((ct, ci, pt, pi));
        *extra_weight.entry((pt, pi)).or_insert(0) += 1;
    }
    for row in g.rows.iter_mut() {
        if let Some(add) = extra_weight.get(&(row.0, row.1)) {
            row.2 += add;
        }
    }
    g
}

/// Independent ground truth: after cutting `cut`, which rows are still supported?
/// A row survives iff it is forward-reachable (over support edges) from a SURVIVING
/// root — a root being any row with no incoming support edge (in-degree 0). This is
/// a dead-simple in-Rust BFS owing nothing to counting, DRed, dd, or SQLite, so it
/// is the referee all three are checked against. Returns encoded survivor keys.
pub fn oracle_survivors(g: &MultiGraph, cut: (u32, i64)) -> std::collections::BTreeSet<i64> {
    use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
    let cut_key = encode(cut.0, cut.1);
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut has_parent: HashSet<i64> = HashSet::new();
    for (pt, pi, ct, ci) in &g.edges {
        let (pk, ck) = (encode(*pt, *pi), encode(*ct, *ci));
        adj.entry(pk).or_default().push(ck);
        has_parent.insert(ck);
    }
    // roots = rows with no incoming support edge, minus the cut row.
    let mut frontier: VecDeque<i64> = VecDeque::new();
    let mut seen: BTreeSet<i64> = BTreeSet::new();
    for (t, i, _w) in &g.rows {
        let k = encode(*t, *i);
        if k != cut_key && !has_parent.contains(&k) {
            seen.insert(k);
            frontier.push_back(k);
        }
    }
    while let Some(k) = frontier.pop_front() {
        if let Some(children) = adj.get(&k) {
            for &c in children {
                if c != cut_key && seen.insert(c) {
                    frontier.push_back(c);
                }
            }
        }
    }
    seen
}

}
