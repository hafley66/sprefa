//! Large-repo e2e perf stress. Proves the full pipeline (scan + extract + the
//! type/call/dataflow graphs + closure) runs to completion on a real large
//! checkout, and reports the cold-vs-incremental ratio — the headline perf
//! number. `#[ignore]`d — the fixture is a gitignored, opt-in shallow clone;
//! provision one with `just v5-fixture-rust` (shallow-clones rust-analyzer,
//! depth 1). Override the fixture path with SPREFA_BENCH_ROOT.
//!
//! What this answers that closure_incremental_bench does not: does the engine
//! hold up on a real polyglot corpus with messy imports, real call graphs, and
//! multi-thousand-file scan breadth — not just a synthetic chain of N fns.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const PROG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/stress.dl");

/// The fixture root: SPREFA_BENCH_ROOT if set, else the default clone location.
/// None -> skip the test (the clone is opt-in; CI lacks it).
fn fixture() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_BENCH_ROOT") {
        let p = PathBuf::from(p);
        if p.is_dir() { return Some(p); }
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/.fixtures/rust-analyzer");
    p.is_dir().then_some(p)
}

/// Count `.rs` files under root (the fixture-size report). Cheap recursive walk.
fn count_rs(root: &Path) -> usize {
    let mut n = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() { stack.push(p); }
            else if p.extension().is_some_and(|x| x == "rs") { n += 1; }
        }
    }
    n
}

/// First repo-relative `.rs` path under root, for the `--changed` incremental run.
fn first_rs(root: &Path) -> Option<String> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() { stack.push(p); }
            else if p.extension().is_some_and(|x| x == "rs") {
                return p.strip_prefix(root).ok()?.to_str().map(String::from);
            }
        }
    }
    None
}

/// Pull a parsed-file count out of stderr from an `[extract]` line shaped
/// like `[extract] type: REBUILD (first-run) — 1483 files parsed, 729.2ms`.
/// (The old `[tick] files N/N parsed` line was retired in the tracing
/// conversion; the `[extract]` lines are the surviving stderr contract.)
fn parsed_count(stderr: &str) -> Option<usize> {
    let line = stderr
        .lines()
        .find(|l| l.contains("[extract]") && l.contains("files parsed"))?;
    let before_marker = line.split("files parsed").next()?;
    before_marker.split_whitespace().last()?.parse().ok()
}

#[test]
#[ignore = "needs tests/.fixtures/rust-analyzer corpus (provision with `just v5-fixture-rust`, or set SPREFA_BENCH_ROOT)"]
fn full_pipeline_cold_and_incremental_on_large_repo() {
    let root = fixture().expect(
        "needs tests/.fixtures/rust-analyzer corpus (provision with `just v5-fixture-rust`, or set SPREFA_BENCH_ROOT)");
    let rs = count_rs(&root);
    assert!(rs > 0, "fixture {root:?} has no .rs files");
    let db = std::env::temp_dir().join("sprefa_perf_stress.db");
    let _ = fs::remove_file(&db);

    eprintln!("perf_stress: fixture {root:?} ({rs} .rs files)");

    // Cold: full scan + extract + graph build + closure. The heavy pass.
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
    let parsed = parsed_count(&cold_stderr).unwrap_or(0);
    assert!(parsed > 0, "cold tick parsed no files:\n{cold_stderr}");
    eprintln!("perf_stress: cold {cold_ms:.0}ms ({parsed} files parsed)\n{cold_stderr}");

    // Incremental: one --changed path against the warm db. Should be a small
    // fraction of cold (the corpus did not move; only the one path re-runs).
    let Some(target) = first_rs(&root) else { return };
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
        "perf_stress: incremental {incr_ms:.0}ms (--changed {target})  cold/incr ratio {ratio:.1}x\n{}",
        String::from_utf8_lossy(&incr.stderr));
}
