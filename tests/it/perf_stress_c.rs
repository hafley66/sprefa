//! C-focused e2e perf stress. Proves the scan + sg extract path runs to
//! completion on a real large C checkout (redis/redis), and reports the
//! cold-vs-incremental ratio — the headline perf number. Skips (does not fail)
//! when no fixture is present; provision one with `just v5-fixture-c`
//! (shallow-clones redis, depth 1, gitignored). Override the fixture path with
//! DL_STRESS_C.
//!
//! C has no type/call/dataflow graph support (rust/kotlin/ts only), so this
//! isolates the source-extraction cost at corpus scale — the complement of
//! perf_stress, which exercises the graph + closure paths over rust.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const PROG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/stress_c.dl");

/// The fixture root: DL_STRESS_C if set, else the default clone location.
/// None -> skip the test (the clone is opt-in; CI lacks it).
fn fixture() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("DL_STRESS_C") {
        let p = PathBuf::from(p);
        if p.is_dir() { return Some(p); }
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/.fixtures/redis");
    p.is_dir().then_some(p)
}

/// Count `.c` files under root (the fixture-size report). Cheap recursive walk.
fn count_c(root: &Path) -> usize {
    let mut n = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() { stack.push(p); }
            else if p.extension().is_some_and(|x| x == "c") { n += 1; }
        }
    }
    n
}

/// First repo-relative `.c` path under root, for the `--changed` incremental run.
fn first_c(root: &Path) -> Option<String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() { stack.push(p); }
            else if p.extension().is_some_and(|x| x == "c") {
                return p.strip_prefix(root).ok()?.to_str().map(String::from);
            }
        }
    }
    None
}

/// Count the `? seen` query's rows on stdout — one per scanned file. The C
/// program's scan+sg families never print an `[extract] … files parsed`
/// stderr line (that line belongs to the type/call/dataflow graph families,
/// which C lacks), and the old `[tick] files N/N parsed` line was retired in
/// the tracing conversion, so the query output is the work receipt here.
fn scanned_count(stdout: &str) -> usize {
    stdout
        .lines()
        .skip_while(|line| !line.starts_with("? seen"))
        .skip(1)
        .take_while(|line| !line.starts_with("? "))
        .filter(|line| !line.trim().is_empty())
        .count()
}

#[test]
fn c_stress_cold_and_incremental_on_large_repo() {
    let Some(root) = fixture() else {
        eprintln!("perf_stress_c: SKIPPED — no fixture. Run `just v5-fixture-c` to provision.");
        return;
    };
    let c = count_c(&root);
    assert!(c > 0, "fixture {root:?} has no .c files");
    let db = std::env::temp_dir().join("sprefa_perf_stress_c.db");
    let _ = fs::remove_file(&db);

    eprintln!("perf_stress_c: fixture {root:?} ({c} .c files)");

    // Cold: full scan + extract. The heavy pass.
    let t = Instant::now();
    let cold = Command::new(DL)
        .arg(PROG)
        .arg("--db").arg(&db)
        .current_dir(&root)
        .output().expect("run dl");
    let cold_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert!(cold.status.success(), "cold tick failed:\n{}",
        String::from_utf8_lossy(&cold.stderr));
    let cold_stderr = String::from_utf8_lossy(&cold.stderr);
    let scanned = scanned_count(&String::from_utf8_lossy(&cold.stdout));
    assert!(scanned > 0, "cold tick scanned no files:\n{cold_stderr}");
    eprintln!("perf_stress_c: cold {cold_ms:.0}ms ({scanned} files scanned)\n{cold_stderr}");

    // Incremental: one --changed path against the warm db. Should be a small
    // fraction of cold (the corpus did not move; only the one path re-runs).
    let Some(target) = first_c(&root) else { return };
    let t = Instant::now();
    let incr = Command::new(DL)
        .arg(PROG)
        .arg("--db").arg(&db)
        .arg("--changed").arg(&target)
        .current_dir(&root)
        .output().expect("run dl --changed");
    let incr_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert!(incr.status.success(), "incremental tick failed:\n{}",
        String::from_utf8_lossy(&incr.stderr));
    let ratio = if incr_ms > 0.0 { cold_ms / incr_ms } else { f64::INFINITY };
    eprintln!(
        "perf_stress_c: incremental {incr_ms:.0}ms (--changed {target})  cold/incr ratio {ratio:.1}x\n{}",
        String::from_utf8_lossy(&incr.stderr));

    // The headline assertion: incremental must beat cold (the corpus did not
    // move; only the one --changed path re-runs).
    assert!(incr_ms < cold_ms,
        "incremental ({incr_ms:.0}ms) not faster than cold ({cold_ms:.0}ms)");
}
