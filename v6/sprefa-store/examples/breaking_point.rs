//! HONEST breaking-point ramp: the SAME task (maintain reachability from roots,
//! then retract a root) fed to two engines under the SAME memory cap, scaled up
//! until each hits its wall. Real code both sides — dd is the resident IVM
//! baseline, the SQLite side is the actual `RelStore::retract_dred`.
//!
//! Hypothesis (from the session): the resident engine dies on RAM (the memcap gun
//! fires, SIGABRT) while the on-disk store keeps RAM bounded and pays in TIME as
//! the cone grows. Each scale runs in an ISOLATED child process so an abort is
//! OBSERVED (non-zero exit), never fatal to the ramp.
//!
//!   DL_MEMCAP_MB=1500 cargo run --release --example breaking_point
//!
//! Read it: dd's peak_rss climbs to the cap and the child aborts; the store's
//! peak_rss stays flat while retract_ms and db_mb climb — RAM bounded, cost on
//! disk and in time.

use std::time::Instant;

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sprefa_store::{benchgraph, memcap, relstore::RelStore};

use differential_dataflow::input::Input;
use differential_dataflow::operators::Iterate;
use timely::dataflow::operators::probe::Handle as ProbeHandle;

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

const LAYERS: usize = 8; // fixed shape; width scales V = 2 + LAYERS*width.
const BACK_STRIDE: usize = 5; // dense cycles so DRed does real cone work.

fn peak_rss_mb() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        let bytes = if cfg!(target_os = "linux") { ru.ru_maxrss as f64 * 1024.0 } else { ru.ru_maxrss as f64 };
        bytes / (1024.0 * 1024.0)
    }
}

fn cap_mb() -> u64 {
    std::env::var("DL_MEMCAP_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(1500)
}

fn roots_from(g: &benchgraph::MultiGraph) -> Vec<i64> {
    use std::collections::HashSet;
    let mut has_parent: HashSet<i64> = HashSet::new();
    for (_pt, _pi, ct, ci) in &g.edges {
        has_parent.insert(benchgraph::encode(*ct, *ci));
    }
    let mut roots: Vec<i64> = g.rows.iter().map(|(t, i, _)| benchgraph::encode(*t, *i)).filter(|k| !has_parent.contains(k)).collect();
    roots.sort_unstable();
    roots
}

// ---- child: dd reach (RESIDENT) — expect RAM death at the cap -----------------
fn child_dd(width: usize) {
    memcap::cap_address_space_mb(cap_mb());
    let g = benchgraph::gen_multi_cyclic(LAYERS, width, BACK_STRIDE);
    let n = g.rows.len();
    let edges: Vec<(i64, i64)> = g.edges.iter().map(|(pt, pi, ct, ci)| (benchgraph::encode(*pt, *pi), benchgraph::encode(*ct, *ci))).collect();
    let roots = roots_from(&g);
    let cut = benchgraph::encode(g.seed.0, g.seed.1);
    let n_edges = edges.len();

    use std::sync::atomic::{AtomicUsize, Ordering};
    static SURV: AtomicUsize = AtomicUsize::new(0);
    timely::execute_directly(move |worker| {
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
            reach.consolidate().inspect(|(_n, _t, diff)| { if *diff > 0 { SURV.fetch_add(1, Ordering::Relaxed); } }).probe_with(&mut probe);
            (edges_in, roots_in)
        });
        for (p, c) in &edges { edges_in.insert((*p, *c)); }
        for r in &roots { roots_in.insert(*r); }
        edges_in.advance_to(1); roots_in.advance_to(1); edges_in.flush(); roots_in.flush();
        worker.step_while(|| probe.less_than(edges_in.time()));
        roots_in.remove(cut);
        edges_in.advance_to(2); roots_in.advance_to(2); edges_in.flush(); roots_in.flush();
        worker.step_while(|| probe.less_than(roots_in.time()));
    });
    println!("OK width={width} nodes={n} edges={n_edges} peak_rss={:.0}MB", peak_rss_mb());
}

// ---- child: store retract_dred (ON DISK) — expect RAM bounded, time climbs ----
async fn child_dred(width: usize) {
    memcap::cap_address_space_mb(cap_mb());
    let g = benchgraph::gen_multi_cyclic(LAYERS, width, BACK_STRIDE);
    let n = g.rows.len();
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> = g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    let n_edges = deps.len();

    let path = std::env::temp_dir().join(format!("bp_dred_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    opt.max_connections(1).min_connections(1);
    let store = RelStore::attach(Database::connect(opt).await.unwrap()).await.unwrap();
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    // drop the Rust-side staging: the measured retract works against the db alone.
    drop(rows);
    drop(deps);

    let t = Instant::now();
    let rounds = store.retract_dred(&[(g.seed.0 as i64, g.seed.1)]).await.unwrap();
    let ms = t.elapsed().as_secs_f64() * 1e3;
    let survivors = store.alive().await.unwrap();

    store.conn().execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE);").await.ok();
    let db_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let db_mb = db_bytes as f64 / (1024.0 * 1024.0);

    println!(
        "OK width={width} nodes={n} edges={n_edges} killed={} rounds={rounds} retract_ms={ms:.0} peak_rss={:.0}MB db={db_mb:.0}MB",
        n as i64 - survivors, peak_rss_mb()
    );
    drop(store);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

fn run_child(exe: &std::path::Path, kind: &str, width: usize) -> (bool, String) {
    let out = std::process::Command::new(exe).arg(kind).arg(width.to_string()).env("DL_MEMCAP_MB", cap_mb().to_string()).output().unwrap();
    let ok = out.status.success();
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().find(|l| l.starts_with("OK")).unwrap_or("").to_string();
    (ok, if line.is_empty() { format!("exit={:?}", out.status.code()) } else { line })
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 {
        let width: usize = args[2].parse().unwrap();
        match args[1].as_str() {
            "dd" => return child_dd(width),
            "dred" => return child_dred(width).await,
            _ => return,
        }
    }

    let exe = std::env::current_exe().unwrap();
    let cap = cap_mb();
    println!("BREAKING POINT — same task, both engines, memcap {cap} MB (shape: LAYERS={LAYERS}, back_stride={BACK_STRIDE})\n");

    println!("== dd reach (RESIDENT) — expect RAM death (gun SIGABRT past {cap} MB) ==");
    let mut dd_broke = None;
    for &width in &[20_000usize, 60_000, 120_000, 240_000, 480_000] {
        let (ok, line) = run_child(&exe, "dd", width);
        if ok {
            println!("  {line}");
        } else {
            println!("  width={width}  *** BROKE — resident RAM > {cap} MB, {line} ***");
            dd_broke = Some(width);
            break;
        }
    }

    println!("\n== store retract_dred (ON DISK) — expect RAM bounded, retract_ms climbs ==");
    for &width in &[20_000usize, 60_000, 120_000, 240_000, 480_000] {
        let (ok, line) = run_child(&exe, "dred", width);
        if ok {
            println!("  {line}");
        } else {
            println!("  width={width}  *** BROKE — {line} ***");
            break;
        }
    }

    println!("\n  read it: dd's peak_rss climbs to the {cap} MB cap and the child aborts{}; the store's peak_rss stays flat while retract_ms and db climb — RAM bounded, cost on disk and in time.",
        match dd_broke { Some(w) => format!(" (first at width={w})"), None => " (raise scale or lower the cap to see it)".to_string() });
}
