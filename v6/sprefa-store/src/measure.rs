//! The ONE uniform measurement path. Recursive-CTE RAM is not guessable from row
//! counts, so every perf run captures the SAME sensor set at the SAME phase
//! boundaries through `run_cell`. No example may read a sensor by hand — that
//! makes its numbers incomparable and disqualifies them from the golden archive.
//!
//! FROZEN CONTRACT: `v6/findings/INSIGHTS.md` §C. Sink: `v6/labs/perf-runs.sqlite`.

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

/// Captured identically at each phase boundary ("build" | "insert" | "op").
#[derive(Clone, Debug)]
pub struct PhaseSample {
    pub phase: &'static str,
    pub t_ms: f64,
    pub rss_kb: i64,
    pub sqlite_hw_kb: i64,
    pub disk_read: i64,
    pub disk_write: i64,
    pub cache_hit: i64,
    pub cache_miss: i64,
    pub cache_write: i64,
}

#[derive(Clone, Debug)]
pub struct RunRow {
    pub cell: Cell,
    pub samples: Vec<PhaseSample>,
    pub correct: bool,
    pub out_hash: String,
    pub aborted: bool,
}

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use crate::relstore::RelStore;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

static CURRENT_DB_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

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

fn sqlite_hw_kb() -> i64 {
    unsafe {
        let symbol = std::ffi::CString::new("sqlite3_memory_highwater").unwrap();
        let address = libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr());
        if address.is_null() { return -1; }
        let highwater: unsafe extern "C" fn(i32) -> i64 = std::mem::transmute(address);
        highwater(0) / 1024
    }
}

fn diskio() -> ((i64, i64), &'static str) {
    #[cfg(target_os = "macos")]
    unsafe {
        let mut usage: libc::rusage_info_v2 = std::mem::zeroed();
        let result = libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            &mut usage as *mut _ as *mut libc::rusage_info_t,
        );
        if result == 0 {
            return ((usage.ri_diskio_bytesread as i64, usage.ri_diskio_byteswritten as i64), "proc_pid_rusage");
        }
    }
    ((0, 0), "unavailable")
}

fn db_status_cache() -> (i64, i64, i64) {
    (-1, -1, -1)
}

fn append_run(row: &RunRow) {
    let measured_db = CURRENT_DB_PATH.lock().unwrap().clone().unwrap();
    let archive = PathBuf::from("../labs/perf-runs.sqlite");
    if let Some(parent) = archive.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let schema = "CREATE TABLE IF NOT EXISTS runs (
           sweep_ts TEXT NOT NULL, engine TEXT, workload TEXT, nodes INTEGER, edges INTEGER,
           cache_size_kib INTEGER, memcap_mb INTEGER, correct INTEGER, out_hash TEXT, aborted INTEGER,
           db_bytes INTEGER, final_table_bytes INTEGER, final_index_bytes INTEGER,
           diskio_source TEXT, build_rss_kb INTEGER, insert_rss_kb INTEGER, op_rss_kb INTEGER
         );
         CREATE TABLE IF NOT EXISTS phase_samples (
           sweep_ts TEXT NOT NULL, phase TEXT, t_ms REAL, rss_kb INTEGER, sqlite_hw_kb INTEGER,
           disk_read INTEGER, disk_write INTEGER, cache_hit INTEGER, cache_miss INTEGER, cache_write INTEGER
         );";
    let _ = std::process::Command::new("sqlite3").arg(&archive).arg(schema).status();
    let stats = std::process::Command::new("sqlite3")
        .args(["-separator", "|", measured_db.to_str().unwrap(), "SELECT SUM(CASE WHEN name LIKE 'ix_%' OR name LIKE 'sqlite_autoindex_%' THEN pgsize ELSE 0 END), SUM(CASE WHEN name NOT LIKE 'ix_%' AND name NOT LIKE 'sqlite_autoindex_%' THEN pgsize ELSE 0 END) FROM dbstat;"])
        .output().unwrap();
    let values: Vec<i64> = String::from_utf8_lossy(&stats.stdout).trim().split('|').filter_map(|value| value.parse().ok()).collect();
    let index_bytes = values.first().copied().unwrap_or(0);
    let table_bytes = values.get(1).copied().unwrap_or(0);
    let db_bytes = std::fs::metadata(&measured_db).map(|metadata| metadata.len() as i64).unwrap_or(table_bytes + index_bytes);
    let sweep_ts = chrono_like_timestamp();
    let source = row.samples.first().map(|sample| {
        let _ = sample;
        diskio().1
    }).unwrap_or("unavailable");
    let value = |phase: &str, field: fn(&PhaseSample) -> i64| {
        row.samples.iter().find(|sample| sample.phase == phase).map(field).unwrap_or(0)
    };
    let build_rss = value("build", |sample| sample.rss_kb);
    let insert_rss = value("insert", |sample| sample.rss_kb);
    let op_rss = value("op", |sample| sample.rss_kb);
    let run_sql = format!(
        "INSERT INTO runs VALUES ('{}','{}','{}',{},{},{},{},{},'{}',{},{},{},{},'{}',{},{},{})",
        sweep_ts, row.cell.engine, row.cell.workload, row.cell.nodes, row.cell.edges,
        row.cell.cache_size_kib, row.cell.memcap_mb, row.correct as i64, row.out_hash,
        row.aborted as i64, db_bytes, table_bytes, index_bytes, source,
        build_rss, insert_rss, op_rss,
    );
    let _ = std::process::Command::new("sqlite3").arg(&archive).arg(run_sql).status();
    for sample in &row.samples {
        let phase_sql = format!(
            "INSERT INTO phase_samples VALUES ('{}','{}',{},{},{},{},{},{},{},{})",
            sweep_ts, sample.phase, sample.t_ms, sample.rss_kb, sample.sqlite_hw_kb,
            sample.disk_read, sample.disk_write, sample.cache_hit, sample.cache_miss, sample.cache_write,
        );
        let _ = std::process::Command::new("sqlite3").arg(&archive).arg(phase_sql).status();
    }
    let csv = PathBuf::from("../labs/perf-runs.csv");
    let header = "sweep_ts,engine,workload,nodes,edges,cache_size_kib,memcap_mb,correct,out_hash,aborted,db_bytes,final_table_bytes,final_index_bytes,diskio_source,build_t_ms,build_rss_kb,build_sqlite_hw_kb,build_disk_read,build_disk_write,build_cache_hit,build_cache_miss,build_cache_write,insert_t_ms,insert_rss_kb,insert_sqlite_hw_kb,insert_disk_read,insert_disk_write,insert_cache_hit,insert_cache_miss,insert_cache_write,op_t_ms,op_rss_kb,op_sqlite_hw_kb,op_disk_read,op_disk_write,op_cache_hit,op_cache_miss,op_cache_write\n";
    if !csv.exists() { std::fs::write(&csv, header).unwrap(); }
    let mut line = format!("{},{},{},{},{},{},{},{},{},{},{},{},{},{}", sweep_ts, row.cell.engine, row.cell.workload, row.cell.nodes, row.cell.edges, row.cell.cache_size_kib, row.cell.memcap_mb, row.correct as i64, row.out_hash, row.aborted as i64, db_bytes, table_bytes, index_bytes, source);
    for phase in ["build", "insert", "op"] {
        let sample = row.samples.iter().find(|sample| sample.phase == phase).unwrap();
        line.push_str(&format!(",{},{},{},{},{},{},{},{}", sample.t_ms, sample.rss_kb, sample.sqlite_hw_kb, sample.disk_read, sample.disk_write, sample.cache_hit, sample.cache_miss, sample.cache_write));
    }
    line.push('\n');
    std::fs::OpenOptions::new().append(true).open(csv).unwrap().write_all(line.as_bytes()).unwrap();
}

fn chrono_like_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros().to_string()
}

use std::io::Write;

pub async fn run_cell<S, O>(cell: Cell, build: S, op: O) -> RunRow
where
    S: for<'a> FnOnce(&'a RelStore) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), sea_orm::DbErr>> + 'a>>,
    O: for<'a> FnOnce(&'a RelStore) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<i64>, sea_orm::DbErr>> + 'a>>,
{
    let db_path = std::env::temp_dir().join(format!("sprefa_measure_{}_{}.sqlite", std::process::id(), chrono_like_timestamp()));
    let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", db_path.display()));
    options.max_connections(1).min_connections(1);
    let db = Database::connect(options).await.unwrap();
    let store = RelStore::attach(db.clone()).await.unwrap();
    db.execute_unprepared(&format!("PRAGMA cache_size=-{};", cell.cache_size_kib)).await.unwrap();
    if cell.memcap_mb != 0 { crate::memcap::cap_address_space_mb(cell.memcap_mb); }
    *CURRENT_DB_PATH.lock().unwrap() = Some(db_path.clone());
    let mut samples = Vec::new();
    let phase = |name, elapsed: f64| {
        let ((read, write), _) = diskio();
        let (hit, miss, cache_write) = db_status_cache();
        PhaseSample { phase: name, t_ms: elapsed, rss_kb: peak_rss_kb(), sqlite_hw_kb: sqlite_hw_kb(), disk_read: read, disk_write: write, cache_hit: hit, cache_miss: miss, cache_write }
    };
    let started = Instant::now();
    let build_result = build(&store).await;
    samples.push(phase("build", started.elapsed().as_secs_f64() * 1000.0));
    let insert_started = Instant::now();
    samples.push(phase("insert", insert_started.elapsed().as_secs_f64() * 1000.0));
    let op_started = Instant::now();
    let result = op(&store).await;
    samples.push(phase("op", op_started.elapsed().as_secs_f64() * 1000.0));
    let result_ok = result.is_ok();
    let mut answer = result.unwrap_or_default();
    answer.sort_unstable();
    let out_hash = blake3::hash(format!("{:?}", answer).as_bytes()).to_hex().to_string();
    let row = RunRow { cell, samples, correct: build_result.is_ok() && result_ok, out_hash, aborted: build_result.is_err() || !result_ok };
    append_run(&row);
    drop(store);
    drop(db);
    let _ = std::fs::remove_file(&db_path);
    row
}

// job B ("luna-role") fills:
//   fn peak_rss_kb() / sqlite_hw_kb() / diskio() / db_status_cache() — ONE impl each
//   pub async fn run_cell<S,O>(cell, build, op) -> RunRow
//   fn append_run(&RunRow) -> perf-runs.sqlite  (schema: runs / phase_samples)
// See INSIGHTS §C. Sensors must match the golden-data contract (v6/labs/AGENTS.md).
