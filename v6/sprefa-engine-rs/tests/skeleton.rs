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
            shared_frontier: None,
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
            shared_frontier: None,
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
        text_intern_plan: None,
        struct_types: Vec::new(),
        struct_ref_columns: HashMap::new(),
        pre_snapshot_rels: Vec::new(),
        relations,
        edges: vec![],
        levels: vec![IncrementalLevelStatement {
            head_rel: "seen".to_string(),
            head_table_name: "seen".to_string(),
            head_delta_table_name: "__delta_seen".to_string(),
            head_columns: vec!["value".to_string()],
            head_column_types: vec![RowColumnType::Int],
            insert_sql: Some("INSERT OR IGNORE INTO \"seen\" (\"value\") SELECT DISTINCT d0.\"value\" FROM \"__frontier_source\" d0 WHERE d0.\"_phase\" >= 0 RETURNING \"value\"".to_string()),
            intern_sql: None,
            select_sql: "SELECT \"value\" FROM \"seen\"".to_string(),
            recompute_delete_sql: "DELETE FROM \"seen\"".to_string(),
            recompute_insert_sqls: vec!["INSERT OR IGNORE INTO \"seen\" (\"value\") SELECT b0.\"value\" FROM \"source\" b0".to_string()],
            recompute_sql: "DELETE FROM \"seen\";\nINSERT OR IGNORE INTO \"seen\" (\"value\") SELECT b0.\"value\" FROM \"source\" b0".to_string(),
            support_sql: Some(vec![
                "DELETE FROM \"__support_next_seen\"".to_string(),
                "INSERT INTO \"__support_next_seen\" (\"value\", \"__refcount\") SELECT \"value\", sum(\"__refcount\") FROM (SELECT b0.\"value\" AS \"value\", count(*) AS \"__refcount\" FROM \"source\" b0 GROUP BY b0.\"value\") GROUP BY \"value\"".to_string(),
                "UPDATE \"seen\" AS h SET \"__refcount\" = COALESCE((SELECT n.\"__refcount\" FROM \"__support_next_seen\" n WHERE n.\"value\" = h.\"value\"), 0)".to_string(),
                "INSERT INTO \"__delta_seen\" (\"_sign\", \"_sequence\", \"value\") SELECT -1, row_number() OVER () - 1, \"value\" FROM \"seen\" WHERE \"__refcount\" <= 0".to_string(),
                "DELETE FROM \"seen\" WHERE \"__refcount\" <= 0".to_string(),
                "DELETE FROM \"__new_seen\"".to_string(),
                "INSERT INTO \"__new_seen\" (\"value\", \"__refcount\") SELECT n.\"value\", n.\"__refcount\" FROM \"__support_next_seen\" n LEFT JOIN \"seen\" h ON n.\"value\" = h.\"value\" WHERE h.\"value\" IS NULL".to_string(),
                "INSERT INTO \"__delta_seen\" (\"_sign\", \"_sequence\", \"value\") SELECT 1, \"rowid\" - 1, \"value\" FROM \"__new_seen\"".to_string(),
                "INSERT INTO \"__frontier_seen\" (\"_phase\", \"_sequence\", \"value\") SELECT ?, \"rowid\" - 1, \"value\" FROM \"__new_seen\"".to_string(),
                "INSERT INTO \"__next_frontier_seen\" (\"_phase\", \"_sequence\", \"value\") SELECT ?, \"rowid\" - 1, \"value\" FROM \"__new_seen\"".to_string(),
                "INSERT OR IGNORE INTO \"seen\" (\"value\", \"__refcount\") SELECT n.\"value\", n.\"__refcount\" FROM \"__support_next_seen\" n".to_string(),
            ]),
            support_intern_sql: None,
            support_count_sql: None,
            expand_sql: None,
            dred_sql: None,
            aggregate_sql: None,
            recursion_group: None,
        }],
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
    }
}

#[tokio::test]
async fn skeleton_one_level_fixture_byte_identical() {
    let program = fixture_program();
    let seam = SqliteSeam::in_memory().expect("open seam");
    let schedule = vec![vec![Arrival {
        rel: "source".to_string(),
        sign: ArrivalSign::Add,
        row: int_row(&[1]),
    }]];
    let fold = run_schedule(&program, &seam, &schedule, 100)
        .await
        .expect("schedule fold");
    assert_eq!(fold.lines.len(), 1, "one schedule tick, no drain");
    let expected =
        "{\"tick\":1,\"deltas\":{\"seen\":{\"add\":[[1]],\"del\":[]},\"source\":{\"add\":[[1]],\"del\":[]}}}";
    assert_eq!(fold.lines[0], expected, "byte-identical to oracle");
}
