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

/// Every stored rel here is `rel X(name: text)`, so one shape digest covers all
/// five; a derived rel takes none. `person` folds onto `Person` in SQLite even
/// with the digest, so it keeps the deterministic `_2` collision suffix.
const SHAPE: &str = "7a5ef237b7b9";
const ENTRY: &str = "0_module_storage_runtime";

fn entry_table(rel: &str) -> String {
    format!("{ENTRY}_{rel}")
}

fn stored_table(prefix: &str, rel: &str) -> String {
    format!("{prefix}_{rel}_{SHAPE}")
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
            ("First".to_string(), stored_table("a_model", "First")),
            ("Person".to_string(), stored_table(ENTRY, "Person")),
            ("Second".to_string(), stored_table("b_model", "Second")),
            ("derived".to_string(), entry_table("derived")),
            ("imported".to_string(), entry_table("imported")),
            (
                "person".to_string(),
                format!("{}_2", stored_table(ENTRY, "person")),
            ),
            ("source".to_string(), stored_table(ENTRY, "source")),
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
    let first_table = stored_table("a_model", "First");
    let imported_table = entry_table("imported");
    assert!(imported_rule
        .insert_sql
        .as_ref()
        .expect("imported insert SQL")
        .contains(&format!("__frontier_{first_table}")));
    assert!(imported_rule.recompute_sql.contains(&first_table));
    assert!(imported_rule
        .insert_sql
        .as_ref()
        .expect("imported insert SQL")
        .contains(&imported_table));
    assert!(derived_rule
        .insert_sql
        .as_ref()
        .expect("derived insert SQL")
        .contains(&format!("__frontier_{imported_table}")));
    assert!(derived_rule.recompute_sql.contains(&imported_table));
    assert!(derived_rule
        .insert_sql
        .as_ref()
        .expect("derived insert SQL")
        .contains(&entry_table("derived")));

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
            stored_table(ENTRY, "Person"),
            entry_table("derived"),
            entry_table("imported"),
            format!("{}_2", stored_table(ENTRY, "person")),
            stored_table(ENTRY, "source"),
            "__str".to_string(),
            stored_table("a_model", "First"),
            stored_table("b_model", "Second"),
        ]
    );
    for name in [
        "First", "Person", "Second", "derived", "imported", "person", "source",
    ] {
        assert!(!public_tables.iter().any(|table| table == name));
    }
}
