//! COUNT TEST for issues/incremental-empty-delta-skip: the per-rel frontier
//! boundary of a tick (prepare clear, promote, read_staged) costs nothing for a
//! rel whose frontier held no row.
//!
//! @comment-ok: TEST header carrying the fail-pre-fix receipt.
//!
//! Fail-pre-fix receipt, measured 2026-08-23 at effa67c95 on wide_64 (64
//! `source` rels, 64 `heavy` rules, 128 rels, no next frontier ever written).
//! per_rel busy tick 1155 statements (9.02 per rel), of which 640 were the
//! boundary: 256 prepare (a DELETE per delta AND per next frontier) and 384
//! promote (three per rel), plus one read_staged over 128 empty carry tables.
//! shared idle tick 65 statements forever, because every head's support_sql
//! names `__support_count`, a table owning no rel, which set `always` and
//! defeated the level gate outright. After: per_rel 771 (6.02 per rel), shared
//! busy 644, both arms idle 1.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering::Relaxed;

use sprefa_engine_rs::program::{run_boot, GenProgram};
use sprefa_engine_rs::run;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam, SEAM_TALLY};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, SqlStatement, Value};

/// The wide fixture the issue names: 64 sources, 64 rules, 128 rels.
const SOURCES: u64 = 64;
const RELS: u64 = SOURCES * 2;
const BUSY_TICKS: usize = 3;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root")
}

fn emitted(arm: &str, options: &str) -> PathBuf {
    let root = repo_root();
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/empty_delta_skip");
    std::fs::create_dir_all(&directory).expect("create the emit directory");
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/shared_frontier_wide/wide_64.dl6");
    let module = directory.join(format!("wide_64_{arm}.rs"));
    let goal = format!(
        "compile_dl6('{}','{}',{})",
        source.display(),
        module.display(),
        options
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

/// One row into every source rel, so every rel in the program moves.
fn busy_batch(tick: usize) -> Vec<Arrival> {
    (0..SOURCES)
        .map(|index| Arrival {
            rel: format!("source_{index}"),
            sign: ArrivalSign::Add,
            row: vec![
                Value::Text(format!("row_{tick}_{index}")),
                Value::Integer(10 + tick as i64 + index as i64),
            ],
        })
        .collect()
}

fn verb_statements(verb: &str) -> u64 {
    sprefa_engine_rs::trace::summary_rows()
        .into_iter()
        .filter(|(label, _)| label.verb == verb)
        .map(|(_, stat)| stat.calls)
        .sum()
}

fn heavy_rows(program: &GenProgram, seam: &SqliteSeam) -> usize {
    (0..SOURCES)
        .map(|index| {
            let rel = format!("heavy_{index}");
            let select = program.final_select.get(&rel).expect("heavy select");
            seam.execute(&SqlStatement {
                sql: select.clone(),
                args: vec![],
            })
            .expect("heavy read")
            .rows
            .len()
        })
        .sum()
}

struct Tick {
    statements: u64,
    boundary: u64,
    carry_reads: u64,
}

struct Fold {
    busy: Vec<Tick>,
    idle: Vec<Tick>,
    rows: usize,
}

fn fold(module: &PathBuf) -> Fold {
    let program: GenProgram = run::load_program(module)
        .expect("load the emitted module")
        .program;
    let seam = run::open_seam(None).expect("in-memory seam");
    seam.size_statement_cache(program.stable_sql_count() + 64);
    seam.run_program_ddl(&program.ddl, &program.queries)
        .expect("DDL execution failed");
    run_boot(&seam, &program.boot);

    let tick = |arrivals: Vec<Arrival>| -> Tick {
        let statements = SEAM_TALLY.statements.load(Relaxed);
        let boundary = verb_statements("clear");
        let carry_reads = verb_statements("read_staged");
        program
            .run_tick(&seam, &arrivals)
            .unwrap_or_else(|failure| panic!("{failure:?}"));
        Tick {
            statements: SEAM_TALLY.statements.load(Relaxed) - statements,
            boundary: verb_statements("clear") - boundary,
            carry_reads: verb_statements("read_staged") - carry_reads,
        }
    };
    let busy = (0..BUSY_TICKS).map(|at| tick(busy_batch(at))).collect();
    let idle = (0..3).map(|_| tick(Vec::new())).collect();
    Fold {
        busy,
        idle,
        rows: heavy_rows(&program, &seam),
    }
}

#[test]
fn a_wide_program_pays_the_frontier_boundary_only_for_the_rels_that_moved() {
    // DL_TRACE_SUMMARY is the door the per-verb counts come through, and it is
    // read once per process.
    sprefa_engine_rs::trace::force_summary();
    let mut settled: BTreeMap<&str, usize> = BTreeMap::new();
    for (arm, options) in [
        ("per_rel", "[emitter(emit_rust:emit_program)]"),
        (
            "shared",
            "[emitter(emit_rust:emit_program), frontier(shared)]",
        ),
    ] {
        let counts = fold(&emitted(arm, options));
        let last = counts.busy.last().expect("a busy tick ran");

        // Before the arc: 9.02 per rel on per_rel, 5.05 on shared.
        assert!(
            last.statements <= 7 * RELS,
            "{arm}: a busy tick cost {} statements over {RELS} rels",
            last.statements
        );
        // prepare's delta DELETE plus promote's frontier DELETE, and nothing
        // else: this program never writes a next frontier. Before: 5 per rel.
        assert!(
            last.boundary <= 2 * RELS,
            "{arm}: a busy tick spent {} statements on the frontier boundary",
            last.boundary
        );
        // A carry probe over tables that no write reached is a read of nothing.
        let carry_reads: u64 = counts
            .busy
            .iter()
            .chain(counts.idle.iter())
            .map(|tick| tick.carry_reads)
            .sum();
        assert_eq!(
            carry_reads, 0,
            "{arm}: {carry_reads} carry reads over a program that stages no carry"
        );
        // The tick after a busy one still empties that tick's deltas. Every
        // settled tick after it reads the probe and stops; a program on `tick`
        // pays one clock statement more.
        for (at, tick) in counts.idle.iter().enumerate().skip(1) {
            assert!(
                tick.statements <= 2,
                "{arm}: settled idle tick {at} cost {} statements",
                tick.statements
            );
        }
        settled.insert(arm, counts.rows);
    }
    // The fold is still the fold: one heavy row per source per busy tick.
    for (arm, rows) in &settled {
        assert_eq!(
            *rows,
            SOURCES as usize * BUSY_TICKS,
            "{arm}: the fold produced {rows} heavy rows"
        );
    }
}
