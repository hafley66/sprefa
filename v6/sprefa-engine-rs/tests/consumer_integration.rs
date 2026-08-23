//! Consumer integration proof: uses sprefa_engine_rs strictly as an external
//! library (`use sprefa_engine_rs::...`) with runtime boot, tick driver, and
//! host executor registry public surfaces.

use std::collections::HashMap;

use sprefa_engine_rs::driver::{drive_tick, run_schedule};
use sprefa_engine_rs::hosts::{executor_for, HostLiveRunner, LINKED_EXECUTORS};
use sprefa_engine_rs::program::run_boot;
use sprefa_engine_rs::sql::SqliteSeam;
use sprefa_engine_rs::types::{
    Arrival, ArrivalSign, BootStatement, IncrementalRelationPlan, InternMode, RelationKind,
    RowColumnType, Value,
};
use sprefa_engine_rs::GenProgram;

#[tokio::test]
async fn external_consumer_drives_runtime_and_hosts() {
    // 1. Host executor registry surface. Every name in the roster links a Rust
    // executor; `shell` is not one of them and never resolves.
    for linked in LINKED_EXECUTORS.split(", ") {
        assert!(
            executor_for(linked).is_some(),
            "the roster names {linked}, which no executor answers"
        );
    }
    assert!(executor_for("shell").is_none(), "no host reaches a shell");

    // HostLiveRunner construction surface
    let empty_rel_cols = HashMap::new();
    let runner = HostLiveRunner::new(&[], &empty_rel_cols).expect("runner constructs");
    assert!(!runner.has_plans());

    // 2. Runtime boot + tick driver surface
    let seam = SqliteSeam::in_memory().expect("in-memory db");
    seam.run_ddl(&[
        "CREATE TABLE \"node\" (\"id\" INTEGER NOT NULL, PRIMARY KEY (\"id\")) WITHOUT ROWID"
            .to_string(),
        "CREATE TEMP TABLE \"__delta_node\" (\"_sign\" INTEGER NOT NULL, \"_sequence\" INTEGER NOT NULL, \"id\" INTEGER NOT NULL)".to_string(),
        "CREATE TEMP TABLE \"__frontier_node\" (\"_phase\" INTEGER NOT NULL, \"_sequence\" INTEGER NOT NULL, \"id\" INTEGER NOT NULL)".to_string(),
        "CREATE TEMP TABLE \"__next_frontier_node\" (\"_phase\" INTEGER NOT NULL, \"_sequence\" INTEGER NOT NULL, \"id\" INTEGER NOT NULL)".to_string(),
    ])
    .expect("ddl");

    let boot = vec![BootStatement {
        rel: "node".to_string(),
        sql: "INSERT INTO \"node\" (\"id\") VALUES (100)".to_string(),
        params: vec![],
    }];
    run_boot(&seam, &boot);

    let mut rel_column_types = HashMap::new();
    rel_column_types.insert("node".to_string(), vec![RowColumnType::Int]);
    let mut rel_columns = HashMap::new();
    rel_columns.insert("node".to_string(), vec!["id".to_string()]);

    let program = GenProgram {
        name: "test_lib_program".to_string(),
        intern_mode: InternMode::Dict,
        ddl: vec![],
        rel_columns,
        rel_column_types,
        arrival_targets: vec!["node".to_string()],
        boot: vec![],
        final_select: HashMap::new(),
        arrival_templates: HashMap::new(),
        text_intern_plan: None,
        struct_types: vec![],
        struct_ref_columns: HashMap::new(),
        pre_snapshot_rels: Vec::new(),
        level_sources: Default::default(),
        relations: vec![IncrementalRelationPlan {
            rel: "node".to_string(),
            kind: RelationKind::Set,
            table_name: "node".to_string(),
            delta_table_name: "__delta_node".to_string(),
            frontier_table_name: "__frontier_node".to_string(),
            next_frontier_table_name: "__next_frontier_node".to_string(),
            departure_frontier_table_name: None,
            shared_frontier: None,
            columns: vec!["id".to_string()],
            column_types: vec![RowColumnType::Int],
            key_indices: vec![],
            arrival_add_sql: Some("INSERT OR IGNORE INTO \"node\" (\"id\") SELECT json_extract(value, '$[0]') FROM json_each(?) RETURNING \"id\"".to_string()),
            arrival_del_sql: Some("DELETE FROM \"node\" WHERE (\"id\") IN (SELECT json_extract(value, '$[0]') FROM json_each(?)) RETURNING \"id\"".to_string()),
            boundary_sql: "SELECT \"id\", \"_sign\" AS \"__sign\", count(*) AS \"__count\" FROM \"__delta_node\" WHERE \"_sign\" IN (-1, 1) GROUP BY \"id\", \"_sign\"".to_string(),
        }],
        edges: vec![],
        levels: vec![],
        retentions: vec![],
        uses_tick: false,
        reconcile_every_tick: false,
        incremental_safe: true,
        enum_types: vec![],
        enum_ref_columns: Default::default(),
        ir_version: sprefa_engine_rs::program::IR_VERSION,
        host_plans: vec![],
        queries: vec![],
        recursive_level_heads: vec![],
    };

    let arrivals = vec![Arrival {
        rel: "node".to_string(),
        sign: ArrivalSign::Add,
        row: vec![Value::Integer(200)],
    }];

    let deltas = drive_tick(&program, &seam, arrivals).await.expect("tick");
    assert_eq!(deltas.rels.len(), 1);
    assert_eq!(deltas.rels[0].rel, "node");
    assert_eq!(deltas.rels[0].add, vec![vec![Value::Integer(200)]]);

    let schedule = vec![vec![Arrival {
        rel: "node".to_string(),
        sign: ArrivalSign::Add,
        row: vec![Value::Integer(300)],
    }]];
    let fold = run_schedule(&program, &seam, &schedule, 10)
        .await
        .expect("schedule fold");
    assert_eq!(fold.lines.len(), 1);
}
