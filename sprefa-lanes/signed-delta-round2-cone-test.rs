//! The signed-delta cone-scope statement budget, in its OWN test process.
//! `stmt_counter` is a process-global atomic, so an exact statement count is only
//! valid when no other statement-issuing test runs concurrently (see `stmt_count.rs`
//! for the same isolation). Here the cone retraction is measured alone.
//!
//! The assertion that pins round-1's fix: retracting a small cone must issue a small
//! CONSTANT number of statements regardless of corpus size — O(cone), not O(corpus).
//! Round 1 re-walked the surviving reach and republished the weight column for every
//! row on every retraction (O(corpus)); the cone-scoped variant touches only the cone.

use sea_orm::{ConnectOptions, Database};
use sprefa_store::{relstore::RelStore, stmt_counter};

async fn open_store() -> (RelStore, std::path::PathBuf) {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let uniq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("sdc_cone_{}_{uniq}.sqlite", std::process::id()));
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

/// Retract the isolated rootA->leafA cone from a graph whose surviving chain is
/// `chain_len` rows; return the statement budget of the retraction.
async fn cone_retract_statements(chain_len: i64) -> u64 {
    let (store, path) = open_store().await;
    let c1 = 2_000_000;
    let c2 = 2_000_001;
    let mut rows: Vec<(i64, i64, i64)> = (1..=chain_len).map(|i| (0, i, 1)).collect();
    rows.push((0, c1, 1));
    rows.push((0, c2, 1));
    let mut deps: Vec<(i64, i64, i64, i64)> = (1..chain_len).map(|i| (0, i, 0, i + 1)).collect();
    deps.push((0, c1, 0, c2));
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    sprefa_store::cascade::build_signed_delta_state(store.conn(), store.ns())
        .await
        .unwrap();
    stmt_counter::reset();
    sprefa_store::cascade::retract_signed_delta(store.conn(), store.ns(), &[(0, c1)])
        .await
        .unwrap();
    let stmts = stmt_counter::get();
    drop(store);
    cleanup(&path);
    stmts
}

#[tokio::test]
async fn signed_delta_cone_retraction_is_constant_statement_budget() {
    let small = cone_retract_statements(2_000).await;
    let big = cone_retract_statements(200_000).await;
    assert_eq!(small, big, "statement count must be O(cone): size-independent");
    assert!(small < 40, "cone retraction must run in a constant budget: {small}");
}
