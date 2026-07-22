use sprefa_store::{benchgraph, measure::{run_cell, Cell}, memcap, relstore::RelStore};

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

#[tokio::main]
async fn main() {
    let mut sweep = Vec::new();
    for workload in ["DAG", "CYC"] {
        for cache_size_kib in [2_000, 32_000] {
            sweep.push(Cell {
                engine: if workload == "DAG" { "sqlite-count" } else { "sqlite-count-scc" },
                workload,
                nodes: 4_002,
                edges: 7_998,
                cache_size_kib,
                memcap_mb: 1_024,
            });
        }
    }

    for cell in sweep {
        let cyclic = cell.workload == "CYC";
        let cache = cell.cache_size_kib;
        let row = run_cell(
            cell,
            move |store: &RelStore| Box::pin(async move {
                let graph = if cyclic {
                    benchgraph::gen_multi_cyclic(4, 1_000, 7)
                } else {
                    benchgraph::gen_multi(4, 1_000)
                };
                let rows: Vec<_> = graph.rows.iter().map(|&(tag, id, weight)| (tag as i64, id, weight)).collect();
                let edges: Vec<_> = graph.edges.iter().map(|&(pt, pi, ct, ci)| (pt as i64, pi, ct as i64, ci)).collect();
                store.add_rows(&rows).await?;
                store.add_deps(&edges).await?;
                Ok(())
            }),
            move |store: &RelStore| Box::pin(async move {
                let seed = (0, 0);
                if cyclic {
                    store.retract_scc(&[seed]).await?;
                } else {
                    store.retract(&[seed]).await?;
                }
                store.alive_keys().await
            }),
        ).await;
        println!(
            "engine={} workload={} nodes={} cache_size_kib={} build_ms={:.1} insert_ms={:.1} op_ms={:.1} rss_kb={} out_hash={} correct={}",
            row.cell.engine,
            row.cell.workload,
            row.cell.nodes,
            cache,
            row.samples[0].t_ms,
            row.samples[1].t_ms,
            row.samples[2].t_ms,
            row.samples[2].rss_kb,
            row.out_hash,
            row.correct,
        );
    }
    // TODO(reach): add reaches_from/count_pairs cells
}
