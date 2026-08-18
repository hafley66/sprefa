use std::collections::BTreeMap;
use std::process::Command;

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, ProgramJson, SqlStatement, Value};
use sprefa_engine_rs::GenProgram;

fn compile_program() -> GenProgram {
    let engine = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = engine.join("../tsv2/goldens/module_storage_runtime/0_module_storage_runtime.dl6");
    let compile = engine.join("../prolog/compile.pl");
    let emit_rust = engine.join("../prolog/emit_rust.pl");
    let temp = tempfile::tempdir().expect("temporary compiler output directory");
    let generated = temp.path().join("module_storage_runtime.program.rs");
    let goal = format!(
        "compile_dl6('{}','{}',[emitter(emit_rust:emit_program)])",
        source.display(),
        generated.display()
    );
    let output = Command::new("swipl")
        .args(["-q", "-l"])
        .arg(&compile)
        .args(["-l"])
        .arg(&emit_rust)
        .args(["-g", &goal, "-g", "halt"])
        .output()
        .expect("run compile_dl6 for Rust emitter");
    assert!(
        output.status.success(),
        "compile_dl6 failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let module_text = std::fs::read_to_string(&generated).expect("read emitted Rust ProgramJson");
    let start = module_text.find("r#\"").expect("raw ProgramJson open") + 3;
    let end = module_text[start..]
        .find("\"#;")
        .expect("raw ProgramJson close")
        + start;
    let program_json: ProgramJson =
        serde_json::from_str(&module_text[start..end]).expect("parse emitted ProgramJson");
    GenProgram::from_json(program_json)
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

fn add(rel: &str, value: &str) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign: ArrivalSign::Add,
        row: vec![text(value)],
    }
}

fn table_rows(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> Vec<Vec<Value>> {
    let select = program
        .final_select
        .get(rel)
        .expect("final select for relation");
    seam.execute(&SqlStatement {
        sql: format!("SELECT * FROM ({select})"),
        args: vec![],
    })
    .expect("read final relation rows")
    .rows
}

#[tokio::test]
async fn generated_program_json_executes_module_storage_names() {
    let program = compile_program();
    let physical: BTreeMap<_, _> = program
        .relations
        .iter()
        .map(|relation| (relation.rel.clone(), relation.table_name.clone()))
        .collect();
    assert_eq!(
        physical,
        BTreeMap::from([
            ("First".to_string(), "a_model_First".to_string()),
            (
                "Person".to_string(),
                "0_module_storage_runtime_Person".to_string(),
            ),
            ("Second".to_string(), "b_model_Second".to_string()),
            (
                "derived".to_string(),
                "0_module_storage_runtime_derived".to_string(),
            ),
            (
                "imported".to_string(),
                "0_module_storage_runtime_imported".to_string(),
            ),
            (
                "person".to_string(),
                "0_module_storage_runtime_person_2".to_string(),
            ),
            (
                "source".to_string(),
                "0_module_storage_runtime_source".to_string(),
            ),
        ])
    );

    let imported_rule = program
        .levels
        .iter()
        .find(|level| level.head_rel == "imported")
        .expect("imported rule");
    let derived_rule = program
        .levels
        .iter()
        .find(|level| level.head_rel == "derived")
        .expect("derived rule");
    assert!(imported_rule
        .insert_sql
        .as_ref()
        .expect("imported insert SQL")
        .contains("__frontier_a_model_First"));
    assert!(imported_rule.recompute_sql.contains("a_model_First"));
    assert!(imported_rule
        .insert_sql
        .as_ref()
        .expect("imported insert SQL")
        .contains("0_module_storage_runtime_imported"));
    assert!(derived_rule
        .insert_sql
        .as_ref()
        .expect("derived insert SQL")
        .contains("__frontier_0_module_storage_runtime_imported"));
    assert!(derived_rule
        .recompute_sql
        .contains("0_module_storage_runtime_imported"));
    assert!(derived_rule
        .insert_sql
        .as_ref()
        .expect("derived insert SQL")
        .contains("0_module_storage_runtime_derived"));

    let seam = SqliteSeam::in_memory().expect("SQLite seam");
    let fold = run_schedule(
        &program,
        &seam,
        &[vec![
            add("Person", "alice"),
            add("person", "alice"),
            add("First", "alice"),
            add("Second", "bob"),
        ]],
        100,
    )
    .await
    .expect("generated ProgramJson runtime execution");
    assert_eq!(
        fold.lines,
        vec![
            "{\"tick\":1,\"deltas\":{\"First\":{\"add\":[[\"alice\"]],\"del\":[]},\"Person\":{\"add\":[[\"alice\"]],\"del\":[]},\"Second\":{\"add\":[[\"bob\"]],\"del\":[]},\"derived\":{\"add\":[[\"alice\"]],\"del\":[]},\"imported\":{\"add\":[[\"alice\"]],\"del\":[]},\"person\":{\"add\":[[\"alice\"]],\"del\":[]}}}"
        ]
    );

    assert_eq!(
        table_rows(&program, &seam, "First"),
        vec![vec![text("alice")]]
    );
    assert_eq!(
        table_rows(&program, &seam, "Person"),
        vec![vec![text("alice")]]
    );
    assert_eq!(
        table_rows(&program, &seam, "Second"),
        vec![vec![text("bob")]]
    );
    assert_eq!(
        table_rows(&program, &seam, "derived"),
        vec![vec![text("alice")]]
    );
    assert_eq!(
        table_rows(&program, &seam, "imported"),
        vec![vec![text("alice")]]
    );
    assert_eq!(
        table_rows(&program, &seam, "person"),
        vec![vec![text("alice")]]
    );
    assert_eq!(
        table_rows(&program, &seam, "source"),
        vec![vec![text("direct")]]
    );

    let public_tables = seam
        .execute(&SqlStatement {
            sql: "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name".to_string(),
            args: vec![],
        })
        .expect("read SQLite table names")
        .rows
        .into_iter()
        .map(|row| match row.first() {
            Some(Value::Text(name)) => name.clone(),
            other => panic!("unexpected SQLite table-name row: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        public_tables,
        vec![
            "0_module_storage_runtime_Person",
            "0_module_storage_runtime_derived",
            "0_module_storage_runtime_imported",
            "0_module_storage_runtime_person_2",
            "0_module_storage_runtime_source",
            "__str",
            "a_model_First",
            "b_model_Second",
        ]
    );
    for name in [
        "First", "Person", "Second", "derived", "imported", "person", "source",
    ] {
        assert!(!public_tables.iter().any(|table| table == name));
    }
}
