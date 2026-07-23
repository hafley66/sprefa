//! The reconcile parity check: `engine::reconcile` (driven through `propagate`, the
//! topological sweep) vs the resident salsa-crate oracle vs a from-scratch oracle, over a
//! matrix of layered DAGs + edit streams. DONE bar = byte-identical ANSWER digest every
//! edit tick + equal RECOMPUTE COUNT (early cutoff behaves identically).
//!
//! `propagate` (engine.rs reconcile) is the labkit `SqlReconciler` shape: it walks seeds
//! + transitive readers in ASCENDING id order (a valid topo order, since deps < node), so
//! a node is recomputed only after its deps are current — the property the earlier
//! lazy one-hop `dirty()` loop violated on DAG diamonds. This test is the standing proof
//! the SQLite plane matches salsa, including the early-cutoff count.

use std::sync::atomic::{AtomicU64, Ordering};

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};
use sprefa_store::oracle::salsa::{self, Reconciler, SalsaReconciler};
use sprefa_store::reconcile;
use sprefa_store::relstore::{stamp, GraphNs};

async fn open() -> (DatabaseConnection, GraphNs) {
    static N: AtomicU64 = AtomicU64::new(0);
    let uniq = N.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir()
        .join(format!("reconcile_parity_{}_{uniq}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    opt.max_connections(1).min_connections(1);
    let db = Database::connect(opt).await.unwrap();
    let ns = GraphNs::default();
    stamp(&db, &ns).await.unwrap();
    (db, ns)
}

/// Seed the SQLite reconcile plane (rx_memo/rx_dep under the default ns) with the cold
/// ascending digests. Returns the RAM value mirror (propagate recomputes from rx_memo
/// digests, so no RAM memo mirror is needed).
async fn engine_build(
    db: &DatabaseConnection,
    ns: &GraphNs,
    deps: &[Vec<u32>],
    init: &[i64],
) -> (Vec<i64>, u64) {
    let n = init.len();
    let value = init.to_vec();
    let mut memo = vec![0i64; n];
    for i in 0..n {
        memo[i] = salsa::node_digest(value[i], deps[i].iter().map(|&j| memo[j as usize]));
        let dep_ids: Vec<i64> = deps[i].iter().map(|&j| j as i64).collect();
        reconcile::seed(db, ns, i as i64, memo[i], &dep_ids, 0).await.unwrap();
    }
    (value, 0)
}

/// XOR of every durable rx_memo digest — reading the table proves the on-disk memo is the
/// truth, not a RAM shadow.
async fn engine_answer(db: &DatabaseConnection, ns: &GraphNs) -> i64 {
    let rows = db
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            format!("SELECT digest FROM {}", ns.memo),
        ))
        .await
        .unwrap();
    rows.iter().fold(0i64, |a, r| a ^ r.try_get_by_index::<i64>(0).unwrap_or(0))
}

/// Over a matrix of (nodes, ticks, edits-per-tick, rng-seed): the SQLite reconcile plane
/// (driven via `propagate`) and the salsa-crate oracle agree on the answer digest EVERY
/// tick, both match the from-scratch oracle at the end, and they recompute the SAME number
/// of nodes (early-cutoff parity). If any breaks, this names the exact shape that broke it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconcile_parity_salsa_vs_sql() {
    for &(n, ticks, per, seed) in &[
        (32usize, 10usize, 3usize, 1u64),
        (128, 20, 6, 2),
        (256, 15, 8, 7),
        (16, 40, 1, 99),
    ] {
        let deps = salsa::reconcile_graph(n);
        let stream = salsa::reconcile_stream(n, &deps, seed, ticks, per);

        let mut sal = SalsaReconciler::default();
        sal.build(deps.clone(), stream.init.clone());

        let (db, ns) = open().await;
        let (mut value, mut recomputes) = engine_build(&db, &ns, &deps, &stream.init).await;

        // post-build answer agrees.
        let mut sal_ans = sal.answer();
        let mut eng_ans = engine_answer(&db, &ns).await;
        assert_eq!(sal_ans, eng_ans, "post-build answer mismatch (n={n})");

        for (ti, tick) in stream.edits.iter().enumerate() {
            sal.edit(tick);
            let rev = (ti + 1) as i64;
            for &(i, v) in tick {
                value[i as usize] = v;
            }
            let seeds: Vec<i64> = tick.iter().map(|&(i, _)| i as i64).collect();
            recomputes += reconcile::propagate(&db, &ns, &seeds, rev, |id, dep_digests| {
                salsa::node_digest(value[id as usize], dep_digests.iter().copied())
            })
            .await
            .unwrap();
            sal_ans = sal.answer();
            eng_ans = engine_answer(&db, &ns).await;
            assert_eq!(
                sal_ans, eng_ans,
                "answer mismatch (n={n}, tick={ti}): salsa {sal_ans} != engine {eng_ans}"
            );
        }

        // 3-way: the durable SQLite memo equals the independent from-scratch oracle.
        assert_eq!(eng_ans, stream.oracle_answer, "engine != from-scratch oracle (n={n})");

        // the early-cutoff proof: salsa and SQL recompute the SAME number of nodes.
        assert_eq!(
            sal.recomputes(),
            recomputes,
            "recompute-count mismatch (n={n}): salsa {} != engine {}",
            sal.recomputes(),
            recomputes
        );
    }
}
