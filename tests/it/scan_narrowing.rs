//! Empty-scan narrowing (docs/arch-measures-review.md sharp edge; the
//! 618->68-file smashy incident): a program with SOME scan rules, run against
//! an existing db, silently reconciles the db down to its own (smaller) scan
//! scope. `tick.rs`'s no-scan-rules guard (see `tests/it/empty_scan_scope_guard.rs`)
//! only covers the case of ZERO scan rules; this covers the SOME-scan-rules
//! case beside it — `reconcile_sources` (src/engine/reconcile.rs) now warns
//! (`scan-narrowing`, routed to the `commit` stage by default) when the files
//! falling outside this tick's scan scope exceed a threshold share
//! (`DL_SCAN_NARROW_THRESHOLD`, default 0.5) of the db's previously known files.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("scan_narrowing_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    for file_index in 0..10 {
        fs::write(
            dir.join(format!("src/module_{file_index}.rs")),
            "fn present() {}\n",
        )
        .unwrap();
    }
    dir
}

const SOURCE_REL_DECL: &str = "rel scanned_source_file(source_path: file).\n";

fn source_rule(glob: &str) -> String {
    format!("{SOURCE_REL_DECL}scanned_source_file(source_path) <- scan(\"WORK\", \"{glob}\", source_path, rev).\n")
}

/// `dl <prog> --check --no-daemon [--stage S]`, hermetic and daemon-free.
/// Reuses `dir.join("db")` across calls so a second invocation reconciles
/// against the first tick's already-scanned file set.
fn check(
    dir: &Path,
    prog: &str,
    stage: Option<&str>,
    extra_env: &[(&str, &str)],
) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let mut cmd = Command::new(DL);
    cmd.arg(dir.join("p.dl"))
        .args([
            "--db",
            dir.join("db").to_str().unwrap(),
            "--check",
            "--no-daemon",
        ])
        .env("SPREFA_CONFIG", dir.join("no.toml"))
        .current_dir(dir);
    if let Some(stage_name) = stage {
        cmd.args(["--stage", stage_name]);
    }
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("run dl --check");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// (1) FAIL-PRE-FIX: tick 1 scans all 10 files; tick 2 (same db) narrows the
/// scan glob down to a single file — 9 of 10 (90%) fall outside the new scope,
/// past the default 50% threshold. `scan-narrowing` fires at the `commit`
/// stage (the --check default) with before/after counts.
#[test]
fn narrowing_past_threshold_warns_with_counts() {
    let sandbox_dir = sandbox("past_threshold");
    let (code1, _out1, err1) = check(&sandbox_dir, &source_rule("src/**/*.rs"), None, &[]);
    assert_eq!(code1, 0, "first tick must scan cleanly:\n{err1}");
    assert!(
        !err1.contains("scan-narrowing"),
        "first tick has no prior state to narrow from:\n{err1}"
    );

    let (code2, _out2, err2) = check(&sandbox_dir, &source_rule("src/module_0.rs"), None, &[]);
    assert_eq!(code2, 0, "a warning must not fail --check:\n{err2}");
    assert!(
        err2.contains("scan-narrowing"),
        "expected the scan-narrowing code:\n{err2}"
    );
    assert!(
        err2.contains("1 of the 10 file"),
        "expected the before/after counts:\n{err2}"
    );
    assert!(
        err2.contains("9") && err2.contains("90%"),
        "expected the out-of-scope count and share:\n{err2}"
    );
    assert!(
        err2.contains("fresh --db") || err2.contains("broaden"),
        "message must name the reconcile-semantics escape hatch:\n{err2}"
    );
}

/// (2) Negative control: narrowing that stays UNDER the default 50% threshold
/// (4 of 10 files, 40%, fall outside scope) stays silent.
#[test]
fn narrowing_under_threshold_is_silent() {
    let sandbox_dir = sandbox("under_threshold");
    let (code1, _out1, err1) = check(&sandbox_dir, &source_rule("src/**/*.rs"), None, &[]);
    assert_eq!(code1, 0, "{err1}");

    // Keep 6 of 10 files in scope (4 fall out, 40% < the 50% default).
    let glob = "src/{module_0,module_1,module_2,module_3,module_4,module_5}.rs";
    let (code2, _out2, err2) = check(&sandbox_dir, &source_rule(glob), None, &[]);
    assert_eq!(code2, 0, "{err2}");
    assert!(
        !err2.contains("scan-narrowing"),
        "under-threshold narrowing must stay silent:\n{err2}"
    );
}

/// (3) `DL_SCAN_NARROW_THRESHOLD` is env-tunable: the same 40% narrowing that
/// stayed silent at the default threshold fires once the threshold is lowered
/// below 40%.
#[test]
fn narrow_threshold_is_env_tunable() {
    let sandbox_dir = sandbox("env_tunable");
    let (code1, _out1, err1) = check(&sandbox_dir, &source_rule("src/**/*.rs"), None, &[]);
    assert_eq!(code1, 0, "{err1}");

    let glob = "src/{module_0,module_1,module_2,module_3,module_4,module_5}.rs";
    let (code2, _out2, err2) = check(
        &sandbox_dir,
        &source_rule(glob),
        None,
        &[("DL_SCAN_NARROW_THRESHOLD", "0.3")],
    );
    assert_eq!(code2, 0, "{err2}");
    assert!(
        err2.contains("scan-narrowing"),
        "a lowered threshold must catch the same 40% narrowing:\n{err2}"
    );
}

/// (4) Stage routing: `scan-narrowing` is an unrouted warning, so it defaults
/// to the `commit` stage only (R7's severity default) — it must NOT surface at
/// `--stage live`, matching every other unrouted warning in this engine.
#[test]
fn narrowing_warning_is_commit_stage_only() {
    let sandbox_dir = sandbox("stage_only");
    let (code1, _out1, err1) = check(&sandbox_dir, &source_rule("src/**/*.rs"), None, &[]);
    assert_eq!(code1, 0, "{err1}");

    let (code2, _out2, err2) = check(
        &sandbox_dir,
        &source_rule("src/module_0.rs"),
        Some("live"),
        &[],
    );
    assert_eq!(code2, 0, "{err2}");
    assert!(
        !err2.contains("scan-narrowing"),
        "an unrouted warning must not surface at the live stage:\n{err2}"
    );
}
