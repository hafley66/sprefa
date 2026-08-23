//! COUNT TEST for issues/recount-waits-for-a-retraction.

use std::path::PathBuf;
use std::process::Command;

use sprefa_engine_rs::driver::format_deltas;
use sprefa_engine_rs::program::{run_boot, GenProgram};
use sprefa_engine_rs::run;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, SqlStatement, Value};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root")
}

fn emitted(name: &str, source_text: &str) -> PathBuf {
    let root = repo_root();
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/recount_gate");
    std::fs::create_dir_all(&directory).expect("create the emit directory");
    let source = directory.join(format!("{name}.dl6"));
    let module = directory.join(format!("{name}.rs"));
    std::fs::write(&source, source_text).expect("write the program source");
    let goal = format!(
        "compile_dl6('{}','{}',[emitter(emit_rust:emit_program)])",
        source.display(),
        module.display()
    );
    let output = Command::new("swipl")
        .arg("--stack_limit=12G")
        .arg("-q")
        .args([
            "-l",
            &root.join("v6/prolog/compile.pl").display().to_string(),
        ])
        .args([
            "-l",
            &root.join("v6/prolog/emit_rust.pl").display().to_string(),
        ])
        .args(["-g", &goal])
        .args(["-g", "halt"])
        .output()
        .expect("spawn swipl");
    assert!(
        output.status.success() && module.is_file(),
        "the program did not compile: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    module
}

fn arrival(rel: &str, sign: ArrivalSign, value: i64) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign,
        row: vec![Value::Integer(value)],
    }
}

fn recount_fires() -> u64 {
    sprefa_engine_rs::trace::summary_rows()
        .into_iter()
        .filter(|(label, _)| label.verb == "recount")
        .map(|(_, stat)| stat.scopes)
        .sum()
}

fn head_rows(seam: &SqliteSeam, program: &GenProgram) -> Vec<i64> {
    let select = program.final_select.get("head").expect("head select");
    let result = seam
        .execute(&SqlStatement {
            sql: select.clone(),
            args: vec![],
        })
        .expect("head read");
    let mut values: Vec<i64> = result
        .rows
        .iter()
        .map(|row| row[0].as_i64().expect("head.value is an int"))
        .collect();
    values.sort();
    values
}

fn fold(module: &PathBuf, schedule: &[Vec<Arrival>]) -> (Vec<u64>, String, Vec<Vec<i64>>) {
    let program = run::load_program(module)
        .expect("load the emitted module")
        .program;
    let seam = run::open_seam(None).expect("in-memory seam");
    seam.size_statement_cache(program.stable_sql_count() + 64);
    seam.run_program_ddl(&program.ddl, &program.queries)
        .expect("DDL execution failed");
    run_boot(&seam, &program.boot);

    let mut fires = Vec::new();
    let mut lines = Vec::new();
    let mut checkpoints = Vec::new();
    for (tick, arrivals) in schedule.iter().enumerate() {
        let before = recount_fires();
        let deltas = program
            .run_tick(&seam, arrivals)
            .unwrap_or_else(|failure| panic!("tick {tick}: {failure:?}"));
        fires.push(recount_fires() - before);
        lines.push(format_deltas(&program, tick + 1, &deltas));
        checkpoints.push(head_rows(&seam, &program));
        assert!(
            !deltas.carry_pending,
            "tick {tick}: a flat program must not carry"
        );
    }
    (fires, lines.join("\n") + "\n", checkpoints)
}

fn gated_and_naive(module: &PathBuf, schedule: &[Vec<Arrival>]) -> (Vec<u64>, Vec<Vec<i64>>) {
    let (gated_fires, gated_log, gated_rows) = fold(module, schedule);
    std::env::set_var("DL_NO_SHRINK_GATE", "1");
    let (_, naive_log, naive_rows) = fold(module, schedule);
    std::env::remove_var("DL_NO_SHRINK_GATE");
    assert_eq!(gated_log.as_bytes(), naive_log.as_bytes());
    assert_eq!(gated_rows, naive_rows);
    (gated_fires, gated_rows)
}

#[test]
fn recount_waits_for_a_retraction_or_a_negated_addition() {
    sprefa_engine_rs::trace::arm();
    sprefa_engine_rs::trace::force_summary();

    let positive = emitted(
        "positive",
        "rel a(value: int) key(1).\n\nhead(Value) <-\n  a(Value).\n",
    );
    let additions: Vec<Vec<Arrival>> = (1..=5)
        .map(|value| vec![arrival("a", ArrivalSign::Add, value)])
        .collect();
    let (fires, rows) = gated_and_naive(&positive, &additions);
    assert_eq!(fires, vec![0, 0, 0, 0, 0]);
    assert_eq!(rows.last(), Some(&vec![1, 2, 3, 4, 5]));

    let retraction = vec![
        vec![arrival("a", ArrivalSign::Add, 3)],
        vec![arrival("a", ArrivalSign::Del, 3)],
    ];
    let (fires, rows) = gated_and_naive(&positive, &retraction);
    assert_eq!(fires, vec![0, 1]);
    assert_eq!(rows.last(), Some(&Vec::new()));

    let negated = emitted(
        "negated",
        "rel a(value: int) key(1).\nrel b(value: int) key(1).\n\nhead(Value) <-\n  a(Value),\n  !b(Value).\n",
    );
    let negated_addition = vec![
        vec![arrival("a", ArrivalSign::Add, 4)],
        vec![arrival("b", ArrivalSign::Add, 4)],
    ];
    let (fires, rows) = gated_and_naive(&negated, &negated_addition);
    assert_eq!(fires, vec![0, 1]);
    assert_eq!(rows.last(), Some(&Vec::new()));
}
