// The `?` order tail on the Rust door: final_select carries the ORDER BY, the
// ordered base-table read plans on its own index, and the harness stops
// imposing its text sort on a cursor that already orders.
//
// Sabotage receipt: dropping ` ORDER BY` from emit_rust.pl:final_select_entry/3
// reds `ordered_rows_leave_the_cursor_in_the_asked_for_order` with the rows in
// insertion order, and reds `an_ordered_base_table_read_plans_on_its_index`
// with `USE TEMP B-TREE FOR ORDER BY`.

use std::process::Command;

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, ProgramJson, SqlStatement, Value};
use sprefa_engine_rs::GenProgram;

struct Compiled {
    program: GenProgram,
    module_path: std::path::PathBuf,
    _scratch: tempfile::TempDir,
}

fn compile() -> Compiled {
    let engine = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = engine.join("tests/fixtures/query_order_tail.dl6");
    let compile_pl = engine.join("../prolog/compile.pl");
    let emit_rust = engine.join("../prolog/emit_rust.pl");
    let scratch = tempfile::tempdir().expect("temporary compiler output directory");
    let generated = scratch.path().join("query_order_tail.program.rs");
    let goal = format!(
        "compile_dl6('{}','{}',[emitter(emit_rust:emit_program)])",
        source.display(),
        generated.display()
    );
    let output = Command::new("swipl")
        .args(["-q", "-l"])
        .arg(&compile_pl)
        .args(["-l"])
        .arg(&emit_rust)
        .args(["-g", &goal, "-g", "halt"])
        .output()
        .expect("run compile_dl6 for the Rust emitter");
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
    Compiled {
        program: GenProgram::from_json(program_json),
        module_path: generated,
        _scratch: scratch,
    }
}

fn schedule() -> Vec<Vec<Arrival>> {
    let score = |player: i64, points: i64| Arrival {
        rel: "score".to_string(),
        sign: ArrivalSign::Add,
        row: vec![Value::Integer(player), Value::Integer(points)],
    };
    let module_defs = |path: &str, defs: i64| Arrival {
        rel: "module_defs".to_string(),
        sign: ArrivalSign::Add,
        row: vec![Value::Text(path.to_string()), Value::Integer(defs)],
    };
    vec![vec![
        score(3, 5),
        score(1, 5),
        score(2, 9),
        score(4, 2),
        module_defs("b.rs", 5),
        module_defs("a.rs", 5),
        module_defs("c.rs", 9),
    ]]
}

fn read(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> Vec<Vec<Value>> {
    let select = program.final_select.get(rel).expect("final select");
    seam.execute(&SqlStatement {
        sql: select.clone(),
        args: vec![],
    })
    .expect("final read")
    .rows
}

fn integers(rows: &[Vec<Value>]) -> Vec<(i64, i64)> {
    rows.iter()
        .map(|row| match (&row[0], &row[1]) {
            (Value::Integer(left), Value::Integer(right)) => (*left, *right),
            other => panic!("not an int pair: {other:?}"),
        })
        .collect()
}

fn text_and_int(rows: &[Vec<Value>]) -> Vec<(String, i64)> {
    rows.iter()
        .map(|row| match (&row[0], &row[1]) {
            (Value::Text(left), Value::Integer(right)) => (left.clone(), *right),
            other => panic!("not a text/int pair: {other:?}"),
        })
        .collect()
}

#[tokio::test]
async fn ordered_rows_leave_the_cursor_in_the_asked_for_order() {
    let compiled = compile();
    let seam = SqliteSeam::in_memory().expect("seam");
    run_schedule(&compiled.program, &seam, &schedule(), 100)
        .await
        .expect("fold the schedule");

    // Two rows tie at 5 points and `player` breaks the tie ascending.
    assert_eq!(
        integers(&read(&compiled.program, &seam, "score")),
        vec![(2, 9), (1, 5), (3, 5), (4, 2)]
    );
    // Same shape through the intern view: two modules tie at 5 defs.
    assert_eq!(
        text_and_int(&read(&compiled.program, &seam, "module_defs")),
        vec![
            ("c.rs".to_string(), 9),
            ("a.rs".to_string(), 5),
            ("b.rs".to_string(), 5)
        ]
    );
}

#[tokio::test]
async fn an_ordered_base_table_read_plans_on_its_index() {
    let compiled = compile();
    let seam = SqliteSeam::in_memory().expect("seam");
    run_schedule(&compiled.program, &seam, &schedule(), 100)
        .await
        .expect("fold the schedule");

    let select = compiled
        .program
        .final_select
        .get("score")
        .expect("final select");
    let plan = seam
        .execute(&SqlStatement {
            sql: format!("EXPLAIN QUERY PLAN {select}"),
            args: vec![],
        })
        .expect("explain the ordered read");
    let plan_text = format!("{:?}", plan.rows);
    assert!(
        plan_text.contains("USING COVERING INDEX") || plan_text.contains("USING INDEX"),
        "the order index went unread: {plan_text}"
    );
    assert!(
        !plan_text.contains("TEMP B-TREE"),
        "the ordered read still sorts: {plan_text}"
    );
}

// A rel whose cursor orders must not be re-sorted by the harness on the way out.
#[test]
fn the_harness_prints_an_ordered_rel_in_its_cursors_order() {
    let compiled = compile();
    let output = Command::new(env!("CARGO_BIN_EXE_emit_rust_harness"))
        .arg(&compiled.module_path)
        .args([
            "--arrive",
            "score=3,5",
            "--arrive",
            "score=1,5",
            "--arrive",
            "score=2,9",
            "--arrive",
            "score=4,2",
            "--final-only",
            "--final-tsv",
            "--final-rels",
            "score",
        ])
        .output()
        .expect("spawn harness");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "score\t2\t9\nscore\t1\t5\nscore\t3\t5\nscore\t4\t2\n"
    );
}
