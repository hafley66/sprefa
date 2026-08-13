//! Live host execution receipts. FAIL-FIRST: before driver::run_schedule_live
//! existed, the happy-path test below failed with "no rows in spanned" because
//! nothing executed the `look` template; the runtime treated hosts as
//! schedule-replay only (the network-replay gap this arc closes).

use std::collections::BTreeMap;

use sprefa_engine_rs::driver::{run_schedule, run_schedule_live};
use sprefa_engine_rs::hosts::HostLiveRunner;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, ProgramJson, SqlStatement, Value};
use sprefa_engine_rs::GenProgram;

fn fixture_program(name: &str) -> GenProgram {
    let path = format!(
        "{}/tests/fixtures/{name}.program.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let module_text = std::fs::read_to_string(&path).expect("read fixture program");
    let start = module_text.find("r#\"").expect("raw string open") + 3;
    let end = module_text[start..].find("\"#;").expect("raw string close") + start;
    let program_json: ProgramJson =
        serde_json::from_str(&module_text[start..end]).expect("fixture program json");
    GenProgram::from_json(program_json)
}

fn add(rel: &str, row: Vec<Value>) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign: ArrivalSign::Add,
        row,
    }
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

// final_select is the program's own decoded read (dict ids back to text),
// the same SQL the tick-final output uses.
fn table_rows(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> Vec<Vec<Value>> {
    let select = program.final_select.get(rel).expect("final select for rel");
    let result = seam
        .execute(&SqlStatement {
            sql: format!("SELECT * FROM ({select}) ORDER BY 1, 2, 3"),
            args: vec![],
        })
        .expect("select rows");
    result.rows
}

#[tokio::test]
async fn live_shell_probe_answers_without_a_scripted_response() {
    let program = fixture_program("live_shell_probe");
    let seam = SqliteSeam::in_memory().expect("seam");
    let schedule = vec![vec![add("source_file", vec![text("a.rs")])]];
    let fold = run_schedule_live(&program, &seam, &schedule, 100)
        .await
        .expect("live run");
    assert_eq!(
        table_rows(&program, &seam, "spanned"),
        vec![vec![text("a.rs"), Value::Integer(3), Value::Integer(9)]],
        "the printf template's span must land through demand -> execute -> response"
    );
    assert_eq!(
        fold.lines.len(),
        2,
        "one scheduled arrival tick, then exactly one host-response tick"
    );
}

/// SABOTAGE. The same program with the template forced to answer a wrong span
/// lands the wrong bytes, so the happy path's exact-row assert is a real detector.
#[tokio::test]
async fn live_shell_probe_sabotaged_template_lands_the_wrong_span() {
    let mut program = fixture_program("live_shell_probe");
    program.host_plans[0].template = "printf '{\"start\":4,\"end\":9}' # {path}".to_string();
    let seam = SqliteSeam::in_memory().expect("seam");
    let schedule = vec![vec![add("source_file", vec![text("a.rs")])]];
    run_schedule_live(&program, &seam, &schedule, 100)
        .await
        .expect("live run");
    assert_eq!(
        table_rows(&program, &seam, "spanned"),
        vec![vec![text("a.rs"), Value::Integer(4), Value::Integer(9)]],
    );
}

/// The linked twin: DL_EXTRACT_BIN is absent from the environment, so a
/// subprocess spelling would fail; rows landing proves the in-process call.
#[tokio::test]
async fn live_extract_runs_in_process_with_no_binary_configured() {
    std::env::remove_var("DL_EXTRACT_BIN");
    let program = fixture_program("live_extract_calls");
    let seam = SqliteSeam::in_memory().expect("seam");
    let target = format!(
        "{}/tests/fixtures/live_extract_target.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let schedule = vec![vec![add("file", vec![text(&target), text("digest-1")])]];
    run_schedule_live(&program, &seam, &schedule, 100)
        .await
        .expect("live run");
    let rows = table_rows(&program, &seam, "call_site");
    assert!(
        rows.iter()
            .any(|row| row == &vec![text(&target), text("digest-1"), text("helper")]),
        "extracted call facts must include main's call to helper, got {rows:?}"
    );
}

#[tokio::test]
async fn unknown_executor_is_named_at_construction() {
    let mut program = fixture_program("live_shell_probe");
    program.host_plans[0].execution = "warp_drive".to_string();
    let failure = HostLiveRunner::new(&program.host_plans, &program.rel_columns)
        .err()
        .expect("unknown executor must be an error");
    assert!(failure.message.contains("warp_drive"), "{failure}");
    assert_eq!(failure.host, "look");
}

/// The replay door stays byte-identical: the scripted-response path still runs
/// through run_schedule with hosts never executing.
#[tokio::test]
async fn scripted_replay_still_runs_without_executing_hosts() {
    let program = fixture_program("live_shell_probe");
    let seam = SqliteSeam::in_memory().expect("seam");
    let schedule = vec![vec![add("source_file", vec![text("nope.rs")])]];
    let fold = run_schedule(&program, &seam, &schedule, 100).await;
    assert_eq!(fold.lines.len(), 1);
    assert_eq!(
        table_rows(&program, &seam, "spanned"),
        Vec::<Vec<Value>>::new()
    );
}

#[test]
fn live_flag_rejects_a_scripted_response_row() {
    let program_path = format!(
        "{}/tests/fixtures/live_shell_probe.program.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let schedule_path = format!(
        "{}/tests/fixtures/live_shell_probe.scripted-response.schedule.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_emit_rust_harness"))
        .args([&program_path, &schedule_path, "--live-hosts"])
        .output()
        .expect("spawn harness");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("__host_response_look"), "{stderr}");
}

#[test]
fn template_fill_escapes_for_the_landing_quote_context() {
    let mut inputs = BTreeMap::new();
    inputs.insert("path".to_string(), text("a'b c.rs"));
    let filled =
        sprefa_engine_rs::hosts::fill_template("head -1 {path} '{path}' \"{path}\"", &inputs);
    assert_eq!(filled, "head -1 'a'\\''b c.rs' 'a'\\''b c.rs' \"a'b c.rs\"");
}
