// TEST: every fold path that walks a set once per member of another set.
// Each case builds its own fixture at N >= 1000 and reads a probe counter the
// fold arms, so a return to the scan shape is a count, never a timing guess.
// The elapsed assertions are the second gate: a quadratic that stays under the
// count bound cannot also stay under 2s at these sizes.

use sprefa_engine_rs::types::{
    IncrementalLevelStatement, IncrementalRelationPlan, RelDelta, RelationKind, RowColumnType,
    Value,
};

/// The probe counters are process-wide, so two cases reading them at once
/// would each see the other's arithmetic.
static PROBE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn plan(rel: &str) -> IncrementalRelationPlan {
    IncrementalRelationPlan {
        rel: rel.to_string(),
        kind: RelationKind::Set,
        table_name: rel.to_string(),
        delta_table_name: format!("__delta_{rel}"),
        frontier_table_name: format!("__frontier_{rel}"),
        next_frontier_table_name: format!("__next_frontier_{rel}"),
        departure_frontier_table_name: Some(format!("__departure_{rel}")),
        shared_frontier: None,
        columns: vec!["value".to_string()],
        column_types: vec![RowColumnType::Int],
        key_indices: vec![0],
        arrival_add_sql: None,
        arrival_del_sql: None,
        boundary_sql: String::new(),
    }
}

fn level(head: &str, relations: &[IncrementalRelationPlan]) -> IncrementalLevelStatement {
    IncrementalLevelStatement {
        head_rel: head.to_string(),
        head_table_name: head.to_string(),
        head_columns: vec!["value".to_string()],
        head_column_types: vec![RowColumnType::Int],
        insert_sql: Some(format!(
            "INSERT INTO \"{head}\" (\"value\") SELECT \"value\" FROM \"{}\" RETURNING \"value\"",
            relations[0].frontier_table_name
        )),
        intern_sql: None,
        head_delta_table_name: format!("__delta_{head}"),
        select_sql: String::new(),
        dred_sql: None,
        support_sql: None,
        support_intern_sql: None,
        support_count_sql: None,
        expand_sql: None,
        aggregate_sql: None,
        recursion_group: None,
        recompute_sql: String::new(),
        recompute_delete_sql: String::new(),
        recompute_insert_sqls: Vec::new(),
    }
}

// TEST: staging departures asks each rel's delta of the boundary list once.
// Pre-fix the boundary list was scanned per rel, so 1200 rels cost 1200 * 1200
// comparisons and this assertion read 1200 vs 720600.
#[test]
fn stage_departures_asks_each_rel_once() {
    let rels = 1_200usize;
    let relations: Vec<IncrementalRelationPlan> =
        (0..rels).map(|index| plan(&format!("rel{index}"))).collect();
    let deltas: Vec<RelDelta> = relations
        .iter()
        .map(|relation| RelDelta {
            rel: relation.rel.clone(),
            add: Vec::new(),
            del: vec![vec![Value::Integer(1)]],
        })
        .collect();
    let seam = sprefa_engine_rs::sql::SqliteSeam::in_memory().expect("seam");
    for relation in &relations {
        sprefa_engine_rs::sql::SqlRunner::execute_multiple(
            &seam,
            &format!(
                "CREATE TABLE \"{}\" (\"_phase\" INTEGER, \"_sequence\" INTEGER, \"value\" INTEGER)",
                relation
                    .departure_frontier_table_name
                    .as_ref()
                    .expect("departure table")
            ),
        )
        .expect("departure ddl");
    }
    let _serial = PROBE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let before = sprefa_engine_rs::incremental::plan_probes();
    let started = std::time::Instant::now();
    sprefa_engine_rs::incremental::stage_departures(&seam, &relations, &deltas, &sprefa_engine_rs::incremental::TickWork::unskipped(&relations))
        .expect("stage departures");
    let probes = sprefa_engine_rs::incremental::plan_probes() - before;
    assert!(
        probes <= 2 * rels as u64,
        "{probes} probes for {rels} rels is a scan"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}

// TEST: the level pass reads which rels a statement's frontier feeds once per
// program, not once per (statement, relation) pair on every tick. Pre-fix 40
// statements over 1200 rels cost 48000 substring searches per call and this
// assertion read 0 vs 48000.
#[test]
fn the_frontier_scan_runs_once_per_program() {
    let rels = 1_200usize;
    let statements_count = 40usize;
    let relations: Vec<IncrementalRelationPlan> =
        (0..rels).map(|index| plan(&format!("rel{index}"))).collect();
    let statements: Vec<IncrementalLevelStatement> = (0..statements_count)
        .map(|index| level(&format!("rel{index}"), &relations))
        .collect();
    let seam = sprefa_engine_rs::sql::SqliteSeam::in_memory().expect("seam");
    for relation in &relations {
        sprefa_engine_rs::sql::SqlRunner::execute_multiple(
            &seam,
            &format!(
                "CREATE TABLE \"{0}\" (\"value\" INTEGER); CREATE TABLE \"{1}\" (\"_phase\" INTEGER, \"_sequence\" INTEGER, \"value\" INTEGER); CREATE TABLE \"{2}\" (\"_phase\" INTEGER, \"_sequence\" INTEGER, \"value\" INTEGER); CREATE TABLE \"{3}\" (\"_sign\" INTEGER, \"_sequence\" INTEGER, \"value\" INTEGER)",
                relation.table_name,
                relation.frontier_table_name,
                relation.next_frontier_table_name,
                relation.delta_table_name
            ),
        )
        .expect("level ddl");
    }
    let _serial = PROBE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let before = sprefa_engine_rs::incremental::frontier_probes();
    let started = std::time::Instant::now();
    let heads = sprefa_engine_rs::incremental::recursive_heads(&statements, &relations);
    for _ in 0..3 {
        sprefa_engine_rs::incremental::apply_levels_before_edges(&seam, &statements, &relations, &heads, &sprefa_engine_rs::incremental::TickWork::unskipped(&relations), &sprefa_engine_rs::incremental::level_sources(&statements, &relations, &heads))
            .expect("levels");
    }
    let probes = sprefa_engine_rs::incremental::frontier_probes() - before;
    assert!(
        probes <= (statements_count * rels) as u64,
        "{probes} frontier probes over 3 ticks repeats a per-program scan"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}

// TEST: a level statement fetches its head plan by name, one probe each. Pre-fix
// each of the 40 statements walked all 1200 relations on every one of 3 ticks
// and this assertion read 240 vs 144000.
#[test]
fn a_level_statement_fetches_its_plan_by_name() {
    let rels = 1_200usize;
    let statements_count = 40usize;
    let relations: Vec<IncrementalRelationPlan> =
        (0..rels).map(|index| plan(&format!("rel{index}"))).collect();
    let statements: Vec<IncrementalLevelStatement> = (0..statements_count)
        .map(|index| level(&format!("rel{index}"), &relations))
        .collect();
    let seam = sprefa_engine_rs::sql::SqliteSeam::in_memory().expect("seam");
    for relation in &relations {
        sprefa_engine_rs::sql::SqlRunner::execute_multiple(
            &seam,
            &format!(
                "CREATE TABLE \"{0}\" (\"value\" INTEGER); CREATE TABLE \"{1}\" (\"_phase\" INTEGER, \"_sequence\" INTEGER, \"value\" INTEGER); CREATE TABLE \"{2}\" (\"_phase\" INTEGER, \"_sequence\" INTEGER, \"value\" INTEGER); CREATE TABLE \"{3}\" (\"_sign\" INTEGER, \"_sequence\" INTEGER, \"value\" INTEGER)",
                relation.table_name,
                relation.frontier_table_name,
                relation.next_frontier_table_name,
                relation.delta_table_name
            ),
        )
        .expect("level ddl");
    }
    let _serial = PROBE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let heads = sprefa_engine_rs::incremental::recursive_heads(&statements, &relations);
    let before = sprefa_engine_rs::incremental::plan_probes();
    let started = std::time::Instant::now();
    for _ in 0..3 {
        sprefa_engine_rs::incremental::apply_levels_before_edges(&seam, &statements, &relations, &heads, &sprefa_engine_rs::incremental::TickWork::unskipped(&relations), &sprefa_engine_rs::incremental::level_sources(&statements, &relations, &heads))
            .expect("levels");
    }
    let probes = sprefa_engine_rs::incremental::plan_probes() - before;
    assert!(
        probes <= (2 * 3 * statements_count) as u64,
        "{probes} plan probes for {} lookups is a scan",
        3 * statements_count
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}

// TEST: the boundary read keys each row once, not twice. Pre-fix the index was
// asked with one rendering and written with a second, so the Debug rendering ran
// 40000 times for 20000 rows and this assertion read 20000 vs 40000.
#[test]
fn the_boundary_read_renders_each_row_key_once() {
    let rows = 20_000usize;
    let relation = plan("measured");
    let result = sprefa_engine_rs::types::QueryResult {
        rows: (0..rows)
            .map(|index| {
                vec![
                    Value::Integer(index as i64),
                    Value::Integer(1),
                    Value::Integer(1),
                ]
            })
            .collect(),
        columns: vec![
            "value".to_string(),
            "__sign".to_string(),
            "__count".to_string(),
        ],
        rows_affected: 0,
    };
    let _serial = PROBE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let before = sprefa_engine_rs::incremental::dedup_probes();
    let started = std::time::Instant::now();
    let delta =
        sprefa_engine_rs::incremental::boundary_delta(&relation, &result).expect("boundary delta");
    let probes = sprefa_engine_rs::incremental::dedup_probes() - before;
    assert_eq!(delta.add.len(), rows);
    assert!(
        probes <= rows as u64,
        "{probes} row keys rendered for {rows} rows is a double render"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}

// TEST: the struct plane dedups collected values through an index. Pre-fix it
// scanned the bucket it had already filled, comparing the whole canonical
// rendering each time, so 3000 distinct values cost 4.5 million string
// comparisons and this assertion read 3000 vs 4501500.
#[test]
fn the_struct_plane_dedups_collected_values_through_an_index() {
    let values = 3_000usize;
    let types = vec![sprefa_engine_rs::types::StructTypePlan {
        name: "point".to_string(),
        columns: vec!["x".to_string(), "y".to_string()],
        refs: vec![None, None],
        key_indices: vec![0],
        conflict_sql: String::new(),
        intern_sql: String::new(),
        lookup_sql: String::new(),
    }];
    let ref_columns = std::collections::HashMap::from([(
        "placed".to_string(),
        vec![None, Some("point".to_string())],
    )]);
    let arrivals: Vec<sprefa_engine_rs::types::Arrival> = (0..values)
        .map(|index| sprefa_engine_rs::types::Arrival {
            rel: "placed".to_string(),
            sign: sprefa_engine_rs::types::ArrivalSign::Add,
            row: vec![
                Value::Integer(index as i64),
                Value::Text(format!("{{\"x\":{index},\"y\":{index}}}")),
            ],
        })
        .collect();
    let _serial = PROBE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let before = sprefa_engine_rs::struct_plane::collect_probes();
    let started = std::time::Instant::now();
    let distinct =
        sprefa_engine_rs::struct_plane::distinct_struct_values(&types, &ref_columns, &arrivals);
    let probes = sprefa_engine_rs::struct_plane::collect_probes() - before;
    assert_eq!(distinct, values);
    assert!(
        probes <= 2 * values as u64,
        "{probes} probes for {values} values is a scan"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}

// TEST: the rev-pair change plane drops paired paths through a set. Pre-fix each
// surviving path was checked against the whole paired list, so 2000 renames plus
// 2000 creations cost 4 million comparisons and this assertion read 4000 vs
// 8000000.
#[test]
fn the_change_plane_drops_paired_paths_through_a_set() {
    let pairs = 2_000usize;
    let mut created: Vec<String> = (0..pairs).map(|index| format!("new/{index}")).collect();
    let mut deleted: Vec<String> = (0..pairs).map(|index| format!("old/{index}")).collect();
    let _serial = PROBE_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let before = sprefa_engine_rs::change_facts::rename_probes();
    let started = std::time::Instant::now();
    let content = |index: usize| soopy::ContentId::blake3(format!("blob{index}").as_bytes());
    let base: std::collections::BTreeMap<String, soopy::ContentId> = (0..pairs)
        .map(|index| (format!("old/{index}"), content(index)))
        .collect();
    let head: std::collections::BTreeMap<String, soopy::ContentId> = (0..pairs)
        .map(|index| (format!("new/{index}"), content(index)))
        .collect();
    let renames =
        sprefa_engine_rs::change_facts::take_renames(&mut created, &mut deleted, &base, &head);
    let probes = sprefa_engine_rs::change_facts::rename_probes() - before;
    assert_eq!(renames.len(), pairs);
    assert_eq!((created.len(), deleted.len()), (0, 0));
    assert!(
        probes <= 4 * pairs as u64,
        "{probes} probes for {pairs} pairs is a scan"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}
