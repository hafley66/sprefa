use std::process::Command;

use sprefa_engine_rs::driver::{drive_tick, run_schedule};
use sprefa_engine_rs::program::run_boot;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, ProgramJson, SqlStatement, Value};
use sprefa_engine_rs::GenProgram;

fn compile_fixture(fixture: &str) -> GenProgram {
    let engine = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = engine.join("../dl/fixtures").join(fixture);
    let compile = engine.join("../prolog/compile.pl");
    let emit_rust = engine.join("../prolog/emit_rust.pl");
    let temp = tempfile::tempdir().unwrap();
    let generated = temp.path().join(format!("{fixture}.program.rs"));
    let goal = format!(
        "compile_dl6('{}','{}',[emitter(emit_rust:emit_program)])",
        source.display(),
        generated.display()
    );
    let output = Command::new("swipl")
        .args(["-q", "-l"])
        .arg(compile)
        .args(["-l"])
        .arg(emit_rust)
        .args(["-g", &goal, "-g", "halt"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(generated).unwrap();
    let start = text.find("r#\"").unwrap() + 3;
    let end = text[start..].find("\"#;").unwrap() + start;
    GenProgram::from_json(serde_json::from_str::<ProgramJson>(&text[start..end]).unwrap())
}

fn compile_program() -> GenProgram {
    compile_fixture("type-annotation-ci.dl6")
}

fn add(rel: &str, id: i64, body: &str) -> Arrival {
    Arrival {
        rel: rel.into(),
        sign: ArrivalSign::Add,
        row: vec![Value::Integer(id), Value::Text(body.into())],
    }
}

fn rows(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> Vec<Vec<Value>> {
    seam.execute(&SqlStatement {
        sql: program.final_select[rel].clone(),
        args: vec![],
    })
    .unwrap()
    .rows
}

#[tokio::test]
async fn annotation_key_and_legacy_key_share_sqlite_replacement_behavior() {
    let program = compile_program();
    assert!(program
        .relations
        .iter()
        .all(|r| !["key", "configure", "first", "second", "optional"].contains(&r.rel.as_str())));
    let seam = SqliteSeam::in_memory().unwrap();
    run_schedule(
        &program,
        &seam,
        &[vec![
            add("LegacyKey", 1, "old"),
            add("LegacyKey", 1, "new"),
            add("AnnotationKey", 1, "old"),
            add("AnnotationKey", 1, "new"),
        ]],
        100,
    )
    .await
    .unwrap();
    let expected = vec![vec![Value::Integer(1), Value::Text("new".into())]];
    assert_eq!(rows(&program, &seam, "LegacyKey"), expected);
    assert_eq!(rows(&program, &seam, "AnnotationKey"), expected);
}

fn option_arrival(sign: ArrivalSign, value: &str, body: &str) -> Arrival {
    Arrival {
        rel: "KeyedOption".into(),
        sign,
        row: vec![Value::Text(value.into()), Value::Text(body.into())],
    }
}

fn delta_rows<'a>(
    deltas: &'a sprefa_engine_rs::types::TickDeltas,
    rel: &str,
) -> &'a sprefa_engine_rs::types::RelDelta {
    deltas.rels.iter().find(|delta| delta.rel == rel).unwrap()
}

#[tokio::test]
async fn keyed_option_program_json_interns_public_values_and_retracts_stale_rows() {
    let program = compile_fixture("keyed-option-runtime.dl6");
    assert_eq!(
        program.enum_ref_columns["KeyedOption"][0]
            .as_ref()
            .unwrap()
            .endpoint_index,
        None
    );
    assert!(program.enum_types[0].identity.is_some());
    let seam = SqliteSeam::in_memory().unwrap();
    seam.run_ddl(&program.ddl).unwrap();
    run_boot(&seam, &program.boot);

    let first = drive_tick(
        &program,
        &seam,
        vec![option_arrival(
            ArrivalSign::Add,
            r#"{"tag":"some","value":7}"#,
            "old",
        )],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&first, "KeyedOption").add,
        vec![vec![
            Value::Text(r#"{"tag":"some","value":7}"#.into()),
            Value::Text("old".into()),
        ]]
    );

    let replacement = drive_tick(
        &program,
        &seam,
        vec![option_arrival(
            ArrivalSign::Add,
            r#"{"value":7,"tag":"some"}"#,
            "new",
        )],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&replacement, "KeyedOption").add,
        vec![vec![
            Value::Text(r#"{"tag":"some","value":7}"#.into()),
            Value::Text("new".into()),
        ]]
    );
    assert_eq!(
        delta_rows(&replacement, "KeyedOption").del,
        vec![vec![
            Value::Text(r#"{"tag":"some","value":7}"#.into()),
            Value::Text("old".into()),
        ]]
    );

    let stale = drive_tick(
        &program,
        &seam,
        vec![option_arrival(
            ArrivalSign::Del,
            r#"{"tag":"some","value":7}"#,
            "old",
        )],
    )
    .await
    .unwrap();
    assert!(delta_rows(&stale, "KeyedOption").add.is_empty());
    assert!(delta_rows(&stale, "KeyedOption").del.is_empty());

    let none = drive_tick(
        &program,
        &seam,
        vec![option_arrival(
            ArrivalSign::Add,
            r#"{"tag":"none"}"#,
            "none",
        )],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&none, "KeyedOption").add,
        vec![vec![
            Value::Text(r#"{"tag":"none"}"#.into()),
            Value::Text("none".into()),
        ]]
    );
    let ids = seam
        .execute(&SqlStatement {
            sql: "SELECT \"id\" FROM \"__enum_identity___opt_int\" ORDER BY \"id\"".into(),
            args: vec![],
        })
        .unwrap()
        .rows;
    assert_eq!(ids, vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]);
}

fn relation_option_arrival(sign: ArrivalSign, value: &str, body: &str) -> Arrival {
    Arrival {
        rel: "KeyedRelationOption".into(),
        sign,
        row: vec![Value::Text(value.into()), Value::Text(body.into())],
    }
}

#[tokio::test]
async fn keyed_relation_option_program_json_normalizes_and_decodes_through_sqlite() {
    let program = compile_fixture("keyed-option-relation-runtime.dl6");
    assert_eq!(
        program.enum_ref_columns["KeyedRelationOption"][0]
            .as_ref()
            .unwrap()
            .endpoint_index,
        None
    );
    assert_eq!(
        program.struct_ref_columns["__opt_Person_some"][1],
        Some("Person".into())
    );
    let seam = SqliteSeam::in_memory().unwrap();
    seam.run_ddl(&program.ddl).unwrap();
    run_boot(&seam, &program.boot);

    let first = drive_tick(
        &program,
        &seam,
        vec![relation_option_arrival(
            ArrivalSign::Add,
            r#"{"tag":"some","value":{"id":1,"name":"Ada"}}"#,
            "old",
        )],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&first, "KeyedRelationOption").add,
        vec![vec![
            Value::Text(r#"{"tag":"some","value":{"id":1,"name":"Ada"}}"#.into()),
            Value::Text("old".into()),
        ]]
    );

    let replacement = drive_tick(
        &program,
        &seam,
        vec![relation_option_arrival(
            ArrivalSign::Add,
            r#"{"value":{"name":"Ada","id":1},"tag":"some"}"#,
            "new",
        )],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&replacement, "KeyedRelationOption").add,
        vec![vec![
            Value::Text(r#"{"tag":"some","value":{"id":1,"name":"Ada"}}"#.into()),
            Value::Text("new".into()),
        ]]
    );
    assert_eq!(
        delta_rows(&replacement, "KeyedRelationOption").del,
        vec![vec![
            Value::Text(r#"{"tag":"some","value":{"id":1,"name":"Ada"}}"#.into()),
            Value::Text("old".into()),
        ]]
    );

    let stale = drive_tick(
        &program,
        &seam,
        vec![relation_option_arrival(
            ArrivalSign::Del,
            r#"{"tag":"some","value":{"id":1,"name":"Ada"}}"#,
            "old",
        )],
    )
    .await
    .unwrap();
    assert!(delta_rows(&stale, "KeyedRelationOption").add.is_empty());
    assert!(delta_rows(&stale, "KeyedRelationOption").del.is_empty());

    let retraction = drive_tick(
        &program,
        &seam,
        vec![relation_option_arrival(
            ArrivalSign::Del,
            r#"{"tag":"some","value":{"id":1,"name":"Ada"}}"#,
            "new",
        )],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&retraction, "KeyedRelationOption").del,
        vec![vec![
            Value::Text(r#"{"tag":"some","value":{"id":1,"name":"Ada"}}"#.into()),
            Value::Text("new".into()),
        ]]
    );
}
