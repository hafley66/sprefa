//! FAIL-PRE-FIX: issues/tick-transaction, docs/failure-modes.md. A tick is
//! many autocommit statements; a process killed mid-tick left base tables
//! half-promoted, and #423's recompute guard means a level whose inputs did
//! not move again is never revisited, so the half state outlives every later
//! tick. `begin_tick`/`commit_tick`/`rollback_tick` (sql.rs) wrap every tick
//! in one SQLite transaction; `drive_tick_transacted` (driver.rs) is the
//! wrap, called from `run_schedule`, `run_schedule_live`, and `LiveLoop::fold`.
//!
//! Hosts resolve strictly between ticks (`HostLiveRunner::collect` runs after
//! a tick's commit, driver.rs / run.rs), so a host executor cannot interrupt
//! a tick's own SQL. The mid-tick failure this test injects is a real one
//! already in the runtime: `diverging_measure_recursion` (tests/diverging_recursion.rs)
//! writes its arrival to a persistent table via `apply_arrivals`, then diverges
//! past `round_cap` while deriving the head relation, in the same tick.
//!
//! Sabotage receipt: comment out the `begin_tick`/`commit_tick`/`rollback_tick`
//! calls in `driver::drive_tick_transacted` (call `drive_tick` directly) and
//! `a_failed_tick_leaves_the_file_db_at_the_previous_tick_state` reds: the
//! failing tick's own arrival survives on disk.

use std::sync::atomic::Ordering::Relaxed;
use std::sync::Mutex;

use sprefa_engine_rs::driver::{drive_tick, drive_tick_transacted, run_schedule};
use sprefa_engine_rs::program::run_boot;
use sprefa_engine_rs::run::open_seam;
use sprefa_engine_rs::serve::{arrival_batch, ArrivalDto};
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam, SEAM_TALLY};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, BoundaryError, ProgramJson, Value};
use sprefa_engine_rs::GenProgram;

// SEAM_TALLY is one process-wide atomic; both tests here read exact deltas
// off it, so they run one at a time regardless of the harness's own threads.
static SERIAL: Mutex<()> = Mutex::new(());

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

fn seed(value: i64) -> Arrival {
    Arrival {
        rel: "seed_number".to_string(),
        sign: ArrivalSign::Add,
        row: vec![Value::Integer(value)],
    }
}

const SEED_NUMBER_TABLE: &str =
    "diverging_measure_recursion_is_bounded_and_loud_seed_number_1230b8accab2";
const COUNTER_TABLE: &str = "diverging_measure_recursion_is_bounded_and_loud_counter";

fn row_count(seam: &SqliteSeam, table: &str) -> i64 {
    seam.scalar(&format!("SELECT COUNT(*) FROM \"{table}\""))
        .expect("row count")
}

#[tokio::test]
async fn a_failed_tick_leaves_the_file_db_at_the_previous_tick_state() {
    let _guard = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let program = fixture_program("diverging_measure_recursion");
    let home = tempfile::tempdir().expect("tempdir");
    let db_path = home.path().join("tick.db");

    // Tick 1: no arrivals. Establishes the on-disk baseline every later tick
    // is measured against.
    let seam = open_seam(Some(&db_path)).expect("file seam");
    run_schedule(&program, &seam, &[vec![]], 100)
        .await
        .expect("an empty tick commits trivially");
    drop(seam);

    let reopened = open_seam(Some(&db_path)).expect("reopen the file seam");
    let baseline_seed = row_count(&reopened, SEED_NUMBER_TABLE);
    let baseline_counter = row_count(&reopened, COUNTER_TABLE);
    drop(reopened);

    // Tick 2: seed(0) writes into seed_number via apply_arrivals, then the
    // counter's `value := value + 1` recursion never reaches a fixpoint and
    // aborts the tick past round_cap=1000 (BoundaryError::DivergingMeasureRecursion).
    let failing = open_seam(Some(&db_path)).expect("file seam for the failing tick");
    let failure = run_schedule(&program, &failing, &[vec![seed(0)]], 100)
        .await
        .err()
        .expect("a growing measure has no fixpoint to reach");
    assert_eq!(
        failure,
        BoundaryError::DivergingMeasureRecursion {
            rel: "counter".to_string(),
            round_cap: 1000,
        },
    );
    drop(failing);

    let after = open_seam(Some(&db_path)).expect("reopen after the failed tick");
    assert_eq!(
        row_count(&after, SEED_NUMBER_TABLE),
        baseline_seed,
        "a failed tick's own arrival must not survive on disk"
    );
    assert_eq!(
        row_count(&after, COUNTER_TABLE),
        baseline_counter,
        "a failed tick must not leave any half-derived rows"
    );
}

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
    let output = std::process::Command::new("swipl")
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
async fn a_tick_transaction_costs_exactly_begin_and_commit() {
    let _guard = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
    let program = compile_ghcache();
    let schedule = ghcache_schedule();
    let arrivals = schedule[0].clone();

    let plain = SqliteSeam::in_memory().expect("seam");
    plain.size_statement_cache(program.stable_sql_count() + 64);
    plain
        .run_program_ddl(&program.ddl, &program.queries)
        .expect("ddl");
    run_boot(&plain, &program.boot);
    let before_plain = SEAM_TALLY.statements.load(Relaxed);
    drive_tick(&program, &plain, arrivals.clone())
        .await
        .expect("plain tick");
    let plain_statements = SEAM_TALLY.statements.load(Relaxed) - before_plain;

    let transacted = SqliteSeam::in_memory().expect("seam");
    transacted.size_statement_cache(program.stable_sql_count() + 64);
    transacted
        .run_program_ddl(&program.ddl, &program.queries)
        .expect("ddl");
    run_boot(&transacted, &program.boot);
    let before_transacted = SEAM_TALLY.statements.load(Relaxed);
    drive_tick_transacted(&program, &transacted, arrivals)
        .await
        .expect("transacted tick");
    let transacted_statements = SEAM_TALLY.statements.load(Relaxed) - before_transacted;

    assert_eq!(
        transacted_statements - plain_statements,
        2,
        "one tick transaction costs exactly BEGIN + COMMIT beyond the plain tick"
    );
}
