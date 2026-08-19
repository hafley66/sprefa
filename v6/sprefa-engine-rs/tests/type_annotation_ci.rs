use std::process::Command;

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, ProgramJson, SqlStatement, Value};
use sprefa_engine_rs::GenProgram;

fn compile_program() -> GenProgram {
    let engine = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = engine.join("../dl/fixtures/type-annotation-ci.dl6");
    let compile = engine.join("../prolog/compile.pl");
    let emit_rust = engine.join("../prolog/emit_rust.pl");
    let temp = tempfile::tempdir().unwrap();
    let generated = temp.path().join("type_annotation_ci.program.rs");
    let goal = format!(
        "compile_dl6('{}','{}',[emitter(emit_rust:emit_program)])",
        source.display(), generated.display()
    );
    let output = Command::new("swipl").args(["-q", "-l"]).arg(compile)
        .args(["-l"]).arg(emit_rust).args(["-g", &goal, "-g", "halt"])
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = std::fs::read_to_string(generated).unwrap();
    let start = text.find("r#\"").unwrap() + 3;
    let end = text[start..].find("\"#;").unwrap() + start;
    GenProgram::from_json(serde_json::from_str::<ProgramJson>(&text[start..end]).unwrap())
}

fn add(rel: &str, id: i64, body: &str) -> Arrival {
    Arrival { rel: rel.into(), sign: ArrivalSign::Add,
        row: vec![Value::Integer(id), Value::Text(body.into())] }
}

fn rows(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> Vec<Vec<Value>> {
    seam.execute(&SqlStatement { sql: program.final_select[rel].clone(), args: vec![] }).unwrap().rows
}

#[tokio::test]
async fn annotation_key_and_legacy_key_share_sqlite_replacement_behavior() {
    let program = compile_program();
    assert!(program.relations.iter().all(|r| !["key", "configure", "first", "second", "optional"].contains(&r.rel.as_str())));
    let seam = SqliteSeam::in_memory().unwrap();
    run_schedule(&program, &seam, &[vec![
        add("LegacyKey", 1, "old"), add("LegacyKey", 1, "new"),
        add("AnnotationKey", 1, "old"), add("AnnotationKey", 1, "new"),
    ]], 100).await.unwrap();
    let expected = vec![vec![Value::Integer(1), Value::Text("new".into())]];
    assert_eq!(rows(&program, &seam, "LegacyKey"), expected);
    assert_eq!(rows(&program, &seam, "AnnotationKey"), expected);
}
