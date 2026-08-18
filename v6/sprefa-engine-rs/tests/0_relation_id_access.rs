use std::process::Command;

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{
    Arrival, ArrivalSign, ProgramJson, RowColumnType, SqlStatement, Value,
};
use sprefa_engine_rs::GenProgram;

fn compile_program() -> GenProgram {
    let engine = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = engine.join("../tsv2/goldens/relation_id_access/0_relation_id_access.dl6");
    let compile = engine.join("../prolog/compile.pl");
    let emit_rust = engine.join("../prolog/emit_rust.pl");
    let temp = tempfile::tempdir().expect("temporary compiler output directory");
    let generated = temp.path().join("relation_id_access.program.rs");
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

fn add(rel: &str, row: Vec<Value>) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign: ArrivalSign::Add,
        row,
    }
}

fn rows(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> Vec<Vec<Value>> {
    let select = program.final_select.get(rel).expect("final select");
    seam.execute(&SqlStatement {
        sql: select.clone(),
        args: vec![],
    })
    .expect("read final relation")
    .rows
}

#[tokio::test]
async fn relation_id_endpoint_stays_integer_while_value_follows_its_row() {
    let program = compile_program();
    let relation = |name: &str| {
        program
            .relations
            .iter()
            .find(|item| item.rel == name)
            .expect("relation plan")
    };
    assert_eq!(
        relation("IdOnly").column_types,
        vec![RowColumnType::RelationId]
    );
    assert_eq!(relation("ValueOnly").column_types, vec![RowColumnType::Ref]);
    assert_eq!(
        relation("Both").column_types,
        vec![RowColumnType::Ref, RowColumnType::RelationId]
    );

    let id_only_sql = program.final_select.get("IdOnly").expect("IdOnly SQL");
    let value_only_sql = program
        .final_select
        .get("ValueOnly")
        .expect("ValueOnly SQL");
    let both_sql = program.final_select.get("Both").expect("Both SQL");
    assert_eq!(
        id_only_sql
            .matches("__ref_0_relation_id_access_Revision")
            .count(),
        0
    );
    assert_eq!(
        value_only_sql
            .matches("__ref_0_relation_id_access_Revision")
            .count(),
        1
    );
    assert_eq!(
        both_sql
            .matches("__ref_0_relation_id_access_Revision")
            .count(),
        1
    );

    let seam = SqliteSeam::in_memory().expect("SQLite seam");
    run_schedule(
        &program,
        &seam,
        &[vec![
            add(
                "File",
                vec![
                    Value::Text(r#"{"oid":"r1"}"#.to_string()),
                    Value::Text("src/main.dl6".to_string()),
                ],
            ),
            add(
                "Holder",
                vec![Value::Text(
                    r#"{"revision":{"oid":"r1"},"path":"src/main.dl6"}"#.to_string(),
                )],
            ),
        ]],
        100,
    )
    .await
    .expect("generated ProgramJson runtime execution");

    assert_eq!(
        rows(&program, &seam, "IdOnly"),
        vec![vec![Value::Integer(1)]]
    );
    assert_eq!(
        rows(&program, &seam, "DotId"),
        vec![vec![Value::Integer(1)]]
    );
    assert_eq!(
        rows(&program, &seam, "Both"),
        vec![vec![
            Value::Text(r#"{"oid":"r1"}"#.to_string()),
            Value::Integer(1)
        ]]
    );
}
