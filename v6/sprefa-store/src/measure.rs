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
