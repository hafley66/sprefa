//! Recursive CTE and folded-delta retraction probe.
//!
//! Run: cargo run --release --example recursive_probe -- [layers width stride]

use std::time::Instant;

use sea_orm::{ConnectOptions, Database};
use sprefa_store::{benchgraph, benchgraph::MultiGraph, relstore::RelStore, stmt_counter};

async fn open_store() -> (RelStore, std::path::PathBuf) {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "recursive_probe_{}_{}.sqlite",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    options.max_connections(1).min_connections(1);
    (
        RelStore::attach(Database::connect(options).await.unwrap())
            .await
            .unwrap(),
        path,
    )
}

fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

async fn measure<F>(g: &MultiGraph, name: &str, op: F)
where
    F: for<'a> FnOnce(
        &'a RelStore,
        (i64, i64),
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = u64> + 'a>>,
{
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> = g
        .edges
        .iter()
        .map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci))
        .collect();
    let (store, path) = open_store().await;
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    let oracle: Vec<i64> = benchgraph::oracle_survivors(g, g.seed)
        .into_iter()
        .collect();
    stmt_counter::reset();
    let started = Instant::now();
    op(&store, (g.seed.0 as i64, g.seed.1)).await;
    let elapsed = started.elapsed().as_secs_f64() * 1e3;
    let survivors = store.alive_keys().await.unwrap();
    println!(
        "| {name} | {elapsed:.1} | {} | {} | {} |",
        stmt_counter::get(),
        survivors.len(),
        if survivors == oracle { "yes" } else { "NO" }
    );
    drop(store);
    cleanup(&path);
}

async fn run(layers: usize, width: usize, stride: usize) {
    let graph = benchgraph::gen_multi_cyclic(layers, width, stride);
    println!("| variant | ms | stmts | survivors | oracle-equal |");
    println!("|---|---:|---:|---:|:---:|");
    measure(&graph, "dred-loop", |store, seed| {
        Box::pin(async move { store.retract_dred(&[seed]).await.unwrap() })
    })
    .await;
    measure(&graph, "dred-cte", |store, seed| {
        Box::pin(async move { store.retract_dred_cte(&[seed]).await.unwrap() })
    })
    .await;
    measure(&graph, "signed-delta", |store, seed| {
        Box::pin(async move {
            sprefa_store::cascade::retract_signed_delta(store.conn(), store.ns(), &[seed])
                .await
                .unwrap()
        })
    })
    .await;
    measure(&graph, "signed-delta-cte", |store, seed| {
        Box::pin(async move { store.retract_signed_delta_cte(&[seed]).await.unwrap() })
    })
    .await;
    measure(&graph, "delta-fold", |store, seed| {
        Box::pin(async move { store.retract_delta_fold(&[seed]).await.unwrap() })
    })
    .await;
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 4 {
        run(
            args[1].parse().unwrap(),
            args[2].parse().unwrap(),
            args[3].parse().unwrap(),
        )
        .await;
    } else {
        run(6, 160_000, 0).await;
        run(6, 160_000, 7).await;
    }
}
