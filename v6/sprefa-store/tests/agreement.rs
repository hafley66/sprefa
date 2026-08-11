//! The completeness guarantee, in CI: over a MATRIX of graph shapes (DAG and
//! cyclic at several densities, several scales), the cycle-safe SQLite store
//! (`retract_dred`), differential-dataflow, and an independent in-Rust oracle
//! must produce BYTE-IDENTICAL survivor sets. If any two disagree the test fails
//! with the exact shape that broke — this is what "dd and our impl agree" means
//! as a standing check, not a one-off example run.
//!
//! Also cross-checks the CONTROL plane by asserting the counting cascade agrees
//! with the oracle on every DAG (where counting is correct) and is the ONLY thing
//! allowed to diverge on cycles.

use sea_orm::{ConnectOptions, Database};
use sprefa_store::{benchgraph, benchgraph::MultiGraph, relstore::RelStore};

use differential_dataflow::input::Input;
use differential_dataflow::operators::Iterate;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use timely::dataflow::operators::probe::Handle as ProbeHandle;

async fn open_store() -> (RelStore, std::path::PathBuf) {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let uniq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("agree_{}_{uniq}.sqlite", std::process::id()));
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

fn roots_from(g: &MultiGraph) -> Vec<i64> {
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

/// dd reachability from `roots`, then retract `cut`; sorted survivor keys.
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

async fn dred_survivors(g: &MultiGraph) -> Vec<i64> {
    let (store, path) = open_store().await;
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> =
        g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    store.retract_dred(&[(g.seed.0 as i64, g.seed.1)]).await.unwrap();
    let keys = store.alive_keys().await.unwrap();
    drop(store);
    cleanup(&path);
    keys
}

async fn count_survivors(g: &MultiGraph) -> Vec<i64> {
    let (store, path) = open_store().await;
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> =
        g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    store.retract(&[(g.seed.0 as i64, g.seed.1)]).await.unwrap();
    let keys = store.alive_keys().await.unwrap();
    drop(store);
    cleanup(&path);
    keys
}

async fn signed_delta_survivors(g: &MultiGraph) -> Vec<i64> {
    let (store, path) = open_store().await;
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> =
        g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    sprefa_store::cascade::retract_signed_delta(store.conn(), store.ns(), &[(g.seed.0 as i64, g.seed.1)])
        .await
        .unwrap();
    let keys = store.alive_keys().await.unwrap();
    drop(store);
    cleanup(&path);
    keys
}

async fn signed_delta_cte_survivors(g: &MultiGraph) -> Vec<i64> {
    let (store, path) = open_store().await;
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> = g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    sprefa_store::cascade::retract_signed_delta_v2(store.conn(), store.ns(), &[(g.seed.0 as i64, g.seed.1)])
        .await
        .unwrap();
    let keys = store.alive_keys().await.unwrap();
    drop(store);
    cleanup(&path);
    keys
}

async fn delta_fold_survivors(g: &MultiGraph) -> Vec<i64> {
    let (store, path) = open_store().await;
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> = g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    store.retract_delta_fold(&[(g.seed.0 as i64, g.seed.1)]).await.unwrap();
    let keys = store.alive_keys().await.unwrap();
    drop(store);
    cleanup(&path);
    keys
}

async fn count_scc_survivors(g: &MultiGraph) -> Vec<i64> {
    let (store, path) = open_store().await;
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> =
        g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    store.retract_scc(&[(g.seed.0 as i64, g.seed.1)]).await.unwrap();
    let keys = store.alive_keys().await.unwrap();
    drop(store);
    cleanup(&path);
    keys
}

/// The one check every shape runs: oracle == store-DRed == dd, byte-for-byte.
async fn assert_agreement(g: &MultiGraph, label: &str) {
    let oracle: Vec<i64> = benchgraph::oracle_survivors(g, g.seed).into_iter().collect();
    let dred = dred_survivors(g).await;
    let signed = signed_delta_survivors(g).await;
    let signed_cte = signed_delta_cte_survivors(g).await;
    let delta_fold = delta_fold_survivors(g).await;
    let edges: Vec<(i64, i64)> = g
        .edges
        .iter()
        .map(|(pt, pi, ct, ci)| (benchgraph::encode(*pt, *pi), benchgraph::encode(*ct, *ci)))
        .collect();
    let dd = dd_survivors(&edges, &roots_from(g), benchgraph::encode(g.seed.0, g.seed.1));
    assert_eq!(dred, oracle, "{label}: sqlite-DRed disagreed with the oracle");
    assert_eq!(dd, oracle, "{label}: dd disagreed with the oracle");
    assert_eq!(dred, dd, "{label}: sqlite-DRed and dd disagreed with each other");
    assert_eq!(signed, oracle, "{label}: signed-delta disagreed with the oracle");
    assert_eq!(signed, dred, "{label}: signed-delta and sqlite-DRed disagreed with each other");
    assert_eq!(signed_cte, oracle, "{label}: signed-delta-CTE disagreed with the oracle");
    assert_eq!(signed_cte, dred, "{label}: signed-delta-CTE and sqlite-DRed disagreed with each other");
    assert_eq!(delta_fold, oracle, "{label}: delta-fold disagreed with the oracle");
    assert_eq!(delta_fold, dred, "{label}: delta-fold and sqlite-DRed disagreed with each other");
}

// ---- DAG matrix: oracle == DRed == dd == counting -----------------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dag_all_engines_agree() {
    for &(layers, width) in &[(2usize, 200usize), (5, 500), (8, 1500)] {
        let g = benchgraph::gen_multi(layers, width);
        let label = format!("DAG l={layers} w={width}");
        assert_agreement(&g, &label).await;
        // counting must ALSO be correct on a DAG.
        let oracle: Vec<i64> = benchgraph::oracle_survivors(&g, g.seed).into_iter().collect();
        assert_eq!(count_survivors(&g).await, oracle, "{label}: counting must be correct on a DAG");
        assert_eq!(count_scc_survivors(&g).await, oracle, "{label}: SCC counting must be correct on a DAG");
    }
}

// ---- Cyclic matrix: oracle == DRed == dd across densities ----------------------
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cyclic_dred_and_dd_agree() {
    for &(layers, width) in &[(3usize, 300usize), (6, 800), (8, 1500)] {
        for &stride in &[2usize, 3, 5, 7, 11] {
            let g = benchgraph::gen_multi_cyclic(layers, width, stride);
            let label = format!("CYCLIC l={layers} w={width} stride={stride}");
            assert_agreement(&g, &label).await;
            let oracle: Vec<i64> = benchgraph::oracle_survivors(&g, g.seed).into_iter().collect();
            assert_eq!(count_scc_survivors(&g).await, oracle, "{label}: SCC counting must match the oracle");
        }
    }
}

// ---- The demonstration: SOME cyclic shape makes counting provably wrong --------
// Not "counting is always wrong on cycles" (it isn't) — "there EXISTS a root-
// anchored dead cycle where counting keeps phantom rows alive and DRed/dd don't".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn counting_is_wrong_on_a_phantom_cycle() {
    let g = benchgraph::gen_multi_cyclic(6, 10_000, 7);
    let oracle: Vec<i64> = benchgraph::oracle_survivors(&g, g.seed).into_iter().collect();
    // DRed stays correct.
    assert_eq!(dred_survivors(&g).await, oracle, "DRed must stay correct on the phantom-cycle graph");
    // counting keeps STRICTLY MORE rows alive (the phantom cycles).
    let count = count_survivors(&g).await;
    assert!(
        count.len() > oracle.len(),
        "counting must over-keep phantom rows: count={} oracle={}",
        count.len(),
        oracle.len()
    );
    // and every oracle survivor is in the counting set (counting only ADDS phantoms).
    let count_set: HashSet<i64> = count.iter().copied().collect();
    for k in &oracle {
        assert!(count_set.contains(k), "counting dropped a real survivor {k}");
    }
}

// ---- The cycle trap, isolated: signed-delta must NOT keep a cut cycle alive ----
// FAIL-FIRST receipt: naive ref-counting keeps a root-anchored cycle alive after
// the anchor is cut — the members mutually support each other (weight never hits
// 0, a phantom cycle). The per-round signed-delta path must NOT: aliveness re-
// derives as the least-fixpoint reachability from the surviving roots, so a back-
// edge into a member that lost its anchor cannot refresh it. Every case asserts
// byte-identical survivors against the oracle AND DRed, then proves naive counting
// would have kept the phantom.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signed_delta_kills_a_cut_cycle() {
    // R(0,0) -> A(0,1), and the cycle A -> B(0,2) -> C(0,3) -> A. Cut R, the only
    // root: the whole cycle has no surviving anchor and must die. A carries support
    // from R AND the back-edge C->A (weight 2), which is what makes NAIVE counting
    // keep the phantom — A loses only R's unit and stays at 1.
    let g = MultiGraph {
        rows: vec![(0, 0, 1), (0, 1, 2), (0, 2, 1), (0, 3, 1)],
        edges: vec![(0, 0, 0, 1), (0, 1, 0, 2), (0, 2, 0, 3), (0, 3, 0, 1)],
        seed: (0, 0),
        per_tag: [4, 0, 0],
    };
    let label = "CUT-CYCLE R->A->B->C->A";
    let oracle: Vec<i64> = benchgraph::oracle_survivors(&g, g.seed).into_iter().collect();
    let dred = dred_survivors(&g).await;
    let signed = signed_delta_survivors(&g).await;
    let count = count_survivors(&g).await;
    assert_eq!(dred, oracle, "{label}: DRed must kill the whole cut cycle");
    assert_eq!(signed, oracle, "{label}: signed-delta must kill the whole cut cycle");
    assert_eq!(signed, dred, "{label}: signed-delta and DRed must agree on a cut cycle");
    // Fail-first: naive counting over-keeps the phantom cycle.
    assert!(
        count.len() > oracle.len(),
        "{label}: naive counting must over-keep the phantom cycle: count={} oracle={}",
        count.len(),
        oracle.len()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signed_delta_v2_revives_through_alternate_anchor() {
    let g = MultiGraph {
        rows: vec![(0, 0, 1), (0, 1, 1), (0, 2, 1), (0, 3, 1), (0, 4, 1)],
        edges: vec![(0, 0, 0, 1), (0, 1, 0, 2), (0, 2, 0, 3), (0, 3, 0, 1), (0, 4, 0, 2)],
        seed: (0, 0),
        per_tag: [6, 0, 0],
    };
    let oracle: Vec<i64> = benchgraph::oracle_survivors(&g, g.seed).into_iter().collect();
    assert_eq!(signed_delta_cte_survivors(&g).await, oracle);
    assert_eq!(dred_survivors(&g).await, oracle);
    assert_eq!(oracle.len(), 4);
}
