//! HERMETIC head-to-head. Each engine runs in its OWN child process — it is the
//! only thing linked-active and the only thing measured, so no engine's resident
//! state, allocator counter, or cache warmth pollutes another's numbers. The
//! parent is a driver: it spawns one child per engine over the SAME deterministic
//! graph (each child regenerates it, byte-identical), collects a blake3 hash of
//! each child's sorted survivor set plus its isolated timing/heap, then asserts
//! the correct engines agree bit-for-bit.
//!
//!   cargo run --release --example head2head -- <layers> <width> [--cyclic [stride]]
//!
//! Correctness is proven by HASH EQUALITY across processes (oracle == dred == dd);
//! counting is reported and allowed to diverge on a phantom cycle. Timing/heap are
//! trustworthy precisely because each is measured alone.

use std::time::Instant;

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sprefa_store::{benchgraph, benchgraph::MultiGraph, memcap, relstore::RelStore};

use differential_dataflow::input::Input;
use differential_dataflow::operators::Iterate;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use timely::dataflow::operators::probe::Handle as ProbeHandle;

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

fn peak_rss_mb() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        let bytes = if cfg!(target_os = "linux") { ru.ru_maxrss as f64 * 1024.0 } else { ru.ru_maxrss as f64 };
        bytes / (1024.0 * 1024.0)
    }
}

/// Stable fingerprint of a survivor set: blake3 over the sorted keys' LE bytes,
/// first 16 hex. Two engines agree iff their fingerprints match — a cross-process
/// byte-identical check that never materializes both sets in one place.
fn fingerprint(keys: &[i64]) -> String {
    let mut h = blake3::Hasher::new();
    for k in keys {
        h.update(&k.to_le_bytes());
    }
    h.finalize().to_hex()[..16].to_string()
}

fn build(layers: usize, width: usize, back_stride: usize) -> MultiGraph {
    if back_stride == 0 {
        benchgraph::gen_multi(layers, width)
    } else {
        benchgraph::gen_multi_cyclic(layers, width, back_stride)
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

/// The one machine-parseable line each child prints. Setup is untimed; `ms` is the
/// isolated retract; `live` is the Rust heap the memcap gun caps, read right after.
fn emit(engine: &str, keys: &[i64], ms: f64, live_mb: f64, db_mb: f64) {
    println!(
        "RESULT engine={engine} count={} hash={} ms={ms:.2} live_mb={live_mb:.1} rss_mb={:.1} db_mb={db_mb:.1}",
        keys.len(),
        fingerprint(keys),
        peak_rss_mb()
    );
}

// ---- child engines (each runs ALONE in its process) ---------------------------

fn child_oracle(g: &MultiGraph) {
    let t = Instant::now();
    let keys: Vec<i64> = benchgraph::oracle_survivors(g, g.seed).into_iter().collect();
    emit("oracle", &keys, t.elapsed().as_secs_f64() * 1e3, memcap::live_bytes() as f64 / 1048576.0, 0.0);
}

fn child_dd(g: &MultiGraph) {
    let edges: Vec<(i64, i64)> =
        g.edges.iter().map(|(pt, pi, ct, ci)| (benchgraph::encode(*pt, *pi), benchgraph::encode(*ct, *ci))).collect();
    let roots = roots_from(g);
    let cut = benchgraph::encode(g.seed.0, g.seed.1);
    let alive: Arc<Mutex<HashMap<i64, isize>>> = Arc::new(Mutex::new(HashMap::new()));
    let alive_out = alive.clone();
    let ms = Arc::new(Mutex::new(0.0f64));
    let ms_out = ms.clone();
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
            reach
                .consolidate()
                .inspect(move |(node, _t, diff)| { *acc.lock().unwrap().entry(*node).or_insert(0) += *diff; })
                .probe_with(&mut probe);
            (edges_in, roots_in)
        });
        for (p, c) in &edges { edges_in.insert((*p, *c)); }
        for r in &roots { roots_in.insert(*r); }
        edges_in.advance_to(1); roots_in.advance_to(1); edges_in.flush(); roots_in.flush();
        worker.step_while(|| probe.less_than(edges_in.time())); // SETUP (untimed)
        // MEASURED: the incremental retract at time 2, alone.
        let t = Instant::now();
        roots_in.remove(cut);
        edges_in.advance_to(2); roots_in.advance_to(2); edges_in.flush(); roots_in.flush();
        worker.step_while(|| probe.less_than(roots_in.time()));
        *ms_out.lock().unwrap() = t.elapsed().as_secs_f64() * 1e3;
    });
    let map = alive_out.lock().unwrap();
    let mut keys: Vec<i64> = map.iter().filter(|(_, w)| **w > 0).map(|(d, _)| *d).collect();
    keys.sort_unstable();
    let live = memcap::live_bytes() as f64 / 1048576.0;
    emit("dd", &keys, *ms.lock().unwrap(), live, 0.0);
}

async fn open_store() -> (RelStore, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("h2h_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    opt.max_connections(1).min_connections(1);
    (RelStore::attach(Database::connect(opt).await.unwrap()).await.unwrap(), path)
}

/// Shared store child body: load the graph, drop ALL staging, run `op`, measure.
async fn child_store(g: MultiGraph, engine: &str, dred: bool) {
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> =
        g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    let seed = (g.seed.0 as i64, g.seed.1);
    let (store, path) = open_store().await;
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    drop(rows);
    drop(deps);
    drop(g); // corpus is on disk now; retract heap must not count it

    let t = Instant::now();
    if dred { store.retract_dred(&[seed]).await.unwrap(); } else { store.retract(&[seed]).await.unwrap(); }
    let ms = t.elapsed().as_secs_f64() * 1e3;
    let live = memcap::live_bytes() as f64 / 1048576.0;
    let keys = store.alive_keys().await.unwrap();

    store.conn().execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE);").await.ok();
    let db_mb = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) as f64 / 1048576.0;
    emit(engine, &keys, ms, live, db_mb);
    drop(store);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

// ---- driver -------------------------------------------------------------------

#[derive(Default)]
struct Row {
    count: i64,
    hash: String,
    ms: f64,
    live: f64,
    rss: f64,
    db: f64,
    ok: bool,
}

fn run_child(exe: &std::path::Path, engine: &str, l: usize, w: usize, bs: usize, cap: u64) -> Row {
    let out = std::process::Command::new(exe)
        .args([engine, &l.to_string(), &w.to_string(), &bs.to_string()])
        .env("DL_MEMCAP_MB", cap.to_string())
        .output()
        .unwrap();
    let mut row = Row { ok: out.status.success(), ..Default::default() };
    let s = String::from_utf8_lossy(&out.stdout);
    if let Some(line) = s.lines().find(|l| l.starts_with("RESULT")) {
        for tok in line.split_whitespace().skip(1) {
            let Some((k, v)) = tok.split_once('=') else { continue };
            match k {
                "count" => row.count = v.parse().unwrap_or(0),
                "hash" => row.hash = v.to_string(),
                "ms" => row.ms = v.parse().unwrap_or(0.0),
                "live_mb" => row.live = v.parse().unwrap_or(0.0),
                "rss_mb" => row.rss = v.parse().unwrap_or(0.0),
                "db_mb" => row.db = v.parse().unwrap_or(0.0),
                _ => {}
            }
        }
    }
    row
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cap_mb: u64 = std::env::var("DL_MEMCAP_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(4096);
    if cap_mb != 0 {
        memcap::cap_address_space_mb(cap_mb);
    }

    // CHILD: `<engine> <layers> <width> <back_stride>` — one engine, alone.
    // Detect by the engine NAME in arg 1 (never a bare number), so the driver form
    // `<layers> <width> --cyclic <stride>` is never misread as a child.
    let is_child = args.get(1).map(|a| matches!(a.as_str(), "oracle" | "dd" | "dred" | "count")).unwrap_or(false);
    if is_child {
        let l: usize = args[2].parse().unwrap();
        let w: usize = args[3].parse().unwrap();
        let bs: usize = args[4].parse().unwrap();
        let g = build(l, w, bs);
        match args[1].as_str() {
            "oracle" => child_oracle(&g),
            "dd" => child_dd(&g),
            "dred" => child_store(g, "dred", true).await,
            "count" => child_store(g, "count", false).await,
            other => eprintln!("unknown engine {other}"),
        }
        return;
    }

    // DRIVER
    let layers: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(6).clamp(1, 20);
    let width: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10_000).clamp(1, 500_000);
    let cyclic = args.iter().any(|a| a == "--cyclic");
    let bs: usize = if cyclic {
        args.iter().position(|a| a == "--cyclic").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(7)
    } else {
        0
    };
    let cap = if cap_mb == 0 { 4096 } else { cap_mb };
    let mode = if cyclic { format!("CYCLIC(stride={bs})") } else { "DAG".to_string() };
    let exe = std::env::current_exe().unwrap();

    eprintln!("[head2head] HERMETIC (one process per engine)  {mode}  layers={layers} width={width}  memcap={cap}MB");
    let oracle = run_child(&exe, "oracle", layers, width, bs, cap);
    let dred = run_child(&exe, "dred", layers, width, bs, cap);
    let dd = run_child(&exe, "dd", layers, width, bs, cap);
    let count = run_child(&exe, "count", layers, width, bs, cap);

    let mark = |r: &Row| -> &'static str {
        if !r.ok { "ABORT" } else if r.hash == oracle.hash { "OK   " } else { "WRONG" }
    };
    eprintln!("  engine        survivors  correct   ms      live_mb  rss_mb  db_mb   hash");
    for (name, r) in [("oracle", &oracle), ("sqlite-dred", &dred), ("dd", &dd), ("sqlite-count", &count)] {
        eprintln!(
            "  {name:<13} {:>9}  {:<7} {:>7.1}  {:>7.1}  {:>6.1}  {:>5.1}   {}",
            r.count, if name == "oracle" { "ref  " } else { mark(r) }, r.ms, r.live, r.rss, r.db, r.hash
        );
    }

    // COMPLETENESS: the cycle-safe store and dd must both match the oracle hash.
    assert!(oracle.ok, "oracle child failed");
    assert!(dred.ok && dred.hash == oracle.hash, "sqlite-dred disagreed with the oracle (hash mismatch)");
    assert!(dd.ok && dd.hash == oracle.hash, "dd disagreed with the oracle (hash mismatch)");
    if cyclic {
        if count.ok && count.hash == oracle.hash {
            eprintln!("  NOTE: counting matched at stride={bs} (no root-anchored phantom); raise density to see it fail.");
        } else if count.ok {
            eprintln!("  DEMONSTRATED: counting kept {} phantom rows (oracle {}) — wrong on the cycle.", count.count - oracle.count, oracle.count);
        }
    } else {
        assert!(count.ok && count.hash == oracle.hash, "counting must be correct on a DAG");
    }
    eprintln!("  COMPLETE: oracle == sqlite-dred == dd  (byte-identical survivor sets, measured hermetically)");
}
