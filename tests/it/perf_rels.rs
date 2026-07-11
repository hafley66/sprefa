//! The engine-telemetry built-ins `rel_count` / `stmt_ms` (src/rels/perf.rs):
//! tick cardinalities and derived-statement costs as queryable facts, so a
//! perf rail is an ordinary `.dl` rule (examples/perf-rails.dl). Semantics
//! under test: rel_count reports counts at source-phase refresh time (derived
//! rels therefore lag one tick), and stmt_ms projects `_stmt_ms` written by
//! the PREVIOUS rebuild — both populate on the second run over one db.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

const PROG: &str = r#"
rel s(x: text).
s("a").
s("b").

rel d(x: text).
d(x) <- s(x).

? rel_count(rel, rows).
? stmt_ms(rel, ms, n).
"#;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("perf_rels_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), PROG).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .current_dir(dir)
        .args(["--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Second run over the same db: rel_count carries the previous tick's derived
/// counts and stmt_ms carries the previous rebuild's statement costs. (The
/// first run's sections may be empty/partial — that lag is the documented
/// contract, not an accident.)
#[test]
fn telemetry_populates_on_the_second_run() {
    let d = sandbox("second_run");
    let (c1, _o1, e1) = run(&d);
    assert_eq!(c1, 0, "first run clean: {e1}");
    let (c2, o2, e2) = run(&d);
    assert_eq!(c2, 0, "second run clean: {e2}");
    assert!(o2.contains("d\t2"), "derived rel d counted (2 rows):\n{o2}");
    assert!(o2.contains("s\t2"), "fact rel s counted (2 rows):\n{o2}");
    let stmt_sec = o2.split("? stmt_ms").nth(1).expect("stmt_ms section");
    assert!(stmt_sec.lines().any(|l| l.trim_start().starts_with("d\t")),
        "stmt_ms has d's rebuild cost:\n{o2}");
    // The family must not report itself: a rel_count row for rel_count would
    // oscillate the self-diff every refresh.
    assert!(!o2.contains("rel_count\t"), "own family excluded:\n{o2}");
}

/// The names are reserved: a user program declaring rel_count is refused.
#[test]
fn perf_rel_names_are_reserved() {
    let d = sandbox("reserved");
    fs::write(d.join("p.dl"), "rel rel_count(rel: text, rows: int).\n").unwrap();
    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .current_dir(&d)
        .args(["--db", d.join("db").to_str().unwrap()])
        .output().expect("run dl");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_ne!(out.status.code().unwrap_or(-1), 0, "declaration refused");
    assert!(err.contains("reserved-name") && err.contains("relation `rel_count`")
        && err.contains("write to the built-in directly"), "parse-tier reservation/fix:\n{err}");
}
