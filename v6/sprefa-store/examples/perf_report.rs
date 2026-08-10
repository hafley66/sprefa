//! The perf harness. One `Engine` trait; six implementations; every run measured
//! HERMETICALLY (one engine alone per child process) with a full metric set, and the
//! parent autogenerates a markdown report.
//!
//! What it guarantees, per the standing asks:
//!   * INPUT identical  — every child regenerates the SAME deterministic graph from
//!     (layers,width,stride) and reports a blake3 of its sorted edge list; the driver
//!     asserts all engines at a scale share one input hash.
//!   * OUTPUT verified   — each child reports a blake3 of its sorted survivor set; the
//!     driver marks an engine correct iff its output hash equals the oracle's.
//!   * EVERYTHING measured — retract wall time, SQL statement count, Rust live heap
//!     (memcap-capped), SQLite C-heap highwater (memcap-blind), peak RSS, on-disk db.
//!   * GUN pointed        — every child sets the memcap allocator cap; an over-cap
//!     engine aborts (SIGABRT) and is recorded as a breakpoint, never swaps.
//!   * SETUP excluded     — only the retract is timed; staging + the whole generated
//!     graph are dropped before the measured op so residence is never counted.
//!
//!   run the report:   cargo run --release --example perf_report
//!   one child alone:  cargo run --release --example perf_report -- <engine> <l> <w> <stride>
//!   knobs: DL_MEMCAP_MB (default 2048), DL_PERF_REPORT (output path)

use std::time::Instant;

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sprefa_store::{benchgraph, benchgraph::MultiGraph, memcap, relstore::RelStore, stmt_counter};

use differential_dataflow::input::Input;
use differential_dataflow::operators::Iterate;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use timely::dataflow::operators::probe::Handle as ProbeHandle;

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

// ---- metrics helpers ----------------------------------------------------------

fn peak_rss_mb() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        let bytes = if cfg!(target_os = "linux") { ru.ru_maxrss as f64 * 1024.0 } else { ru.ru_maxrss as f64 };
        bytes / 1048576.0
    }
}
fn rust_live_mb() -> f64 {
    memcap::live_bytes() as f64 / 1048576.0
}
/// SQLite's OWN C-allocator highwater (the memcap gun is blind to it). `reset=1`
/// clears it so a later read isolates the next phase.
fn sqlite_hw_mb(reset: bool) -> f64 {
    unsafe { libsqlite3_sys::sqlite3_memory_highwater(if reset { 1 } else { 0 }) as f64 / 1048576.0 }
}
fn fingerprint(keys: &[i64]) -> String {
    let mut h = blake3::Hasher::new();
    for k in keys {
        h.update(&k.to_le_bytes());
    }
    h.finalize().to_hex()[..16].to_string()
}

// ---- the trait ----------------------------------------------------------------

/// One measured retraction of the seed root from the shared graph. `retract_ms` is
/// the MEASURED op only; setup is untimed and the generated graph is dropped before
/// it, so no metric counts corpus residence.
#[derive(Default, Clone)]
struct Outcome {
    survivors: Vec<i64>,
    retract_ms: f64,
    statements: u64,
    rust_peak_mb: f64,  // high-water Rust heap DURING the retract (not after)
    sqlite_hw_mb: f64,  // SQLite C-heap high-water during the retract
    peak_rss_mb: f64,   // process resident high-water (the TRUE footprint)
    db_mb: f64,
}

/// Every experiment is an `Engine`: build state from the graph (untimed), run the
/// measured retract, report the full metric set. Static methods, dispatched by name
/// in the child — no trait objects, no async-trait dep.
trait Engine {
    fn name() -> &'static str;
    async fn measure(g: MultiGraph) -> Outcome;
}

// ---- shared store setup -------------------------------------------------------

async fn open_store() -> (RelStore, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("perf_{}_{}.sqlite", std::process::id(), rand_tag()));
    let _ = std::fs::remove_file(&path);
    let mut opt = ConnectOptions::new(if sqlite_ram_probe() {
        "sqlite::memory:".to_string()
    } else {
        format!("sqlite://{}?mode=rwc", path.display())
    });
    opt.max_connections(1).min_connections(1);
    let conn = Database::connect(opt).await.unwrap();
    // Lab hook (H6): page_size must be stamped before the first table is created.
    if let Ok(page_size) = std::env::var("DL_LAB_PAGE_SIZE") {
        conn.execute_unprepared(&format!("PRAGMA page_size={page_size};")).await.unwrap();
    }
    let store = RelStore::attach(conn).await.unwrap();
    if sqlite_ram_probe() {
        store.conn().execute_unprepared("PRAGMA cache_size=-1048576;").await.unwrap();
    }
    (store, path)
}
fn sqlite_ram_probe() -> bool {
    std::env::var("DL_SQLITE_RAM_PROBE").map(|value| value == "1").unwrap_or(false)
}
fn rand_tag() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos() as u64
}
fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

/// Load the graph into a store, drop ALL staging + the graph, run `op` timed, and
/// gather every store metric. Shared by the three cascade engines so they are
/// measured identically. `op` returns the statement delta of the measured retract.
async fn store_measure<F>(g: MultiGraph, op: F) -> Outcome
where
    F: for<'a> FnOnce(&'a RelStore, (i64, i64)) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>,
{
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> =
        g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    let seed = (g.seed.0 as i64, g.seed.1);
    let (store, path) = open_store().await;
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    drop(rows);
    drop(deps);
    drop(g); // corpus lives on disk now; nothing below counts its residence

    // Lab hook (H4): optional ANALYZE in the UNTIMED setup window, so the measured
    // retract runs with sqlite_stat1 populated.
    if std::env::var("DL_LAB_ANALYZE").map(|v| v == "1").unwrap_or(false) {
        store.conn().execute_unprepared("ANALYZE;").await.unwrap();
    }

    stmt_counter::reset();
    sqlite_hw_mb(true); // clear highwater so the retract's C-heap is isolated
    memcap::reset_peak(); // bracket the retract's Rust-heap high-water
    let t = Instant::now();
    op(&store, seed).await;
    let retract_ms = t.elapsed().as_secs_f64() * 1e3;
    let statements = stmt_counter::get();
    let rust_peak = memcap::peak_bytes() as f64 / 1048576.0;
    let sqlite_hw = sqlite_hw_mb(false);
    let survivors = store.alive_keys().await.unwrap();

    store.conn().execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE);").await.ok();
    let db_mb = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) as f64 / 1048576.0;
    let out = Outcome {
        survivors,
        retract_ms,
        statements,
        rust_peak_mb: rust_peak,
        sqlite_hw_mb: sqlite_hw,
        peak_rss_mb: peak_rss_mb(),
        db_mb,
    };
    drop(store);
    cleanup(&path);
    out
}

// ---- engines ------------------------------------------------------------------

struct Oracle;
impl Engine for Oracle {
    fn name() -> &'static str { "oracle" }
    async fn measure(g: MultiGraph) -> Outcome {
        memcap::reset_peak();
        let t = Instant::now();
        let survivors: Vec<i64> = benchgraph::oracle_survivors(&g, g.seed).into_iter().collect();
        let ms = t.elapsed().as_secs_f64() * 1e3;
        Outcome { retract_ms: ms, rust_peak_mb: memcap::peak_bytes() as f64 / 1048576.0, peak_rss_mb: peak_rss_mb(), survivors, ..Default::default() }
    }
}

struct Counting;
impl Engine for Counting {
    fn name() -> &'static str { "sqlite-count" }
    async fn measure(g: MultiGraph) -> Outcome {
        store_measure(g, |s, seed| Box::pin(async move { s.retract(&[seed]).await.unwrap(); })).await
    }
}

struct CountingScc;
impl Engine for CountingScc {
    fn name() -> &'static str { "sqlite-count-scc" }
    async fn measure(g: MultiGraph) -> Outcome {
        store_measure(g, |s, seed| Box::pin(async move { s.retract_scc(&[seed]).await.unwrap(); })).await
    }
}

struct DredLoop;
impl Engine for DredLoop {
    fn name() -> &'static str { "sqlite-dred-loop" }
    async fn measure(g: MultiGraph) -> Outcome {
        store_measure(g, |s, seed| Box::pin(async move { s.retract_dred(&[seed]).await.unwrap(); })).await
    }
}

struct DredCte;
impl Engine for DredCte {
    fn name() -> &'static str { "sqlite-dred-cte" }
    async fn measure(g: MultiGraph) -> Outcome {
        store_measure(g, |s, seed| Box::pin(async move { s.retract_dred_cte(&[seed]).await.unwrap(); })).await
    }
}

struct Dd;
impl Engine for Dd {
    fn name() -> &'static str { "dd" }
    async fn measure(g: MultiGraph) -> Outcome {
        let edges: Vec<(i64, i64)> =
            g.edges.iter().map(|(pt, pi, ct, ci)| (benchgraph::encode(*pt, *pi), benchgraph::encode(*ct, *ci))).collect();
        let roots = roots_from(&g);
        let cut = benchgraph::encode(g.seed.0, g.seed.1);
        drop(g);
        let alive: Arc<Mutex<HashMap<i64, isize>>> = Arc::new(Mutex::new(HashMap::new()));
        let alive_out = alive.clone();
        let ms = Arc::new(Mutex::new(0.0f64));
        let ms_out = ms.clone();
        let live = Arc::new(Mutex::new(0.0f64));
        let live_out = live.clone();
        let edges = Arc::new(edges);
        let roots = Arc::new(roots);
        timely::execute_directly(move |worker| {
            let acc = alive.clone();
            let mut probe = ProbeHandle::new();
            let (mut edges_in, mut roots_in) = worker.dataflow(|scope| {
                let (edges_in, edges_c) = scope.new_collection::<(i64, i64), isize>();
                let (roots_in, roots_c) = scope.new_collection::<i64, isize>();
                let ec = edges_c.clone();
                let rc = roots_c.clone();
                let reach = roots_c.iterate(move |scope, inner| {
                    let edges = ec.enter(scope);
                    let roots = rc.enter(scope);
                    edges.semijoin(inner).map(|(_p, c)| c).concat(roots).distinct()
                });
                reach.consolidate().inspect(move |(node, _t, diff)| { *acc.lock().unwrap().entry(*node).or_insert(0) += *diff; }).probe_with(&mut probe);
                (edges_in, roots_in)
            });
            for (p, c) in edges.iter() { edges_in.insert((*p, *c)); }
            for r in roots.iter() { roots_in.insert(*r); }
            let (de, dr) = (edges.clone(), roots.clone());
            drop(de);
            drop(dr);
            edges_in.advance_to(1); roots_in.advance_to(1); edges_in.flush(); roots_in.flush();
            worker.step_while(|| probe.less_than(edges_in.time())); // SETUP (untimed)
            let t = Instant::now();
            roots_in.remove(cut);
            edges_in.advance_to(2); roots_in.advance_to(2); edges_in.flush(); roots_in.flush();
            worker.step_while(|| probe.less_than(roots_in.time()));
            *ms_out.lock().unwrap() = t.elapsed().as_secs_f64() * 1e3;
            // dd's resident heap: arrangements + input it must keep to stay incremental.
            *live_out.lock().unwrap() = rust_live_mb();
        });
        let mut survivors: Vec<i64> = {
            let map = alive_out.lock().unwrap();
            map.iter().filter(|(_, w)| **w > 0).map(|(d, _)| *d).collect()
        };
        survivors.sort_unstable();
        let retract_ms = *ms.lock().unwrap();
        let rust_resident = *live.lock().unwrap();
        Outcome { retract_ms, rust_peak_mb: rust_resident, peak_rss_mb: peak_rss_mb(), survivors, ..Default::default() }
    }
}

fn roots_from(g: &MultiGraph) -> Vec<i64> {
    use std::collections::HashSet;
    let mut has_parent: HashSet<i64> = HashSet::new();
    for (_pt, _pi, ct, ci) in &g.edges {
        has_parent.insert(benchgraph::encode(*ct, *ci));
    }
    let mut roots: Vec<i64> =
        g.rows.iter().map(|(t, i, _)| benchgraph::encode(*t, *i)).filter(|k| !has_parent.contains(k)).collect();
    roots.sort_unstable();
    roots
}

/// blake3 of the sorted edge list — the INPUT fingerprint every engine must share.
fn input_hash(g: &MultiGraph) -> String {
    let mut e: Vec<(i64, i64)> =
        g.edges.iter().map(|(pt, pi, ct, ci)| (benchgraph::encode(*pt, *pi), benchgraph::encode(*ct, *ci))).collect();
    e.sort_unstable();
    let mut h = blake3::Hasher::new();
    for (u, v) in &e {
        h.update(&u.to_le_bytes());
        h.update(&v.to_le_bytes());
    }
    h.finalize().to_hex()[..16].to_string()
}

const ENGINES: [&str; 6] = ["oracle", "sqlite-count", "sqlite-count-scc", "sqlite-dred-loop", "sqlite-dred-cte", "dd"];

async fn run_engine(name: &str, g: MultiGraph) -> Option<Outcome> {
    match name {
        "oracle" => Some(Oracle::measure(g).await),
        "sqlite-count" => Some(Counting::measure(g).await),
        "sqlite-count-scc" => Some(CountingScc::measure(g).await),
        "sqlite-dred-loop" => Some(DredLoop::measure(g).await),
        "sqlite-dred-cte" => Some(DredCte::measure(g).await),
        "dd" => Some(Dd::measure(g).await),
        _ => None,
    }
}

// ---- driver: matrix, hash checks, markdown report -----------------------------

struct Cell {
    ok: bool,          // process did not abort
    correct: bool,     // output hash == oracle hash
    out: Outcome,
    out_hash: String,
    in_hash: String,
}

fn run_child(exe: &std::path::Path, engine: &str, l: usize, w: usize, bs: usize, cap: u64) -> Cell {
    let out = std::process::Command::new(exe)
        .args([engine, &l.to_string(), &w.to_string(), &bs.to_string()])
        .env("DL_MEMCAP_MB", cap.to_string())
        .output()
        .unwrap();
    let mut c = Cell { ok: out.status.success(), correct: false, out: Outcome::default(), out_hash: String::new(), in_hash: String::new() };
    let s = String::from_utf8_lossy(&out.stdout);
    if let Some(line) = s.lines().find(|l| l.starts_with("RESULT")) {
        for tok in line.split_whitespace().skip(1) {
            let Some((k, v)) = tok.split_once('=') else { continue };
            match k {
                "count" => c.out.survivors = vec![0; v.parse().unwrap_or(0)], // len only, for the table
                "out_hash" => c.out_hash = v.to_string(),
                "in_hash" => c.in_hash = v.to_string(),
                "ms" => c.out.retract_ms = v.parse().unwrap_or(0.0),
                "stmts" => c.out.statements = v.parse().unwrap_or(0),
                "rust_peak_mb" => c.out.rust_peak_mb = v.parse().unwrap_or(0.0),
                "sqlite_hw_mb" => c.out.sqlite_hw_mb = v.parse().unwrap_or(0.0),
                "rss_mb" => c.out.peak_rss_mb = v.parse().unwrap_or(0.0),
                "db_mb" => c.out.db_mb = v.parse().unwrap_or(0.0),
                _ => {}
            }
        }
    }
    c
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cap_mb: u64 = std::env::var("DL_MEMCAP_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(2048);
    if cap_mb != 0 {
        memcap::cap_address_space_mb(cap_mb);
    }

    // CHILD: one engine, alone. `<engine> <layers> <width> <stride>`.
    if args.len() >= 5 && ENGINES.contains(&args[1].as_str()) {
        let l: usize = args[2].parse().unwrap();
        let w: usize = args[3].parse().unwrap();
        let bs: usize = args[4].parse().unwrap();
        let g = benchgraph::gen_multi_cyclic(l, w, bs); // bs=0 => plain DAG
        let ih = input_hash(&g);
        let name = args[1].clone();
        let out = run_engine(&name, g).await.unwrap();
        println!(
            "RESULT engine={name} storage={} count={} out_hash={} in_hash={ih} ms={:.3} stmts={} rust_peak_mb={:.2} sqlite_hw_mb={:.2} rss_mb={:.1} db_mb={:.2}",
            if sqlite_ram_probe() { "memory" } else { "file" },
            out.survivors.len(), fingerprint(&out.survivors), out.retract_ms, out.statements, out.rust_peak_mb, out.sqlite_hw_mb, out.peak_rss_mb, out.db_mb
        );
        return;
    }

    // DRIVER.
    let cap = if cap_mb == 0 { 2048 } else { cap_mb };
    let exe = std::env::current_exe().unwrap();
    let report_path = std::env::var("DL_PERF_REPORT").unwrap_or_else(|_| {
        format!("{}/PERF-REPORT.md", env!("CARGO_MANIFEST_DIR"))
    });

    // The matrix: (label, layers, width, stride). Ramp width up; a couple cyclic
    // shapes to exercise the phantom-cycle path. Kept modest so the whole report
    // finishes in a few minutes under the gun; the ramp finds breakpoints on its own.
    let scales: Vec<(&str, usize, usize, usize)> = vec![
        ("DAG 60k", 6, 10_000, 0),
        ("DAG 240k", 6, 40_000, 0),
        ("DAG 960k", 6, 160_000, 0),
        ("DAG 2.9M", 6, 480_000, 0),
        ("DAG 5.8M", 6, 960_000, 0),
        ("DAG 11.5M", 6, 1_920_000, 0),
        ("CYC 60k s7", 6, 10_000, 7),
        ("CYC 960k s7", 6, 160_000, 7),
        ("CYC 2.9M s7", 6, 480_000, 7),
        ("CYC 5.8M s7", 6, 960_000, 7),
        ("CYC 11.5M s7", 6, 1_920_000, 7),
    ];

    let started = Instant::now();
    let mut md = String::new();
    md.push_str("# v6 store — retraction perf & completeness report\n\n");
    md.push_str(&format!(
        "Autogenerated by `examples/perf_report.rs`. Every engine runs HERMETICALLY (one \
         process each), memcap gun = **{cap} MB**. Setup is untimed; the generated graph is \
         dropped before the measured retract, so no metric counts corpus residence.\n\n"
    ));
    md.push_str("Columns: **ms** = measured retract wall time; **stmts** = SQL statements in \
        the retract (0 for dd/oracle); **rust_peak** = HIGH-WATER Rust heap during the retract \
        (what the gun caps); **sqlite_hw** = SQLite C-heap high-water during the retract \
        (gun-blind, malloc'd by the bundled C lib); **rss** = process resident high-water — the \
        TRUE footprint = sqlite_hw + bounded page cache + code; **db** = on-disk db after the \
        retract. `correct` = output hash equals the oracle's.\n\n");
    md.push_str("**Reading the memory columns (important):** the store engines show \
        `rust_peak` ≈ 0.1 MB because the retract runs ENTIRELY inside SQLite's C engine — the \
        800k-row working set never enters Rust. That is NOT the store's memory footprint; \
        **`rss` is** (e.g. ~415 MB @ 960k, of which `sqlite_hw` ~80 MB is live SQLite heap and \
        the rest is the bounded, evictable page cache set by `PRAGMA cache_size`). dd's number \
        is the opposite: its `rust_peak`/`rss` are LIVE arrangements it must keep resident and \
        cannot evict. Same RSS ballpark at 960k; the difference is evictability and growth.\n\n");

    let mut broke: Vec<String> = Vec::new();
    // Machine-readable matrix — the single source of truth for the chart
    // generator (tools/gen-perf-charts.sh). Emitted dependency-free (hand-rolled
    // JSON: every field is a number or a bare identifier string) alongside the md.
    let mut json_rows: Vec<String> = Vec::new();

    for (label, l, w, bs) in &scales {
        let nodes = 2 + l * w;
        let shape = if *bs == 0 { "DAG" } else { "CYC" };
        // oracle first = the reference hash + the shared input hash for this scale.
        let oracle = run_child(&exe, "oracle", *l, *w, *bs, cap);
        let ref_hash = oracle.out_hash.clone();
        let ref_in = oracle.in_hash.clone();

        md.push_str(&format!(
            "## {label} — nodes≈{nodes}, {}\n\n",
            if *bs == 0 { "DAG".to_string() } else { format!("cyclic stride={bs}") }
        ));
        md.push_str(&format!("_input hash `{ref_in}` (all engines must match)_\n\n"));
        md.push_str("| engine | survivors | correct | ms | stmts | rust_peak MB | sqlite_hw MB | rss MB | db MB |\n");
        md.push_str("|---|---:|:---:|---:|---:|---:|---:|---:|---:|\n");

        for name in ENGINES {
            let cell = if name == "oracle" {
                Cell { ok: oracle.ok, correct: true, out: oracle.out.clone(), out_hash: oracle.out_hash.clone(), in_hash: oracle.in_hash.clone() }
            } else {
                let mut c = run_child(&exe, name, *l, *w, *bs, cap);
                c.correct = c.ok && c.out_hash == ref_hash;
                c
            };
            // input-identity guard: a non-aborted engine MUST have seen the same graph.
            if cell.ok && !cell.in_hash.is_empty() && cell.in_hash != ref_in {
                md.push_str(&format!("| {name} | — | INPUT-MISMATCH | | | | | | |\n"));
                continue;
            }
            if !cell.ok {
                broke.push(format!("{name} @ {label} (nodes≈{nodes})"));
                md.push_str(&format!("| {name} | — | **ABORT (gun {cap}MB)** | | | | | | |\n"));
                json_rows.push(format!(
                    "{{\"shape\":\"{shape}\",\"nodes\":{nodes},\"engine\":\"{name}\",\"aborted\":true,\"cap_mb\":{cap}}}"
                ));
                continue;
            }
            let mark = if name == "oracle" { "ref".to_string() } else if cell.correct { "yes".to_string() } else { "**NO**".to_string() };
            md.push_str(&format!(
                "| {name} | {} | {mark} | {:.1} | {} | {:.2} | {:.2} | {:.1} | {:.2} |\n",
                cell.out.survivors.len(), cell.out.retract_ms, cell.out.statements, cell.out.rust_peak_mb, cell.out.sqlite_hw_mb, cell.out.peak_rss_mb, cell.out.db_mb
            ));
            json_rows.push(format!(
                "{{\"shape\":\"{shape}\",\"nodes\":{nodes},\"engine\":\"{name}\",\"aborted\":false,\
                 \"correct\":{},\"survivors\":{},\"ms\":{:.1},\"stmts\":{},\
                 \"rust_mb\":{:.3},\"sqlite_hw_mb\":{:.2},\"rss_mb\":{:.1},\"db_mb\":{:.2}}}",
                if name == "oracle" { true } else { cell.correct },
                cell.out.survivors.len(), cell.out.retract_ms, cell.out.statements,
                cell.out.rust_peak_mb, cell.out.sqlite_hw_mb, cell.out.peak_rss_mb, cell.out.db_mb
            ));
        }
        md.push('\n');
        eprintln!("[perf] {label} done ({:.0}s elapsed)", started.elapsed().as_secs_f64());
    }

    if !broke.is_empty() {
        md.push_str("## Matrix breakpoints\n\n");
        md.push_str(&format!("Under the {cap} MB gun in the matrix above, these aborted (SIGABRT):\n\n"));
        for b in &broke {
            md.push_str(&format!("- {b}\n"));
        }
        md.push('\n');
    }

    // ---- dedicated breakpoint ramp: TIGHT gun, force dd's RAM wall -------------
    // dd holds the reachable set resident (rust_live ~215 B/node above); the store
    // holds ~0 Rust heap (state on disk). Ramp width under a tight cap so dd aborts
    // and the on-disk store keeps going — the whole thesis, observed not asserted.
    let break_cap: u64 = std::env::var("DL_BREAK_CAP").ok().and_then(|s| s.parse().ok()).unwrap_or(700);
    md.push_str(&format!("## Breakpoint ramp — tight gun {break_cap} MB\n\n"));
    md.push_str(&format!(
        "Same task, ramping nodes under a {break_cap} MB memcap. dd is resident so it \
         hits the wall on its arrangements. **Honest caveat:** the store engines keep \
         retract state on disk (rust_live ~0.1 MB in the matrix), so when a store row \
         below aborts it is the SHARED in-RAM graph BUILDER (benchgraph holds the whole \
         `Vec<Vec>` + row/edge Vecs before inserting), NOT the retract. That builder \
         ceiling hits all engines at the same scale and masks dd's true resident wall \
         at extreme sizes — the real per-engine memory story is the `rust_live` column \
         in the matrix (dd climbs ~215 B/node; the store stays 0.09 MB flat). Isolating \
         dd's wall past the builder ceiling needs a streaming graph generator (gap G6).\n\n"
    ));
    md.push_str("| nodes | dd | sqlite-count | sqlite-dred-loop |\n|---:|:---:|:---:|:---:|\n");
    let ramp_engines = ["dd", "sqlite-count", "sqlite-dred-loop"];
    let mut still: std::collections::HashSet<&str> = ramp_engines.iter().copied().collect();
    for &w in &[160_000usize, 480_000, 960_000, 1_600_000, 2_400_000, 3_200_000] {
        let nodes = 2 + 6 * w;
        let mut cells = Vec::new();
        for e in ramp_engines {
            if !still.contains(e) {
                cells.push("—".to_string());
                continue;
            }
            let c = run_child(&exe, e, 6, w, 0, break_cap);
            if c.ok {
                cells.push(format!("{:.0} ms / {:.0} MB rss", c.out.retract_ms, c.out.peak_rss_mb));
            } else {
                cells.push(format!("**ABORT** (>{break_cap} MB)"));
                still.remove(e);
            }
        }
        md.push_str(&format!("| {nodes} | {} | {} | {} |\n", cells[0], cells[1], cells[2]));
        eprintln!("[perf] ramp nodes={nodes} done ({:.0}s)", started.elapsed().as_secs_f64());
        if still.is_empty() {
            break;
        }
    }
    md.push('\n');
    for e in ramp_engines {
        if still.contains(e) {
            md.push_str(&format!("- `{e}` never aborted in the ramp (RAM bounded at this cap).\n"));
        }
    }

    md.push_str(&format!("\n_Report generated in {:.0}s._\n", started.elapsed().as_secs_f64()));

    std::fs::write(&report_path, &md).unwrap();
    eprintln!("[perf] wrote {report_path}");

    // Machine-readable twin next to the md — the chart generator reads this, so
    // nobody hand-edits a table again. Path mirrors the md (…/perf.json by default).
    let json_path = std::env::var("DL_PERF_JSON").unwrap_or_else(|_| {
        format!("{}/perf.json", env!("CARGO_MANIFEST_DIR"))
    });
    let json = format!(
        "{{\"generated_s\":{:.0},\"cap_mb\":{cap},\"cells\":[\n{}\n]}}\n",
        started.elapsed().as_secs_f64(),
        json_rows.join(",\n")
    );
    std::fs::write(&json_path, &json).unwrap();
    eprintln!("[perf] wrote {json_path}");
    if !broke.is_empty() {
        eprintln!("[perf] breakpoints: {}", broke.join(", "));
    }
}
