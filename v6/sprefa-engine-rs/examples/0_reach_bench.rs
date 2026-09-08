//! Adapter for sprefa-store/bench/run.sh's shared root-reachability workload.
//! Executes a compiler-emitted program through the production tick driver.
use serde::Deserialize;
use sprefa_engine_rs::{
    driver::drive_tick_transacted,
    program::run_boot,
    run,
    sql::{SqlRunner, SqliteSeam, SEAM_TALLY},
    types::{Arrival, ArrivalSign, SqlStatement, Value},
};
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

#[derive(Deserialize)]
struct Fixture {
    nodes: usize,
    edges: Vec<Vec<i64>>,
    input_hash: String,
    before: Vec<i64>,
    after: Vec<i64>,
}

fn rows(seam: &SqliteSeam, sql: &str) -> Vec<Vec<i64>> {
    seam.execute(&SqlStatement {
        sql: sql.to_string(),
        args: vec![],
    })
    .unwrap()
    .rows
    .into_iter()
    .map(|row| {
        row.into_iter()
            .map(|value| match value {
                Value::Integer(number) => number,
                other => panic!("expected integer, got {other:?}"),
            })
            .collect()
    })
    .collect()
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let program = run::load_program(std::path::Path::new(&args[1]))
        .unwrap()
        .program;
    let graph: Fixture = serde_json::from_str(&std::fs::read_to_string(&args[2]).unwrap()).unwrap();
    let mut arrivals: Vec<Arrival> = graph
        .edges
        .iter()
        .map(|edge| Arrival {
            rel: "edge".into(),
            sign: ArrivalSign::Add,
            row: edge.iter().map(|&value| Value::Integer(value)).collect(),
        })
        .collect();
    for root in [0, 1] {
        arrivals.push(Arrival {
            rel: "root".into(),
            sign: ArrivalSign::Add,
            row: vec![Value::Integer(root)],
        });
    }
    let seam = run::open_seam(None).unwrap();
    seam.size_statement_cache(program.stable_sql_count() + 64);
    let count_sql = format!("SELECT count(*) FROM ({})", program.final_select["alive"]);
    let alive_sql = format!("{} ORDER BY node", program.final_select["alive"]);
    let edge_sql = format!("{} ORDER BY parent, child", program.final_select["edge"]);
    let setup_started = Instant::now();
    seam.run_program_ddl(&program.ddl, &program.queries)
        .unwrap();
    run_boot(&seam, &program.boot);
    let initial = drive_tick_transacted(&program, &seam, arrivals)
        .await
        .unwrap();
    let before_count = seam.scalar(&count_sql).unwrap();
    let setup_ms = setup_started.elapsed().as_secs_f64() * 1e3;
    assert!(!initial.carry_pending);
    assert_eq!(before_count as usize, graph.before.len());
    assert_eq!(
        rows(&seam, &alive_sql),
        graph.before.iter().map(|&n| vec![n]).collect::<Vec<_>>()
    );
    assert_eq!(rows(&seam, &edge_sql), graph.edges);
    eprintln!("INPUT|sprefa-engine-rs|{}", graph.input_hash);
    let statement_start = SEAM_TALLY.statements.load(Relaxed);
    let started = Instant::now();
    let retracted = drive_tick_transacted(
        &program,
        &seam,
        vec![Arrival {
            rel: "root".into(),
            sign: ArrivalSign::Del,
            row: vec![Value::Integer(0)],
        }],
    )
    .await
    .unwrap();
    let after_count = seam.scalar(&count_sql).unwrap();
    let retract_ms = started.elapsed().as_secs_f64() * 1e3;
    let statements = SEAM_TALLY.statements.load(Relaxed) - statement_start;
    assert!(!retracted.carry_pending);
    assert_eq!(after_count as usize, graph.after.len());
    assert_eq!(
        rows(&seam, &alive_sql),
        graph.after.iter().map(|&n| vec![n]).collect::<Vec<_>>()
    );
    eprintln!("STATUS|sprefa-engine-rs|ok|compile_dl6 emit_rust program through drive_tick_transacted and GenProgram::run_tick; exact input edges and before/after sets match shared BFS outside clocks; count inside clocks|child-process peak RSS from time; in-memory rusqlite store|DL_MEMCAP_MB requested but unenforced for this adapter");
    eprintln!("CSV,sprefa-engine-rs,{},{},{},{setup_ms:.3},{retract_ms:.3},{statements},RSS_FROM_TIME,N/A,N/A,0", graph.nodes, graph.edges.len(), before_count-after_count);
}
