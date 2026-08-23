// TEST: the fold's summary names the (verb, relation) pairs sf_guard's IR
// declares, and the seam compiles each distinct SQL text once per connection.
// Fail-first: before the statement cache landed, `execute` called
// `Connection::prepare` on every dispatch, and nothing counted the compiles;
// the dispatch/text ratio below is what a return to that shape breaks.

use std::process::Command;

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::sql::SqliteSeam;
use sprefa_engine_rs::trace;
use sprefa_engine_rs::types::{Arrival, ArrivalSign, ProgramJson, Value};
use sprefa_engine_rs::GenProgram;

fn compile_sf_guard() -> GenProgram {
    let engine = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = engine.join("../tsv2/tests/shared_frontier/sf_guard.dl6");
    let temp = tempfile::tempdir().expect("temporary compiler output directory");
    let generated = temp.path().join("sf_guard.program.rs");
    let goal = format!(
        "compile_dl6('{}','{}',[emitter(emit_rust:emit_program)])",
        source.display(),
        generated.display()
    );
    let output = Command::new("swipl")
        .args(["-q", "-l"])
        .arg(engine.join("../prolog/compile.pl"))
        .args(["-l"])
        .arg(engine.join("../prolog/emit_rust.pl"))
        .args(["-g", &goal, "-g", "halt"])
        .output()
        .expect("run compile_dl6 for Rust emitter");
    assert!(
        output.status.success(),
        "compile_dl6 failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let module_text = std::fs::read_to_string(&generated).expect("read emitted ProgramJson");
    let start = module_text.find("r#\"").expect("raw ProgramJson open") + 3;
    let end = module_text[start..]
        .find("\"#;")
        .expect("raw ProgramJson close")
        + start;
    let program_json: ProgramJson =
        serde_json::from_str(&module_text[start..end]).expect("parse emitted ProgramJson");
    GenProgram::from_json(program_json)
}

fn three_ticks() -> Vec<Vec<Arrival>> {
    let add = |name: &str, age: i64| Arrival {
        rel: "person".to_string(),
        sign: ArrivalSign::Add,
        row: vec![Value::Text(name.to_string()), Value::Integer(age)],
    };
    vec![
        vec![add("ann", 30), add("kid", 7)],
        vec![add("bob", 44)],
        vec![add("cyd", 21), add("dee", 12)],
    ]
}

#[tokio::test]
async fn summary_names_the_ir_verbs_and_the_seam_compiles_each_text_once() {
    std::env::set_var("DL_TRACE_SUMMARY", "1");
    let program = compile_sf_guard();
    let seam = SqliteSeam::in_memory().expect("seam");
    seam.count_prepares();
    trace::reset();
    run_schedule(&program, &seam, &three_ticks(), 100)
        .await
        .expect("three tick fold");

    let rows = trace::summary_rows();
    let pairs: Vec<(String, String)> = rows
        .iter()
        .map(|(label, _)| (label.verb.to_string(), label.relation.to_string()))
        .collect();
    for wanted in [
        ("arrive", "person"),
        ("stage", "person"),
        ("publish", "person"),
        ("publish", "adult"),
        ("level_insert", "adult"),
        ("clear", "prepare"),
        ("clear", "promote"),
    ] {
        assert!(
            pairs
                .iter()
                .any(|(verb, relation)| verb == wanted.0 && relation == wanted.1),
            "summary is missing {wanted:?}: {pairs:?}"
        );
    }
    // Every relation a row names is one the IR declares, never text parsed out
    // of a statement.
    let declared: Vec<&str> = program
        .relations
        .iter()
        .map(|relation| relation.rel.as_str())
        .collect();
    for (verb, relation) in &pairs {
        let boundary = matches!(relation.as_str(), "-" | "prepare" | "merge" | "promote");
        assert!(
            boundary || declared.contains(&relation.as_str()),
            "{verb} named an undeclared relation {relation}"
        );
    }

    // COUNT: distinct texts fit the cache, so none is compiled twice, and the
    // fold dispatches far more statements than it has texts.
    assert!(
        seam.distinct_sql_texts() <= seam.statement_cache_capacity(),
        "cache holds {} of {} texts",
        seam.statement_cache_capacity(),
        seam.distinct_sql_texts()
    );
    assert!(
        seam.dispatches() >= 2 * seam.distinct_sql_texts() as u64,
        "{} dispatches over {} texts is no reuse",
        seam.dispatches(),
        seam.distinct_sql_texts()
    );
}

// TEST: the arrival chunk size comes from the connection's own binding ceiling,
// not from a constant a comment claims SQLite enforces.
#[test]
fn the_variable_budget_is_read_from_the_connection() {
    let seam = SqliteSeam::in_memory().expect("seam");
    assert_eq!(seam.variable_limit(), 32_766);
}

// TEST: the boundary read folds duplicate rows through an index, one probe per
// row. Pre-fix it scanned the rows already collected, so 20000 rows cost
// 200 million comparisons and this assertion read 20000 vs 199990000.
#[test]
fn boundary_delta_probes_once_per_row() {
    let program_rows = 20_000usize;
    let relation = sprefa_engine_rs::types::IncrementalRelationPlan {
        rel: "measured".to_string(),
        kind: sprefa_engine_rs::types::RelationKind::Set,
        table_name: "measured".to_string(),
        delta_table_name: "__delta_measured".to_string(),
        frontier_table_name: "__frontier_measured".to_string(),
        next_frontier_table_name: "__next_frontier_measured".to_string(),
        departure_frontier_table_name: None,
        shared_frontier: None,
        columns: vec!["value".to_string()],
        column_types: vec![sprefa_engine_rs::types::RowColumnType::Int],
        key_indices: vec![0],
        arrival_add_sql: None,
        arrival_del_sql: None,
        boundary_sql: String::new(),
    };
    let result = sprefa_engine_rs::types::QueryResult {
        rows: (0..program_rows)
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
    let before = sprefa_engine_rs::incremental::dedup_probes();
    let started = std::time::Instant::now();
    let delta =
        sprefa_engine_rs::incremental::boundary_delta(&relation, &result).expect("boundary delta");
    let probes = sprefa_engine_rs::incremental::dedup_probes() - before;
    assert_eq!(delta.add.len(), program_rows);
    assert!(
        probes <= 2 * program_rows as u64,
        "{probes} probes for {program_rows} rows is a scan"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "quadratic timing: {:?}",
        started.elapsed()
    );
}
