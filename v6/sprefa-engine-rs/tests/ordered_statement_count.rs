//! COUNT TEST for issues/ordered-tick-recompute: the ordered tick pays for what
//! changed, not for the program's size.
//!
//! @comment-ok: TEST header carrying the fail-pre-fix receipt.
//!
//! Fail-pre-fix receipt, measured 2026-08-23 at def5dbb63 on ghcache.dl6
//! (154 rels, 100 levels, 52 ordered arms), statements per tick through
//! `SEAM_TALLY`, this test's own reading:
//!
//!   tick  0: 1890 (2 arrivals)   tick  6: 1884 (1 arrival)
//!   tick  1: 1901 (1 arrival)    tick  7: 1885 (1 arrival)
//!   tick  2: 1895 (1 arrival)    tick  8: 1902 (1 arrival)
//!   tick  3: 1925 (1 arrival)    tick  9: 1881 (0 arrivals)
//!   tick  4: 1890 (1 arrival)    tick 10: 1878 (0 arrivals)
//!   tick  5: 1902 (1 arrival)
//!
//! The issue's table says 1,135 for tick 5; that number is the sum of the four
//! phases it attributed, and the tick's total is what this test reads.
//!
//! An idle tick cost the same as a working one: `read_snapshot` read all 154
//! rels five times, `recompute_levels` rebuilt all 100 levels twice, and the
//! frontier clear fired 2 statements per rel.
//!
//! The tick log is compared byte for byte against `ghcache_ticklog_base.txt`,
//! which was generated at that same sha and is the correctness receipt: a
//! skipped level that mattered moves a row and reds this test.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

use sprefa_engine_rs::driver::format_deltas;
use sprefa_engine_rs::program::run_boot;
use sprefa_engine_rs::run;
use sprefa_engine_rs::serve::{arrival_batch, ArrivalDto};
use sprefa_engine_rs::sql::SEAM_TALLY;
use sprefa_engine_rs::types::Arrival;

/// A tick with no arrival reads the clock and the carry, nothing else.
const ZERO_ARRIVAL_CAP: u64 = 100;
/// One arrival pays for its own dependency cone, not for 154 rels.
const ONE_ARRIVAL_CAP: u64 = 300;
const DRAIN_CAP: usize = 100;

fn engine_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    engine_dir()
        .join("../..")
        .canonicalize()
        .expect("canonical repo root")
}

fn modified(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .unwrap_or_else(|error| panic!("stat {}: {error}", path.display()))
        .modified()
        .expect("a modification time")
}

/// The newest mtime among the program source and everything that emits it, so a
/// compiler edit is never folded through a stale cached module.
fn emitter_stamp(root: &Path) -> std::time::SystemTime {
    let mut newest = modified(&root.join("v6/dl/ghcache/ghcache.dl6"));
    let mut roots = vec![root.join("v6/prolog")];
    while let Some(directory) = roots.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "out") {
                    continue;
                }
                roots.push(path);
            } else if path.extension().is_some_and(|kind| kind == "pl") {
                newest = newest.max(modified(&path));
            }
        }
    }
    newest
}

/// ghcache.dl6 through the Rust emitter, cached under `target/`: the emit costs
/// swipl 3.5s and the fold under test does not change it.
fn emitted_ghcache() -> PathBuf {
    let root = repo_root();
    let cached = engine_dir().join("target/ordered_statement_count/ghcache.rs");
    if cached.is_file() && modified(&cached) > emitter_stamp(&root) {
        return cached;
    }
    std::fs::create_dir_all(cached.parent().expect("a cache directory"))
        .expect("create the emit cache directory");
    let source = root.join("v6/dl/ghcache/ghcache.dl6");
    let goal = format!(
        "compile_dl6('{}','{}',[emitter(emit_rust:emit_program)])",
        source.display(),
        cached.display()
    );
    let started = Instant::now();
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
    println!(
        "ordered_statement_count: emit {:.2}s",
        started.elapsed().as_secs_f64()
    );
    assert!(
        output.status.success() && cached.is_file(),
        "ghcache.dl6 did not compile: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    cached
}

fn schedule(root: &Path) -> Vec<Vec<Arrival>> {
    let text = std::fs::read_to_string(root.join("v6/dl/ghcache/ghcache.schedule.json"))
        .expect("read the ghcache schedule");
    let batches: Vec<Vec<ArrivalDto>> =
        serde_json::from_str(&text).expect("parse the ghcache schedule");
    batches
        .into_iter()
        .map(|batch| arrival_batch(batch).unwrap_or_else(|failure| panic!("{failure}")))
        .collect()
}

struct Tick {
    arrivals: usize,
    statements: u64,
}

/// driver::run_schedule's loop, with the tally read between ticks. The driver
/// itself reports one total for the fold, and the defect is per tick.
fn fold(
    program: &sprefa_engine_rs::program::GenProgram,
    schedule: &[Vec<Arrival>],
) -> (Vec<Tick>, String) {
    let seam = run::open_seam(None).expect("in-memory seam");
    seam.size_statement_cache(program.stable_sql_count() + 64);
    seam.run_program_ddl(&program.ddl, &program.queries)
        .expect("DDL execution failed");
    run_boot(&seam, &program.boot);
    let mut ticks = Vec::new();
    let mut lines = Vec::new();
    let mut tick_number = 0usize;
    let mut carry_pending = false;
    let mut drains_used = 0usize;
    loop {
        let drains = tick_number >= schedule.len();
        let arrivals = match schedule.get(tick_number) {
            Some(batch) => batch.clone(),
            None if carry_pending => Vec::new(),
            None => break,
        };
        assert!(
            !drains || drains_used < DRAIN_CAP,
            "drain overflow after {drains_used} drain ticks"
        );
        let count = arrivals.len();
        let before = SEAM_TALLY.statements.load(Relaxed);
        let deltas = program
            .run_tick(&seam, &arrivals)
            .unwrap_or_else(|failure| panic!("tick {tick_number}: {failure:?}"));
        ticks.push(Tick {
            arrivals: count,
            statements: SEAM_TALLY.statements.load(Relaxed) - before,
        });
        tick_number += 1;
        if drains {
            drains_used += 1;
        }
        carry_pending = deltas.carry_pending;
        lines.push(format_deltas(program, tick_number, &deltas));
    }
    (ticks, lines.join("\n") + "\n")
}

#[test]
fn an_ordered_tick_costs_its_change_not_the_program_size() {
    let root = repo_root();
    let module = emitted_ghcache();
    let program = run::load_program(&module)
        .expect("load the emitted ghcache module")
        .program;
    assert!(
        program.ordered_program,
        "ghcache folds through ordered.rs::run_tick, which is what this test counts"
    );
    let schedule = schedule(&root);

    let started = Instant::now();
    let (ticks, log) = fold(&program, &schedule);
    println!(
        "ordered_statement_count: fold {:.2}s",
        started.elapsed().as_secs_f64()
    );
    for (index, tick) in ticks.iter().enumerate() {
        println!(
            "ordered_statement_count: tick {index} arrivals={} statements={}",
            tick.arrivals, tick.statements
        );
    }

    let expected =
        std::fs::read_to_string(engine_dir().join("tests/fixtures/ghcache_ticklog_base.txt"))
            .expect("read the base tick log");
    assert!(
        log == expected,
        "the tick log moved: {} lines here against {} in the base fixture",
        log.lines().count(),
        expected.lines().count()
    );

    assert_eq!(ticks.len(), 11, "the scripted schedule folds in 11 ticks");
    let over: Vec<String> = ticks
        .iter()
        .enumerate()
        .filter_map(|(index, tick)| {
            let cap = match tick.arrivals {
                0 => ZERO_ARRIVAL_CAP,
                1 => ONE_ARRIVAL_CAP,
                _ => return None,
            };
            (tick.statements > cap).then(|| {
                format!(
                    "tick {index}: {} statements for {} arrival(s), cap {cap}",
                    tick.statements, tick.arrivals
                )
            })
        })
        .collect();
    assert!(over.is_empty(), "{}", over.join("; "));
}
