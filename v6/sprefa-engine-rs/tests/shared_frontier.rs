// frontier(shared) parity on the Rust door: same fixture both arms, equal
// final rows, and a SEARCH (never SCAN) plan on the shared frontier view.

use std::process::Command;

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, ProgramJson, SqlStatement, Value};
use sprefa_engine_rs::GenProgram;

fn compile_arm(shared: bool) -> GenProgram {
    let engine = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = engine.join("../tsv2/tests/shared_frontier/sf_guard.dl6");
    let compile = engine.join("../prolog/compile.pl");
    let emit_rust = engine.join("../prolog/emit_rust.pl");
    let temp = tempfile::tempdir().expect("temporary compiler output directory");
    let generated = temp.path().join("sf_guard.program.rs");
    let options = if shared {
        "[emitter(emit_rust:emit_program), frontier(shared)]"
    } else {
        "[emitter(emit_rust:emit_program)]"
    };
    let goal = format!(
        "compile_dl6('{}','{}',{})",
        source.display(),
        generated.display(),
        options
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

fn schedule() -> Vec<Vec<Arrival>> {
    let add = |name: &str, age: i64| Arrival {
        rel: "person".to_string(),
        sign: ArrivalSign::Add,
        row: vec![Value::Text(name.to_string()), Value::Integer(age)],
    };
    vec![vec![add("ann", 30), add("kid", 7)], vec![add("bob", 44)]]
}

fn adult_rows(program: &GenProgram, seam: &SqliteSeam) -> Vec<Vec<Value>> {
    let select = program.final_select.get("adult").expect("final select");
    let mut rows = seam
        .execute(&SqlStatement {
            sql: select.clone(),
            args: vec![],
        })
        .expect("read adult")
        .rows;
    rows.sort_by(|left, right| format!("{left:?}").cmp(&format!("{right:?}")));
    rows
}

#[tokio::test]
async fn shared_arm_matches_per_rel_and_searches_the_frontier() {
    let per_rel = compile_arm(false);
    let shared = compile_arm(true);
    assert!(shared
        .relations
        .iter()
        .all(|relation| relation.shared_frontier.is_some()));
    assert!(per_rel
        .relations
        .iter()
        .all(|relation| relation.shared_frontier.is_none()));

    let per_seam = SqliteSeam::in_memory().expect("seam");
    run_schedule(&per_rel, &per_seam, &schedule(), 100)
        .await
        .expect("per_rel run");
    let shared_seam = SqliteSeam::in_memory().expect("seam");
    run_schedule(&shared, &shared_seam, &schedule(), 100)
        .await
        .expect("shared run");
    assert_eq!(
        adult_rows(&per_rel, &per_seam),
        adult_rows(&shared, &shared_seam)
    );

    let frontier_view = &shared
        .relations
        .iter()
        .find(|relation| relation.rel == "person")
        .expect("person plan")
        .frontier_table_name;
    let plan = shared_seam
        .execute(&SqlStatement {
            sql: format!(
                "EXPLAIN QUERY PLAN SELECT * FROM \"{frontier_view}\" WHERE \"_phase\" >= 0"
            ),
            args: vec![],
        })
        .expect("explain");
    let plan_text = format!("{:?}", plan.rows);
    assert!(plan_text.contains("SEARCH"), "no SEARCH in: {plan_text}");
    assert!(
        !plan_text.contains("SCAN __frontier\""),
        "frontier scanned: {plan_text}"
    );
}
