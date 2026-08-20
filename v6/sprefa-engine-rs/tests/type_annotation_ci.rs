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

/* An enum-typed (here `key(option(...))`) column holds a REFERENCE: the
 * arrival carries the option instance's integer id and the instance arrives as
 * its own variant row. `rel_column_types` already says `int` for that column.
 * FAIL-PRE-FIX: this door interned a tagged object and minted variant
 * arrivals, so the corpus spelling `KeyedOption(1, "old")` threw
 * enum_arrival_shape_mismatch. Sabotage: restore validated_tagged_object/encode
 * in src/enum_plane.rs and both tests below refuse the integer again. */

fn keyed(rel: &str, sign: ArrivalSign, id: i64, body: &str) -> Arrival {
    Arrival {
        rel: rel.into(),
        sign,
        row: vec![Value::Integer(id), Value::Text(body.into())],
    }
}

fn variant(rel: &str, id: i64, payload: Value) -> Arrival {
    Arrival {
        rel: rel.into(),
        sign: ArrivalSign::Add,
        row: vec![Value::Integer(id), payload],
    }
}

fn delta_rows<'a>(
    deltas: &'a sprefa_engine_rs::types::TickDeltas,
    rel: &str,
) -> &'a sprefa_engine_rs::types::RelDelta {
    deltas.rels.iter().find(|delta| delta.rel == rel).unwrap()
}

fn parent_row(id: i64, body: &str) -> Vec<Value> {
    vec![Value::Integer(id), Value::Text(body.into())]
}

#[tokio::test]
async fn a_keyed_option_column_carries_its_instance_reference_and_replaces_by_key() {
    let program = compile_fixture("keyed-option-runtime.dl6");
    assert_eq!(
        program.rel_column_types["KeyedOption"],
        vec![
            sprefa_engine_rs::types::RowColumnType::Int,
            sprefa_engine_rs::types::RowColumnType::Text
        ]
    );
    let seam = SqliteSeam::in_memory().unwrap();
    seam.run_ddl(&program.ddl).unwrap();
    run_boot(&seam, &program.boot);

    let first = drive_tick(
        &program,
        &seam,
        vec![
            variant("__opt_int_some", 1, Value::Integer(7)),
            keyed("KeyedOption", ArrivalSign::Add, 1, "old"),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&first, "KeyedOption").add,
        vec![parent_row(1, "old")]
    );

    let replacement = drive_tick(
        &program,
        &seam,
        vec![keyed("KeyedOption", ArrivalSign::Add, 1, "new")],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&replacement, "KeyedOption").add,
        vec![parent_row(1, "new")]
    );
    assert_eq!(
        delta_rows(&replacement, "KeyedOption").del,
        vec![parent_row(1, "old")]
    );

    let stale = drive_tick(
        &program,
        &seam,
        vec![keyed("KeyedOption", ArrivalSign::Del, 1, "old")],
    )
    .await
    .unwrap();
    assert!(delta_rows(&stale, "KeyedOption").add.is_empty());
    assert!(delta_rows(&stale, "KeyedOption").del.is_empty());

    let none = drive_tick(
        &program,
        &seam,
        vec![
            Arrival {
                rel: "__opt_int_none".into(),
                sign: ArrivalSign::Add,
                row: vec![Value::Integer(2)],
            },
            keyed("KeyedOption", ArrivalSign::Add, 2, "none"),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&none, "KeyedOption").add,
        vec![parent_row(2, "none")]
    );
    assert_eq!(
        delta_rows(&none, "__opt_int_tag").add,
        vec![vec![Value::Integer(2), Value::Text("none".into())]]
    );
}

#[tokio::test]
async fn a_keyed_relation_option_column_carries_its_instance_reference_through_sqlite() {
    let program = compile_fixture("keyed-option-relation-runtime.dl6");
    assert_eq!(
        program.struct_ref_columns["__opt_Person_some"][1],
        Some("Person".into())
    );
    let seam = SqliteSeam::in_memory().unwrap();
    seam.run_ddl(&program.ddl).unwrap();
    run_boot(&seam, &program.boot);

    let person = |text: &str| variant("__opt_Person_some", 1, Value::Text(text.into()));
    let first = drive_tick(
        &program,
        &seam,
        vec![
            person(r#"{"id":1,"name":"Ada"}"#),
            keyed("KeyedRelationOption", ArrivalSign::Add, 1, "old"),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&first, "KeyedRelationOption").add,
        vec![parent_row(1, "old")]
    );

    // The reversed-key struct canonicalizes onto the same Person row, so the
    // option instance stays one row and only the parent body replaces.
    let replacement = drive_tick(
        &program,
        &seam,
        vec![
            person(r#"{"name":"Ada","id":1}"#),
            keyed("KeyedRelationOption", ArrivalSign::Add, 1, "new"),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&replacement, "KeyedRelationOption").add,
        vec![parent_row(1, "new")]
    );
    assert_eq!(
        delta_rows(&replacement, "KeyedRelationOption").del,
        vec![parent_row(1, "old")]
    );

    let stale = drive_tick(
        &program,
        &seam,
        vec![keyed("KeyedRelationOption", ArrivalSign::Del, 1, "old")],
    )
    .await
    .unwrap();
    assert!(delta_rows(&stale, "KeyedRelationOption").add.is_empty());
    assert!(delta_rows(&stale, "KeyedRelationOption").del.is_empty());

    let retraction = drive_tick(
        &program,
        &seam,
        vec![keyed("KeyedRelationOption", ArrivalSign::Del, 1, "new")],
    )
    .await
    .unwrap();
    assert_eq!(
        delta_rows(&retraction, "KeyedRelationOption").del,
        vec![parent_row(1, "new")]
    );
}
