//! The 10-second law at the SQL seam. FAIL PRE FIX: with no progress handler
//! the runaway CTE below spins until killed; the seam had no way to stop a
//! statement once stepped (rehome.dl6's 187x187x40x40 cross product ran at
//! 100% CPU for five minutes before a coordinator SIGKILL, 2026-08-24).

use std::time::{Duration, Instant};

use sprefa_engine_rs::sql::{statement_budget_exceeded, SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::SqlStatement;

const RUNAWAY: &str = "WITH RECURSIVE spin(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM spin) \
                       SELECT count(*) FROM spin";

#[test]
fn a_statement_past_its_budget_is_aborted_and_named() {
    let seam = SqliteSeam::in_memory().expect("seam");
    seam.set_statement_budget(Duration::from_millis(200));
    let started = Instant::now();
    let error = seam.scalar(RUNAWAY).expect_err("the runaway CTE must be cut");
    let wall = started.elapsed();
    assert!(statement_budget_exceeded(&error), "not the valve: {error}");
    let text = error.to_string();
    assert!(text.contains("200 ms budget"), "budget unnamed: {text}");
    assert!(text.contains("DL_STATEMENT_BUDGET_MS"), "override unnamed: {text}");
    assert!(
        wall < Duration::from_secs(5),
        "the valve fired late: {wall:?}"
    );
}

#[test]
fn the_row_path_is_cut_too() {
    let seam = SqliteSeam::in_memory().expect("seam");
    seam.set_statement_budget(Duration::from_millis(200));
    let statement = SqlStatement {
        sql: RUNAWAY.to_string(),
        args: Vec::new(),
    };
    let error = seam.execute(&statement).expect_err("cut");
    assert!(statement_budget_exceeded(&error), "not the valve: {error}");
}

#[test]
fn a_statement_inside_its_budget_is_untouched() {
    let seam = SqliteSeam::in_memory().expect("seam");
    seam.set_statement_budget(Duration::from_millis(200));
    let bounded = "WITH RECURSIVE spin(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM spin LIMIT 50000) \
                   SELECT count(*) FROM spin";
    assert_eq!(seam.scalar(bounded).expect("bounded"), 50_000);
    assert_eq!(seam.statement_budget(), Duration::from_millis(200));
}

#[test]
fn the_default_budget_is_ten_seconds() {
    let seam = SqliteSeam::in_memory().expect("seam");
    assert_eq!(seam.statement_budget(), Duration::from_secs(10));
}
