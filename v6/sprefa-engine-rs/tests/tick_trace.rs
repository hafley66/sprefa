// TEST: the resident runner arms DL_TRACE_SUMMARY on its own (run.rs:694
// force_summary(), beside arm()), so dl_tick_cost's "wall" row carries a real
// sqlite_ms instead of the always-zero default a run without the env var read
// before this fix. Fail-first: reverting that one line reds this test.

use std::collections::BTreeMap;
use std::process::Command;

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::executors::cost::TickCostExecutor;
use sprefa_engine_rs::hosts::IHostExecutor;
use sprefa_engine_rs::serve::{arrival_batch, ArrivalDto};
use sprefa_engine_rs::sql::SqliteSeam;
use sprefa_engine_rs::trace;
use sprefa_engine_rs::types::{Arrival, ProgramJson};
use sprefa_engine_rs::GenProgram;

fn compile_ghcache() -> GenProgram {
    let engine = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = engine.join("../dl/ghcache/ghcache.dl6");
    let temp = tempfile::tempdir().expect("temporary compiler output directory");
    let generated = temp.path().join("ghcache.program.rs");
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

fn ghcache_schedule() -> Vec<Vec<Arrival>> {
    let engine = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = engine.join("../dl/ghcache/ghcache.schedule.json");
    let text = std::fs::read_to_string(&path).expect("read ghcache.schedule.json");
    let batches: Vec<Vec<ArrivalDto>> = serde_json::from_str(&text).expect("parse schedule");
    batches
        .into_iter()
        .map(|batch| arrival_batch(batch).unwrap_or_else(|failure| panic!("{failure}")))
        .collect()
}

#[tokio::test]
async fn the_wall_row_carries_a_real_sqlite_ms_once_armed() {
    let program = compile_ghcache();
    let schedule = ghcache_schedule();
    let seam = SqliteSeam::in_memory().expect("seam");
    trace::reset();
    trace::force_summary();
    run_schedule(&program, &seam, &schedule, 100)
        .await
        .expect("ghcache schedule fold");

    let mut env = BTreeMap::new();
    env.insert("bucket".to_string(), "0".to_string());
    let rows = TickCostExecutor
        .run("dl_tick_cost", "", &env)
        .expect("dl_tick_cost read");
    let wall = rows
        .iter()
        .find(|row| row.get("executor").and_then(|value| value.as_str()) == Some("wall"))
        .unwrap_or_else(|| panic!("no wall row in {rows:?}"));
    let sqlite_ms = wall
        .get("sqlite_ms")
        .and_then(|value| value.as_i64())
        .unwrap_or_else(|| panic!("wall row has no sqlite_ms: {wall:?}"));
    assert!(
        sqlite_ms > 0,
        "wall row sqlite_ms should be > 0 once force_summary is armed, got {wall:?}"
    );
}
