//! The on-disk run trail: one `extract_run` row plus its `extract_phase` rows
//! in `$HOME/.agent/dl6.db`, and the `--trail` report that reads them back.
#![cfg(feature = "cli")]

use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::Connection;

const BIN: &str = env!("CARGO_BIN_EXE_extract");
const FIXTURE: &str = "tests/fixtures/rust/sample.rs";

/// A fresh HOME per case, so no two cases share a store and none touches the
/// real `~/.agent/dl6.db`.
fn fake_home(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("sprefa-extract-103-{tag}"));
    let _ = std::fs::remove_dir_all(&home);
    home
}

fn db_of(home: &Path) -> PathBuf {
    home.join(".agent/dl6.db")
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
        .expect("count the trail")
}

#[test]
fn bench_writes_one_run_row_and_its_phases() {
    let home = fake_home("written");
    let output = Command::new(BIN)
        .args(["--bench", "--family", "call", FIXTURE])
        .env_remove("RUST_LOG")
        .env_remove("DL_TRAIL")
        .env("HOME", &home)
        .output()
        .expect("run extract");
    assert!(output.status.success(), "extract failed: {output:?}");
    let conn = Connection::open(db_of(&home)).expect("open the trail");
    assert_eq!(count(&conn, "extract_run"), 1, "want exactly one run row");
    assert!(
        count(&conn, "extract_phase") > 0,
        "the run row carries no phase rows"
    );
    let (wall_ms, load_start, argv): (i64, f64, String) = conn
        .query_row(
            "SELECT wall_ms, load_start, argv FROM extract_run",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read the run row");
    assert!(wall_ms >= 0, "wall_ms is not a duration");
    assert!(load_start > 0.0, "no load average beside the timing");
    assert!(argv.contains("--bench"), "argv did not reach the row: {argv}");
    let _ = std::fs::remove_dir_all(&home);
}

// FAIL-FIRST RECEIPT: a trail that ignored its off switch put a row in
// `~/.agent/dl6.db` for every fixture run of the battery.
#[test]
fn dl_trail_zero_writes_nothing() {
    let home = fake_home("off");
    let output = Command::new(BIN)
        .args(["--bench", "--family", "call", FIXTURE])
        .env_remove("RUST_LOG")
        .env("DL_TRAIL", "0")
        .env("HOME", &home)
        .output()
        .expect("run extract");
    assert!(output.status.success(), "extract failed: {output:?}");
    assert!(
        !db_of(&home).exists(),
        "DL_TRAIL=0 still created {}",
        db_of(&home).display()
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn trail_reads_back_the_run_it_wrote() {
    let home = fake_home("read");
    let written = Command::new(BIN)
        .args(["--bench", "--family", "call", FIXTURE])
        .env_remove("RUST_LOG")
        .env_remove("DL_TRAIL")
        .env("HOME", &home)
        .output()
        .expect("run extract");
    assert!(written.status.success(), "extract failed: {written:?}");
    let report = Command::new(BIN)
        .args(["--trail", "1"])
        .env_remove("RUST_LOG")
        .env("HOME", &home)
        .output()
        .expect("run extract --trail");
    assert!(report.status.success(), "--trail failed: {report:?}");
    let stdout = String::from_utf8_lossy(&report.stdout);
    assert!(stdout.starts_with("run 1 "), "no run line in\n{stdout}");
    assert!(stdout.contains("wall "), "no wall column in\n{stdout}");
    assert!(stdout.contains("load "), "no load column in\n{stdout}");
    assert!(
        stdout.lines().any(|line| line.contains("rust")),
        "no rust phase row in\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn trail_on_an_empty_store_says_so() {
    let home = fake_home("empty");
    let output = Command::new(BIN)
        .arg("--trail")
        .env_remove("RUST_LOG")
        .env("HOME", &home)
        .output()
        .expect("run extract --trail");
    assert!(output.status.success(), "--trail failed: {output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "no runs");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn trail_conflicts_with_every_extraction_flag() {
    let output = Command::new(BIN)
        .args(["--trail", "1", FIXTURE])
        .env_remove("RUST_LOG")
        .output()
        .expect("run extract");
    assert!(!output.status.success(), "--trail took a PATH: {output:?}");
}

#[test]
fn two_runs_share_one_store_and_number_in_order() {
    let home = fake_home("twice");
    for _ in 0..2 {
        let output = Command::new(BIN)
            .args(["--bench", "--family", "call", FIXTURE])
            .env_remove("RUST_LOG")
            .env_remove("DL_TRAIL")
            .env("HOME", &home)
            .output()
            .expect("run extract");
        assert!(output.status.success(), "extract failed: {output:?}");
    }
    let conn = Connection::open(db_of(&home)).expect("open the trail");
    assert_eq!(count(&conn, "extract_run"), 2, "want two run rows");
    let orphans: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM extract_phase \
             WHERE run_id NOT IN (SELECT \"__id\" FROM extract_run)",
            [],
            |row| row.get(0),
        )
        .expect("count orphan phase rows");
    assert_eq!(orphans, 0, "a phase row names no run");
    let _ = std::fs::remove_dir_all(&home);
}
