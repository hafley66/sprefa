//! v3 plugin-arch baseline for sqlite-batched inserts.
//!
//! Workload: extract every C string literal from the linux kernel and
//! insert the tuples into sqlite. Goal: push sqlite insert throughput
//! as hard as the plugin surface allows, so we know where v3's write
//! leg sits before building on it.
//!
//! Two effects, both mediated by the v3 registry:
//!
//!   ctx.put(ExtractStrings { paths })  → Vec<StringHit>     (Passthrough + rayon)
//!   ctx.put(InsertHits     { hits  })  → u64 (rows written) (BoundedBatched writer)
//!
//! The op loops:
//!   for chunk in paths.chunks(N) {
//!     let hits = ctx.put(ExtractStrings { chunk }).await;
//!     rows    += ctx.put(InsertHits     { hits  }).await;
//!   }
//!
//! This exercises:
//! - per-chunk scan emission through Passthrough (zero plugin overhead)
//! - per-chunk write emission through BoundedBatched (coalesces
//!   consecutive chunks into larger transactions for amortized fsync)
//! - backpressure: if insert is slower than extract, the op parks on
//!   put(InsertHits).await when the write inbox fills.
//!
//! sqlite knobs pinned aggressively ("optimize the shit out"):
//!   journal_mode = WAL
//!   synchronous  = OFF           (benchmark; prod would be NORMAL)
//!   temp_store   = MEMORY
//!   mmap_size    = 1 GiB
//!   page_size    = 8192
//!   cache_size   = -262144        (256 MiB)
//!   locking_mode = EXCLUSIVE
//! Single writer thread; prepared INSERT reused; one BEGIN/COMMIT per
//! coalesced batch.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use ignore::WalkBuilder;
use regex::bytes::Regex;
use rusqlite::Connection;

use effect_runtime::batchers::{BoundedBatched, Passthrough};
use effect_runtime::{EffectKind, RtCtxBuilder};

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// --- effects --------------------------------------------------------------

#[derive(Clone)]
struct StringHit {
    file_id: u32,
    byte_start: u32,
    byte_end: u32,
    value: String,
}

struct ExtractStrings {
    // file_id parallel to paths so the handler doesn't need the full
    // path table to emit rows. Keeps inserts purely numeric + string.
    files: Vec<(u32, PathBuf)>,
}

impl EffectKind for ExtractStrings {
    type Response = Vec<StringHit>;
    /// No meaningful input bytes (paths). Attribute throughput via
    /// the response side so telemetry reports extraction bandwidth
    /// as hit-byte output per put.
    fn response_bytes(r: &Self::Response) -> Option<usize> {
        Some(r.iter().map(|h| h.value.len()).sum())
    }
}

struct InsertHits {
    hits: Vec<StringHit>,
}

impl EffectKind for InsertHits {
    type Response = u64;
    /// Attribute insert throughput to the payload side: sum of hit
    /// string lengths approximates the bytes sqlite writes per put.
    /// Telemetry reports "sqlite write MB/s" off this one field.
    fn payload_bytes(&self) -> Option<usize> {
        self.hits.iter().map(|h| h.value.len()).sum::<usize>().into()
    }
}

// --- helpers --------------------------------------------------------------

fn enumerate(root: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for entry in WalkBuilder::new(root).hidden(true).git_ignore(false).build() {
        let Ok(e) = entry else { continue };
        if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let p = e.into_path();
        let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
        if exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) {
            v.push(p);
        }
    }
    v
}

fn rss_peak_kb() -> u64 {
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 { return 0; }
        #[cfg(target_os = "macos")]
        { (u.ru_maxrss as u64) / 1024 }
        #[cfg(not(target_os = "macos"))]
        { u.ru_maxrss as u64 }
    }
}

/// C/C++ string-literal regex. Double-quoted, escape-aware. Good enough
/// for a throughput test — not a C parser. Matches raw byte ranges so
/// we avoid UTF-8 validation per hit.
fn string_lit_regex() -> Regex {
    // "(?:[^"\\\n]|\\.)*"
    Regex::new(r#""(?:[^"\\\n]|\\.)*""#).unwrap()
}

// --- sqlite writer --------------------------------------------------------

fn open_writer(db_path: &Path) -> Connection {
    if db_path.exists() {
        std::fs::remove_file(db_path).ok();
    }
    let conn = Connection::open(db_path).expect("open sqlite");
    // Pin page_size BEFORE anything else touches the db; after the
    // first page is written you cannot change it without VACUUM.
    conn.pragma_update(None, "page_size", 8192).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "synchronous", "OFF").unwrap();
    conn.pragma_update(None, "temp_store", "MEMORY").unwrap();
    conn.pragma_update(None, "mmap_size", 1_073_741_824i64).unwrap();
    conn.pragma_update(None, "cache_size", -262_144i64).unwrap();
    conn.pragma_update(None, "locking_mode", "EXCLUSIVE").unwrap();
    // Schema: no AUTOINCREMENT (cheaper: rowid advances by 1 each
    // insert under the prepared stmt path), no indexes during the
    // bulk load. An index can be built after if we actually query.
    conn.execute_batch(
        "CREATE TABLE strings (
            file_id    INTEGER NOT NULL,
            byte_start INTEGER NOT NULL,
            byte_end   INTEGER NOT NULL,
            value      TEXT    NOT NULL
         );",
    ).unwrap();
    conn
}

fn insert_batched(conn: &mut Connection, batches: Vec<InsertHits>) -> Vec<u64> {
    // Coalesce N InsertHits puts into one BEGIN/COMMIT. Multi-value
    // INSERT in chunks of MULTI_ROW rows per statement to collapse
    // N statement executions into N/MULTI_ROW. sqlite caps bind
    // variables at 32766 by default; 4 bind per row → 8191 max.
    const MULTI_ROW: usize = 512;
    let counts: Vec<u64> = batches.iter().map(|b| b.hits.len() as u64).collect();
    let mut flat: Vec<&StringHit> = Vec::new();
    for b in &batches { for h in &b.hits { flat.push(h); } }
    let tx = conn.transaction().expect("tx begin");
    {
        // Precompute the VALUES placeholder string once per template
        // size. We reuse stmt_full (for full MULTI_ROW chunks) and
        // build stmt_tail lazily for the remainder.
        let values_n = |n: usize| -> String {
            let mut s = String::with_capacity(n * 14);
            for i in 0..n {
                if i > 0 { s.push(','); }
                s.push_str("(?,?,?,?)");
            }
            s
        };
        let full_sql = format!(
            "INSERT INTO strings (file_id, byte_start, byte_end, value) VALUES {}",
            values_n(MULTI_ROW),
        );
        let mut full_stmt = tx.prepare_cached(&full_sql).expect("prepare full");
        let mut i = 0;
        while i + MULTI_ROW <= flat.len() {
            let chunk = &flat[i..i + MULTI_ROW];
            let params: Vec<&dyn rusqlite::ToSql> = chunk
                .iter()
                .flat_map(|h| -> [&dyn rusqlite::ToSql; 4] {
                    [&h.file_id, &h.byte_start, &h.byte_end, &h.value]
                })
                .collect();
            full_stmt.execute(params.as_slice()).expect("insert full");
            i += MULTI_ROW;
        }
        drop(full_stmt);
        if i < flat.len() {
            let rem = flat.len() - i;
            let tail_sql = format!(
                "INSERT INTO strings (file_id, byte_start, byte_end, value) VALUES {}",
                values_n(rem),
            );
            let mut tail_stmt = tx.prepare(&tail_sql).expect("prepare tail");
            let chunk = &flat[i..];
            let params: Vec<&dyn rusqlite::ToSql> = chunk
                .iter()
                .flat_map(|h| -> [&dyn rusqlite::ToSql; 4] {
                    [&h.file_id, &h.byte_start, &h.byte_end, &h.value]
                })
                .collect();
            tail_stmt.execute(params.as_slice()).expect("insert tail");
        }
    }
    tx.commit().expect("tx commit");
    counts
}

// --- main -----------------------------------------------------------------

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut root = PathBuf::from("tests/smoke/.fixtures/linux");
    let mut workers: usize = 8;
    let mut trials: usize = 3;
    let mut chunk_size: usize = 256;
    let mut write_inbox_cap: usize = 8;
    let mut max_batch: usize = 4;
    let mut db_dir = std::env::temp_dir();
    let mut skip_insert: bool = false;
    let mut scan_only: bool = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root"       => { root = PathBuf::from(&args[i + 1]); i += 2; }
            "--workers"    => { workers = args[i + 1].parse().unwrap(); i += 2; }
            "--trials"     => { trials = args[i + 1].parse::<usize>().unwrap().max(1); i += 2; }
            "--chunk"      => { chunk_size = args[i + 1].parse().unwrap(); i += 2; }
            "--cap"        => { write_inbox_cap = args[i + 1].parse().unwrap(); i += 2; }
            "--max-batch"  => { max_batch = args[i + 1].parse().unwrap(); i += 2; }
            "--db-dir"     => { db_dir = PathBuf::from(&args[i + 1]); i += 2; }
            "--skip-insert"=> { skip_insert = true; i += 1; }
            "--scan-only"  => { scan_only = true; i += 1; }
            other => panic!("unknown arg: {}", other),
        }
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .ok();

    // Enumerate + warm.
    let t_enum = Instant::now();
    let paths = enumerate(&root, &["c", "h"]);
    let enumerate_ms = t_enum.elapsed().as_secs_f64() * 1000.0;
    assert!(!paths.is_empty(), "no files enumerated under {:?}", root);
    let rss_after_enum_kb = rss_peak_kb();
    let t_pc = Instant::now();
    for p in &paths { let _ = std::fs::read(p); }
    let page_cache_ms = t_pc.elapsed().as_secs_f64() * 1000.0;
    let rss_after_pc_kb = rss_peak_kb();

    let regex_arc: Arc<Regex> = Arc::new(string_lit_regex());

    eprintln!(
        "# harness: files={} enumerate_ms={:.1} page_cache_ms={:.1}",
        paths.len(), enumerate_ms, page_cache_ms,
    );
    eprintln!(
        "# rss_kb: after_enum={} after_pc={}",
        rss_after_enum_kb, rss_after_pc_kb,
    );
    eprintln!(
        "# config: workers={} chunk_size={} write_inbox_cap={} max_batch={}",
        workers, chunk_size, write_inbox_cap, max_batch,
    );

    for trial in 0..trials {
        // Fresh DB per trial so we measure apples-to-apples. File path
        // in tmp so it's on APFS (macOS) — inserts hit the page cache
        // / disk, not just RAM.
        let db_path = db_dir.join(format!("sprf_v3_sqlite_bench.{}.db", trial));
        let writer_conn = open_writer(&db_path);
        let conn_cell = std::sync::Mutex::new(Some(writer_conn));

        // Build registry. ExtractStrings → Passthrough + rayon.
        // InsertHits → BoundedBatched with single writer thread.
        let regex_for_extract = regex_arc.clone();
        let ctx = RtCtxBuilder::new()
            .register::<ExtractStrings, _>(Passthrough::<ExtractStrings, _>::new(
                move |e: ExtractStrings| -> Vec<StringHit> {
                    use rayon::prelude::*;
                    e.files
                        .par_iter()
                        .flat_map_iter(|(fid, path)| {
                            let bytes = match std::fs::read(path) {
                                Ok(b) => b,
                                Err(_) => return Vec::new().into_iter(),
                            };
                            let mut out = Vec::with_capacity(64);
                            for m in regex_for_extract.find_iter(&bytes) {
                                let s = std::str::from_utf8(m.as_bytes())
                                    .map(|s| s.to_string())
                                    .unwrap_or_default();
                                if s.is_empty() { continue; }
                                out.push(StringHit {
                                    file_id: *fid,
                                    byte_start: m.start() as u32,
                                    byte_end: m.end() as u32,
                                    value: s,
                                });
                            }
                            out.into_iter()
                        })
                        .collect()
                },
            ))
            .register::<InsertHits, _>(BoundedBatched::<InsertHits>::new(
                write_inbox_cap,
                max_batch,
                /* workers */ 1,
                move |batch: Vec<InsertHits>| -> Vec<u64> {
                    if skip_insert {
                        // Ack without writing. Isolates scan throughput
                        // from sqlite throughput.
                        return batch.iter().map(|b| b.hits.len() as u64).collect();
                    }
                    let mut guard = conn_cell.lock().unwrap();
                    let conn = guard.as_mut().expect("writer conn");
                    insert_batched(conn, batch)
                },
            ))
            .build();

        // Pair each path with a file_id.
        let files_indexed: Vec<(u32, PathBuf)> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| (i as u32, p.clone()))
            .collect();

        let t0 = Instant::now();
        let mut rows: u64 = 0;
        let mut hit_count: u64 = 0;

        // Pipelined fan: buffer up to K extract+insert pairs in flight
        // so scan and write overlap. K small (4) because each extract
        // chunk is already large and each insert holds hits in memory.
        use futures::stream::{FuturesUnordered, StreamExt};
        const IN_FLIGHT: usize = 4;
        let mut inserts = FuturesUnordered::new();

        for chunk in files_indexed.chunks(chunk_size) {
            // Extract is awaited (cheap relative to insert, and gives
            // us hit_count as we go). Insert is spawned and overlaps.
            let hits = ctx
                .put(ExtractStrings { files: chunk.to_vec() })
                .await;
            hit_count += hits.len() as u64;
            if scan_only {
                rows += hits.len() as u64;
                continue;
            }
            inserts.push(ctx.put(InsertHits { hits }));
            while inserts.len() >= IN_FLIGHT {
                if let Some(r) = inserts.next().await {
                    rows += r;
                }
            }
        }
        while let Some(r) = inserts.next().await {
            rows += r;
        }
        let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let rss_kb = rss_peak_kb();
        let rows_per_sec = (rows as f64) / (wall_ms / 1000.0);
        // Per-effect breakdown from the framework telemetry sink —
        // same fields this bench used to compute by hand.
        eprintln!("{}", ctx.telemetry().summary());

        // Drop the RtCtx (and through it the BoundedBatched sender).
        // Worker thread sees recv() error and exits, releasing the
        // connection. Wait briefly so EXCLUSIVE locking_mode releases
        // before we open a fresh verify connection.
        drop(ctx);
        if scan_only || skip_insert {
            eprintln!(
                "# trial {}/{} (scan_only={} skip_insert={}): wall_ms={:.1} rows={} rows/s={:.0} hits={} rss_kb={}",
                trial + 1, trials, scan_only, skip_insert,
                wall_ms, rows, rows_per_sec, hit_count, rss_kb,
            );
            // No DB file to verify / clean when scan_only.
            continue;
        }
        // Busy-wait with retry — worker thread exit is async.
        let verify: Connection = loop {
            std::thread::sleep(std::time::Duration::from_millis(20));
            match Connection::open(&db_path) {
                Ok(c) => match c.query_row::<u64, _, _>(
                    "SELECT COUNT(*) FROM strings", [], |r| r.get(0),
                ) {
                    Ok(_) => break c,
                    Err(_) => continue,
                },
                Err(_) => continue,
            }
        };
        let on_disk: u64 = verify
            .query_row("SELECT COUNT(*) FROM strings", [], |r| r.get(0))
            .unwrap();
        let db_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
        verify.close().ok();
        std::fs::remove_file(&db_path).ok();
        let wal = db_dir.join(format!("sprf_v3_sqlite_bench.{}.db-wal", trial));
        let shm = db_dir.join(format!("sprf_v3_sqlite_bench.{}.db-shm", trial));
        std::fs::remove_file(wal).ok();
        std::fs::remove_file(shm).ok();

        assert_eq!(
            rows, on_disk,
            "emitted rows ({}) != on-disk rows ({})",
            rows, on_disk,
        );
        eprintln!(
            "# trial {}/{}: wall_ms={:.1} rows={} rows/s={:.0} hits={} db_bytes={} rss_kb={}",
            trial + 1, trials, wall_ms, rows, rows_per_sec, hit_count, db_bytes, rss_kb,
        );
    }
}
