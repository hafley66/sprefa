//! The N+1 statement-count tripwire, in its OWN test process. `stmt_counter` is
//! a process-global atomic, so an exact-count assertion is only valid when no
//! other statement-issuing test runs concurrently. In the shared lib binary it
//! raced the cascade/reach/reconcile tests (saw 61 vs 5) and panicked while
//! holding the temporal `statement_lock`, poisoning it and cascading into the
//! other temporal tests. Isolated here, it is deterministic.

use sea_orm::{ConnectOptions, Database};
use sprefa_store::stmt_counter;
use sprefa_store::temporal::TemporalStore;

#[tokio::test]
async fn commit_statement_count_is_constant_for_a_delta_batch() {
    let path = std::env::temp_dir().join(format!(
        "temporal_stmtcount_{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    options.max_connections(1).min_connections(1);
    let store = TemporalStore::attach(Database::connect(options).await.unwrap())
        .await
        .unwrap();

    let deltas: Vec<(i64, i64)> = (0..1_000).map(|key| (key, 1)).collect();
    stmt_counter::reset();
    store.commit(&deltas).await.unwrap();
    assert_eq!(stmt_counter::get(), 5, "delta commit must be a fixed set-based batch, not N+1");

    let _ = std::fs::remove_file(&path);
}
