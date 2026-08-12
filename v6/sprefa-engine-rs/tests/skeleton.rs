// Skeleton proof for step 2: DDL execution, boot, and the tick loop draining
// arrivals against the real SQLite seam. Levels arrive in step 3; this test
// asserts the source-boundary line the drained arrivals produce.

use std::collections::HashMap;

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::sql::SqliteSeam;
use sprefa_engine_rs::types::*;
use sprefa_engine_rs::GenProgram;

fn int_row(values: &[i64]) -> Row {
    values.iter().map(|v| Value::Integer(*v)).collect()
}

fn fixture_program() -> GenProgram {
    let mut rel_column_types = HashMap::new();
    rel_column_types.insert("seen".to_string(), vec![RowColumnType::Int]);
    rel_column_types.insert("source".to_string(), vec![RowColumnType::Int]);
    let mut rel_columns = HashMap::new();
    rel_columns.insert("seen".to_string(), vec!["value".to_string()]);
    rel_columns.insert("source".to_string(), vec!["value".to_string()]);

    let relations = vec![
        IncrementalRelationPlan {
            rel: "seen".to_string(),
            kind: RelationKind::Set,
            table_name: "seen".to_string(),
            delta_table_name: "__delta_seen".to_string(),
            frontier_table_name: "__frontier_seen".to_string(),
            next_frontier_table_name: "__next_frontier_seen".to_string(),
            departure_frontier_table_name: None,
            columns: vec!["value".to_string()],
            column_types: vec![RowColumnType::Int],
            key_indices: vec![],
            arrival_add_sql: None,
            arrival_del_sql: None,
            boundary_sql: "SELECT \"value\", \"_sign\" AS \"__sign\", count(*) AS \"__count\" FROM \"__delta_seen\" WHERE \"_sign\" IN (-1, 1) GROUP BY \"value\", \"_sign\"".to_string(),
        },
        IncrementalRelationPlan {
            rel: "source".to_string(),
            kind: RelationKind::Set,
            table_name: "source".to_string(),
            delta_table_name: "__delta_source".to_string(),
            frontier_table_name: "__frontier_source".to_string(),
            next_frontier_table_name: "__next_frontier_source".to_string(),
            departure_frontier_table_name: None,
            columns: vec!["value".to_string()],
            column_types: vec![RowColumnType::Int],
            key_indices: vec![],
            arrival_add_sql: Some("INSERT OR IGNORE INTO \"source\" (\"value\") SELECT json_extract(value, '$[0]') FROM json_each(?) RETURNING \"value\"".to_string()),
            arrival_del_sql: Some("DELETE FROM \"source\" WHERE (\"value\") IN (SELECT json_extract(value, '$[0]') FROM json_each(?)) RETURNING \"value\"".to_string()),
            boundary_sql: "SELECT \"value\", \"_sign\" AS \"__sign\", count(*) AS \"__count\" FROM \"__delta_source\" WHERE \"_sign\" IN (-1, 1) GROUP BY \"value\", \"_sign\"".to_string(),
        },
    ];

    GenProgram {
        name: "next_level_is_the_bare_atom_spelling".to_string(),
        intern_mode: InternMode::Dict,
        ddl: vec![
            "CREATE TABLE \"seen\" (\"value\" INTEGER NOT NULL, \"__refcount\" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY (\"value\")) WITHOUT ROWID".to_string(),
            "CREATE TABLE \"source\" (\"value\" INTEGER NOT NULL, PRIMARY KEY (\"value\")) WITHOUT ROWID".to_string(),
            "CREATE TEMP TABLE \"__delta_seen\" (\"_sign\" INTEGER NOT NULL, \"_sequence\" INTEGER NOT NULL, \"value\" INTEGER NOT NULL)".to_string(),
            "CREATE TEMP TABLE \"__frontier_seen\" (\"_phase\" INTEGER NOT NULL, \"_sequence\" INTEGER NOT NULL, \"value\" INTEGER NOT NULL)".to_string(),
            "CREATE TEMP TABLE \"__next_frontier_seen\" (\"_phase\" INTEGER NOT NULL, \"_sequence\" INTEGER NOT NULL, \"value\" INTEGER NOT NULL)".to_string(),
            "CREATE TEMP TABLE \"__delta_source\" (\"_sign\" INTEGER NOT NULL, \"_sequence\" INTEGER NOT NULL, \"value\" INTEGER NOT NULL)".to_string(),
            "CREATE TEMP TABLE \"__frontier_source\" (\"_phase\" INTEGER NOT NULL, \"_sequence\" INTEGER NOT NULL, \"value\" INTEGER NOT NULL)".to_string(),
            "CREATE TEMP TABLE \"__next_frontier_source\" (\"_phase\" INTEGER NOT NULL, \"_sequence\" INTEGER NOT NULL, \"value\" INTEGER NOT NULL)".to_string(),
            "CREATE TEMP TABLE \"__support_next_seen\" (\"value\" INTEGER NOT NULL, \"__refcount\" INTEGER NOT NULL, PRIMARY KEY (\"value\")) WITHOUT ROWID".to_string(),
            "CREATE TEMP TABLE \"__new_seen\" (\"value\" INTEGER NOT NULL, \"__refcount\" INTEGER NOT NULL)".to_string(),
            "CREATE INDEX \"seen_zero\" ON \"seen\" (\"__refcount\") WHERE \"__refcount\" <= 0".to_string(),
        ],
        rel_columns,
        rel_column_types,
        arrival_targets: vec!["source".to_string()],
        boot: vec![
            BootStatement { rel: "seen".to_string(), sql: "DELETE FROM \"seen\"".to_string(), params: vec![] },
            BootStatement { rel: "seen".to_string(), sql: "INSERT OR IGNORE INTO \"seen\" (\"value\") SELECT b0.\"value\" FROM \"source\" b0".to_string(), params: vec![] },
        ],
        final_select: HashMap::new(),
        arrival_templates: HashMap::new(),
        relations,
        edges: vec![],
        levels: vec![],
        retentions: vec![],
        reconcile_every_tick: false,
        incremental_safe: true,
    }
}

#[tokio::test]
async fn skeleton_drains_one_arrival_and_diffs_source_boundary() {
    let program = fixture_program();
    let seam = SqliteSeam::in_memory().expect("open seam");
    let schedule = vec![vec![Arrival {
        rel: "source".to_string(),
        sign: ArrivalSign::Add,
        row: int_row(&[1]),
    }]];
    let fold = run_schedule(&program, &seam, &schedule, 100).await;
    assert_eq!(fold.lines.len(), 1, "one schedule tick, no drain");
    let line = &fold.lines[0];
    assert!(
        line.contains("\"source\":{\"add\":[[1]]"),
        "source boundary add present in {}",
        line
    );
    // Levels are not wired in the skeleton; the seen rel is not yet derived.
    assert!(
        !line.contains("\"seen\""),
        "seen not derived in skeleton: {}",
        line
    );
}
