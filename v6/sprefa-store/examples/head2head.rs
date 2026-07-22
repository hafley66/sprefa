//! HONEST head-to-head, one process, SQLite = source of truth.
//!
//! Builds one graph, computes an INDEPENDENT oracle (in-Rust BFS from surviving
//! roots), then runs three engines over byte-identical input and checks each
//! against the oracle:
//!   1. store `retract_dred`  (cycle-safe SQLite cascade — the real RelStore code)
//!   2. differential-dataflow  (resident IVM baseline)
//!   3. store `retract`        (counting Z-set — correct only on a DAG)
//!
//! On a DAG all three match the oracle. With `--cyclic` the counting cascade is
//! PROVABLY wrong (phantom cycles keep dead nodes alive) while DRed and dd stay
//! correct — the demonstration, measured, not asserted. The run ABORTS if the
//! oracle, DRed, and dd ever disagree (that is the completeness guarantee).
//!
//!   cargo run --release --example head2head -- <layers> <width> [--cyclic [stride]]

use std::time::Instant;

use sea_orm::{ConnectOptions, Database};
use sprefa_store::{benchgraph, memcap, relstore::RelStore};

use differential_dataflow::input::Input;
use differential_dataflow::operators::Iterate;
use timely::dataflow::operators::probe::Handle as ProbeHandle;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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

async fn open_store() -> (RelStore, std::path::PathBuf) {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let uniq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("head2head_{}_{uniq}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    opt.max_connections(1).min_connections(1);
    let store = RelStore::attach(Database::connect(opt).await.unwrap()).await.unwrap();
    (store, path)
}

fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

/// dd reachability: insert edges + roots, take the reachable set, retract the cut
/// root, return the surviving node keys (sorted). Same dataflow as dd_reach.rs.
fn dd_survivors(edges: &[(i64, i64)], roots: &[i64], cut: i64) -> Vec<i64> {
    let edges = edges.to_vec();
    let roots = roots.to_vec();
    let alive: Arc<Mutex<HashMap<i64, isize>>> = Arc::new(Mutex::new(HashMap::new()));
    let alive_out = alive.clone();
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
                .inspect(move |(node, _t, diff)| {
                    *acc.lock().unwrap().entry(*node).or_insert(0) += *diff;
                })
                .probe_with(&mut probe);
            (edges_in, roots_in)
        });
        for (p, c) in &edges {
            edges_in.insert((*p, *c));
        }
        for r in &roots {
            roots_in.insert(*r);
        }
        edges_in.advance_to(1);
        roots_in.advance_to(1);
        edges_in.flush();
        roots_in.flush();
        worker.step_while(|| probe.less_than(edges_in.time()));
        // retract the cut root at time 2
        roots_in.remove(cut);
        edges_in.advance_to(2);
        roots_in.advance_to(2);
        edges_in.flush();
        roots_in.flush();
        worker.step_while(|| probe.less_than(roots_in.time()));
    });
    let map = alive_out.lock().unwrap();
    let mut ids: Vec<i64> = map.iter().filter(|(_, w)| **w > 0).map(|(d, _)| *d).collect();
    ids.sort_unstable();
    ids
}

#[tokio::main]
async fn main() {
    let cap_mb: u64 = std::env::var("DL_MEMCAP_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(4096);
    if cap_mb != 0 {
        memcap::cap_address_space_mb(cap_mb);
    }
    let args: Vec<String> = std::env::args().collect();
    let layers: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(6).clamp(1, 20);
    let width: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10_000).clamp(1, 500_000);
    let cyclic = args.iter().any(|a| a == "--cyclic");
    let back_stride: usize = if cyclic {
        // arg after --cyclic, default 7 (a sparse-but-present cycle density).
        args.iter().position(|a| a == "--cyclic").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(7)
    } else {
        0
    };

    let g = if cyclic {
        benchgraph::gen_multi_cyclic(layers, width, back_stride)
    } else {
        benchgraph::gen_multi(layers, width)
    };
    let n = g.rows.len();
    let n_edges = g.edges.len();
    let seed = g.seed;

    // ---- ORACLE (independent referee) --------------------------------------
    let oracle = benchgraph::oracle_survivors(&g, seed);
    let oracle_vec: Vec<i64> = oracle.iter().copied().collect();

    let mode = if cyclic { format!("CYCLIC(stride={back_stride})") } else { "DAG".to_string() };
    eprintln!("[head2head] {mode}  rels·rows={n} edges={n_edges}  oracle_survivors={}", oracle.len());

    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> = g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    let seed_pair = (seed.0 as i64, seed.1);

    // ---- 1. store retract_dred (cycle-safe, the source of truth) ------------
    let (store, path) = open_store().await;
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    let t = Instant::now();
    let dred_rounds = store.retract_dred(&[seed_pair]).await.unwrap();
    let dred_ms = t.elapsed().as_secs_f64() * 1e3;
    let dred_keys = store.alive_keys().await.unwrap();
    let dred_ok = dred_keys == oracle_vec;
    drop(store);
    cleanup(&path);

    // ---- 2. differential-dataflow (resident baseline) ----------------------
    let edge_list: Vec<(i64, i64)> = g.edges.iter().map(|(pt, pi, ct, ci)| (benchgraph::encode(*pt, *pi), benchgraph::encode(*ct, *ci))).collect();
    // roots = the indeg-0 rows (encoded). dd seeds reachability from them, then
    // retracts the cut root — exactly the oracle's surviving-root set plus the cut.
    let roots = roots_from(&g);
    let cut_enc = benchgraph::encode(seed.0, seed.1);
    let t = Instant::now();
    let dd_keys = dd_survivors(&edge_list, &roots, cut_enc);
    let dd_ms = t.elapsed().as_secs_f64() * 1e3;
    let dd_ok = dd_keys == oracle_vec;

    // ---- 3. store counting retract (correct only on a DAG) -----------------
    let (store2, path2) = open_store().await;
    store2.add_rows(&rows).await.unwrap();
    store2.add_deps(&deps).await.unwrap();
    let t = Instant::now();
    let count_rounds = store2.retract(&[seed_pair]).await.unwrap();
    let count_ms = t.elapsed().as_secs_f64() * 1e3;
    let count_keys = store2.alive_keys().await.unwrap();
    let count_ok = count_keys == oracle_vec;
    drop(store2);
    cleanup(&path2);

    // ---- report -------------------------------------------------------------
    let yn = |b: bool| if b { "OK  " } else { "WRONG" };
    eprintln!("  engine          survivors  correct  rounds   ms");
    eprintln!("  oracle          {:>9}  {:>5}    {:>5}   {:>7}", oracle.len(), "ref", "-", "-");
    eprintln!("  sqlite-dred     {:>9}  {:>5}    {:>5}   {:>7.1}", dred_keys.len(), yn(dred_ok), dred_rounds, dred_ms);
    eprintln!("  dd (resident)   {:>9}  {:>5}    {:>5}   {:>7.1}", dd_keys.len(), yn(dd_ok), "-", dd_ms);
    eprintln!("  sqlite-count    {:>9}  {:>5}    {:>5}   {:>7.1}", count_keys.len(), yn(count_ok), count_rounds, count_ms);
    eprintln!("  peak_rss {:.1} MB", peak_rss_mb());

    println!("CSV,head2head,{mode},{n},{n_edges},{},dred,{dred_ok},{dred_ms:.3}", oracle.len());
    println!("CSV,head2head,{mode},{n},{n_edges},{},dd,{dd_ok},{dd_ms:.3}", oracle.len());
    println!("CSV,head2head,{mode},{n},{n_edges},{},count,{count_ok},{count_ms:.3}", oracle.len());

    // COMPLETENESS GUARANTEE: the cycle-safe SQLite store and dd MUST both equal
    // the oracle. Counting is ALLOWED to diverge on cycles (that is the point) —
    // it is reported, not asserted.
    assert!(dred_ok, "sqlite-dred disagreed with the oracle — cycle-safe retraction is INCORRECT");
    assert!(dd_ok, "dd disagreed with the oracle — the baseline itself is wrong, check the harness");
    if cyclic && count_ok {
        eprintln!("  NOTE: counting matched on this cyclic graph (back_stride={back_stride} produced no root-anchored phantom); raise density to see it fail.");
    }
    if !cyclic {
        assert!(count_ok, "counting disagreed on a DAG — it must be correct there");
    }
    eprintln!("  COMPLETE: oracle == sqlite-dred == dd  (byte-identical survivor sets)");
}

/// The indeg-0 rows (roots), encoded — the exact seed set dd's reachability starts from.
fn roots_from(g: &benchgraph::MultiGraph) -> Vec<i64> {
    use std::collections::HashSet;
    let mut has_parent: HashSet<i64> = HashSet::new();
    for (_pt, _pi, ct, ci) in &g.edges {
        has_parent.insert(benchgraph::encode(*ct, *ci));
    }
    let mut roots: Vec<i64> = g
        .rows
        .iter()
        .map(|(t, i, _)| benchgraph::encode(*t, *i))
        .filter(|k| !has_parent.contains(k))
        .collect();
    roots.sort_unstable();
    roots
}
