//! COUNT TEST for issues/one-tick-path: a sequenced `<+` arm runs inside the
//! incremental path and a level whose inputs did not move pays nothing.
//!
//! @comment-ok: TEST header carrying the fail-pre-fix receipt.
//!
//! Fail-pre-fix receipt, measured 2026-08-23. Before the arc, ONE arm reading
//! `pre/1` set `ordered_program` and the whole module folded through
//! `ordered.rs::run_tick`, which rebuilt every level from its base tables twice
//! a tick: this program's 50-level chain recomputed 100 times on a tick whose
//! only arrival was an `increment`, which no level reads. The first draft of
//! the one-path runtime, before `level_sources`, ran all 50 levels twice for
//! the same arrival (100 level runs, 3499 statements on ghcache's shape).

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering::Relaxed;

use sprefa_engine_rs::incremental::level_runs;
use sprefa_engine_rs::program::run_boot;
use sprefa_engine_rs::run;
use sprefa_engine_rs::sql::{SqlRunner, SEAM_TALLY};
use sprefa_engine_rs::types::{ArmSchedule, Arrival, ArrivalSign, Value};

const LEVELS: usize = 50;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root")
}

/// One `pre/1` arm over a keyed counter plus a 50-deep `<-` chain that the arm
/// never touches.
fn program_source() -> String {
    let mut text = String::from(
        "rel increment(name: text) log keep(all).\n\
         rel base(value: int) key(1).\n\
         rel counter(name: text, total: int) key(1).\n\n\
         counter(Name, Next) <+\n  increment(Name),\n  pre(counter(Name, Total), 0),\n  Next := Total + 1.\n\n\
         level0(Value) <-\n  base(Value).\n",
    );
    for index in 1..LEVELS {
        text.push_str(&format!(
            "level{index}(Value) <-\n  level{}(Value).\n",
            index - 1
        ));
    }
    text
}

fn emitted() -> PathBuf {
    let root = repo_root();
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/one_tick_path");
    std::fs::create_dir_all(&directory).expect("create the emit directory");
    let source = directory.join("one_tick_path.dl6");
    let module = directory.join("one_tick_path.rs");
    std::fs::write(&source, program_source()).expect("write the program source");
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

fn arrival(rel: &str, row: Vec<Value>) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign: ArrivalSign::Add,
        row,
    }
}

struct Tick {
    statements: u64,
    level_runs: u64,
    carry_pending: bool,
}

#[test]
fn a_pre_arm_does_not_pay_for_levels_it_never_touches() {
    let module = emitted();
    let program = run::load_program(&module)
        .expect("load the emitted module")
        .program;
    assert_eq!(
        program.levels.len(),
        LEVELS,
        "the chain lowered to one level statement per rule"
    );
    let sequenced = program
        .edges
        .iter()
        .filter(|edge| edge.schedule == ArmSchedule::Sequenced)
        .count();
    assert_eq!(
        sequenced,
        program.edges.len(),
        "every arm of this program reads pre/1 and is sequenced"
    );

    let seam = run::open_seam(None).expect("in-memory seam");
    seam.size_statement_cache(program.stable_sql_count() + 64);
    seam.run_program_ddl(&program.ddl, &program.queries)
        .expect("DDL execution failed");
    run_boot(&seam, &program.boot);

    let mut fold = |arrivals: Vec<Arrival>| -> Tick {
        let statements = SEAM_TALLY.statements.load(Relaxed);
        let runs = level_runs();
        let deltas = program
            .run_tick(&seam, &arrivals)
            .unwrap_or_else(|failure| panic!("{failure:?}"));
        Tick {
            statements: SEAM_TALLY.statements.load(Relaxed) - statements,
            level_runs: level_runs() - runs,
            carry_pending: deltas.carry_pending,
        }
    };

    // The chain's only source moves, so every level downstream of it runs.
    let seeded = fold(vec![arrival("base", vec![Value::Integer(1)])]);
    assert!(
        seeded.level_runs >= LEVELS as u64,
        "an arrival into the chain's source runs all {LEVELS} levels, ran {}",
        seeded.level_runs
    );

    // Drain the carry that seeding staged, so the ticks below start settled.
    let mut drains = 0usize;
    let mut pending = seeded.carry_pending;
    while pending {
        pending = fold(Vec::new()).carry_pending;
        drains += 1;
        assert!(drains < 20, "the seed never drained");
    }
    // One tick after a busy one still empties that tick's delta and frontier
    // tables, which is work the busy tick's own writes bought.
    fold(Vec::new());

    // Nothing arrives and nothing carries: the tick reads the probe and stops.
    let idle = fold(Vec::new());
    assert_eq!(idle.level_runs, 0, "an idle tick ran a level");
    assert!(
        idle.statements < 10,
        "an idle tick cost {} statements",
        idle.statements
    );

    // The pre arm's trigger feeds no level, so no level has an input that moved.
    let folded = fold(vec![arrival(
        "increment",
        vec![Value::Text("clicks".to_string())],
    )]);
    assert_eq!(
        folded.level_runs, 0,
        "a level whose inputs did not move recomputed {} times",
        folded.level_runs
    );
    assert!(
        folded.statements <= 40,
        "the pre arm's tick cost {} statements over a {LEVELS}-level program it does not read",
        folded.statements
    );

    // The fold is still the fold: two increments and the counter reads 2.
    while fold(Vec::new()).carry_pending {}
    fold(Vec::new());
    fold(vec![arrival(
        "increment",
        vec![Value::Text("clicks".to_string())],
    )]);
    let rows = seam
        .execute(&sprefa_engine_rs::types::SqlStatement {
            sql: program
                .final_select
                .get("counter")
                .expect("counter select")
                .clone(),
            args: vec![],
        })
        .expect("counter read");
    assert_eq!(
        rows.rows.len(),
        1,
        "one counter row, read {}",
        rows.rows.len()
    );
    assert_eq!(
        rows.rows[0][1].as_i64(),
        Some(2),
        "two increments fold to 2, read {:?}",
        rows.rows[0][1]
    );
}
