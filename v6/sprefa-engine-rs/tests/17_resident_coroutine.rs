//! The resident coroutine end to end: a source session's turns arrive, fold
//! into same-role runs, pair into one bundle and one ask, and the resident's
//! reply arrives at a later tick as a plain arrival into a base rel.
//!
//! Regenerate the snapshot with:
//! swipl -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl \
//!   -g "compile_dl6('v6/dl/fixtures/resident-coroutine.dl6', \
//!       'v6/sprefa-engine-rs/tests/fixtures/resident-coroutine.program.rs', \
//!       [emitter(emit_rust:emit_program)])" -g halt

use sprefa_engine_rs::driver::run_schedule;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, SqlStatement, Value};
use sprefa_engine_rs::GenProgram;

#[path = "fixtures/resident-coroutine.program.rs"]
mod resident_coroutine_program;

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

fn add(rel: &str, row: Vec<Value>) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign: ArrivalSign::Add,
        row,
    }
}

fn turn(number: i64, role: &str, said: &str) -> Arrival {
    add(
        "turn",
        vec![
            text("s"),
            Value::Integer(number),
            Value::Integer(100 + number),
            text(role),
            text(said),
        ],
    )
}

// The program's own decoded read, the one GET /rel/{name} answers from.
fn final_rows(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> Vec<Vec<Value>> {
    let sql = program
        .final_select
        .get(rel)
        .unwrap_or_else(|| panic!("{rel} has a final select"))
        .clone();
    let result = seam
        .execute(&SqlStatement { sql, args: vec![] })
        .unwrap_or_else(|failure| panic!("read {rel}: {failure}"));
    let mut rows = result.rows.clone();
    rows.sort_by_key(|row| format!("{row:?}"));
    rows
}

fn session_turns() -> Vec<Arrival> {
    vec![
        turn(1, "user", "hi"),
        turn(2, "assistant", "one"),
        turn(3, "assistant", "two"),
        turn(4, "user", "more"),
        turn(5, "user", "please"),
        turn(6, "assistant", "done"),
    ]
}

const PROMPT: &str = "<ai>\none\ntwo\n</ai>\n<user>\nmore\nplease\n</user>";

#[tokio::test]
async fn six_turns_fold_into_four_runs_one_bundle_and_one_ask() {
    let program = GenProgram::from_json(resident_coroutine_program::program());
    let seam = SqliteSeam::in_memory().expect("seam");
    let fold = run_schedule(&program, &seam, &[session_turns()], 100)
        .await
        .expect("the turns fold");

    assert_eq!(
        final_rows(&program, &seam, "run"),
        vec![
            vec![text("s"), Value::Integer(1), text("user"), text("hi")],
            vec![
                text("s"),
                Value::Integer(2),
                text("assistant"),
                text("one\ntwo")
            ],
            vec![
                text("s"),
                Value::Integer(4),
                text("user"),
                text("more\nplease")
            ],
            vec![text("s"), Value::Integer(6), text("assistant"), text("done")],
        ],
        "a run is the maximal same-role stretch, its turns concatenated in turn order"
    );

    assert_eq!(
        final_rows(&program, &seam, "bundle"),
        vec![vec![
            text("s"),
            Value::Integer(2),
            Value::Integer(4),
            text("one\ntwo"),
            text("more\nplease")
        ]],
        "the only assistant-then-user pair with no run between them"
    );

    assert_eq!(
        final_rows(&program, &seam, "resident_ask"),
        vec![vec![text("s"), Value::Integer(4), text(PROMPT)]],
        "one bundle asks the resident once"
    );

    assert_eq!(
        final_rows(&program, &seam, "handled"),
        Vec::<Vec<Value>>::new(),
        "nothing wrote resident, so the ask is standing and unanswered"
    );

    assert_eq!(fold.lines.len(), 1, "level rules settle inside their tick");
    assert!(
        !fold.lines[0].contains("\"handled\""),
        "no handled delta before a reply arrives: {}",
        fold.lines[0]
    );
}

// The reply is an ordinary arrival into a base rel no rule heads, the same
// door hosts.rs:1840 already pushes its collected rows through.
#[tokio::test]
async fn the_reply_arrives_as_a_plain_arrival_and_handled_answers() {
    let program = GenProgram::from_json(resident_coroutine_program::program());
    let seam = SqliteSeam::in_memory().expect("seam");
    let reply = add(
        "resident",
        vec![
            text("s"),
            Value::Integer(4),
            Value::Integer(9),
            text("reply"),
        ],
    );
    let fold = run_schedule(&program, &seam, &[session_turns(), vec![reply]], 100)
        .await
        .expect("the reply folds");

    assert_eq!(
        final_rows(&program, &seam, "resident_ask"),
        vec![vec![text("s"), Value::Integer(4), text(PROMPT)]],
        "the ask stays; answering it is a query, not a retraction"
    );
    assert_eq!(
        final_rows(&program, &seam, "handled"),
        vec![vec![text("s"), Value::Integer(4)]],
        "handled reads the reply that arrived"
    );

    assert_eq!(fold.lines.len(), 2);
    assert!(
        fold.lines[1].contains("\"handled\":{\"add\":[[\"s\",4]],\"del\":[]}"),
        "the reply's tick carries the handled add: {}",
        fold.lines[1]
    );
}
