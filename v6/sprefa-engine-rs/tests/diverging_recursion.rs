//! FAIL-PRE-FIX: this program ran 45s under a timeout gun and was killed
//! (rc=124). `reconcile_ref_count_statement` drove the recursive CTE seed
//! inside SQLite, where a hop is not countable, and `Next := Value + 1`
//! reaches no fixpoint. The emitted expand plan was parsed and ignored.
//!
//! The fixture program is the emitted rust door for
//! conformance/fixtures/23_diverging_recursion.pl.

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::sql::SqliteSeam;
use sprefa_engine_rs::types::{Arrival, ArrivalSign, BoundaryError, ProgramJson, Value};
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

fn seed(value: i64) -> Arrival {
    Arrival {
        rel: "seed_number".to_string(),
        sign: ArrivalSign::Add,
        row: vec![Value::Integer(value)],
    }
}

#[tokio::test]
async fn a_growing_measure_aborts_the_tick_naming_the_head_rel() {
    let program = fixture_program("diverging_measure_recursion");
    let seam = SqliteSeam::in_memory().expect("seam");
    let failure = run_schedule(&program, &seam, &[vec![seed(0)]], 100)
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
    // The bytes the ts door raises (1_incremental.ts bounded_wave), so the two
    // doors abort a tick identically rather than in two spellings.
    assert_eq!(
        failure.to_string(),
        "diverging_measure_recursion(counter, 1000)",
    );
}

/// SABOTAGE: the same walk on a measure that cannot grow past the base rows
/// must still close, so the cap is not simply refusing recursion.
#[tokio::test]
async fn a_bounded_measure_still_reaches_its_fixpoint() {
    let program = fixture_program("bounded_measure_recursion");
    let seam = SqliteSeam::in_memory().expect("seam");
    let schedule = vec![
        vec![Arrival {
            rel: "link".to_string(),
            sign: ArrivalSign::Add,
            row: vec![Value::Integer(1), Value::Integer(2)],
        }],
        vec![Arrival {
            rel: "link".to_string(),
            sign: ArrivalSign::Add,
            row: vec![Value::Integer(2), Value::Integer(3)],
        }],
    ];
    let fold = run_schedule(&program, &seam, &schedule, 100)
        .await
        .expect("a bounded closure reaches its fixpoint");
    assert_eq!(
        fold.lines,
        vec![
            "{\"tick\":1,\"deltas\":{\"link\":{\"add\":[[1,2]],\"del\":[]},\"reachable\":{\"add\":[[1,2]],\"del\":[]}}}",
            "{\"tick\":2,\"deltas\":{\"link\":{\"add\":[[2,3]],\"del\":[]},\"reachable\":{\"add\":[[1,3],[2,3]],\"del\":[]}}}",
        ],
    );
}
