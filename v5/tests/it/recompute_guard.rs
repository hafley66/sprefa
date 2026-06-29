//! The static recompute-guard rail (examples/recompute-guard.dl) as a GATE.
//!
//! A Rust fn that calls the global primitive `embed_graph` (a from-scratch
//! recompute) must early-out on an unchanged digest (`load_rel_digest`) or carry
//! a `// @recompute unguarded: <reason>` waiver. The rail turns an unguarded
//! recompute into a `--check` exit-2 error. These fixtures prove it BITES on the
//! unguarded shape and clears on either escape hatch, so the green run against
//! the real engine source is not vacuous.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
/// The shipped rail, run against a sandbox root.
const RAIL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/recompute-guard.dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("recompute_guard_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

/// Run the rail with `--check` against `root`; return (exit_code, stderr).
/// `--no-daemon` forces the in-process path so the test never attaches to (or
/// spawns) a background daemon.
fn check(root: &Path) -> (i32, String) {
    let out = Command::new(DL)
        .arg(RAIL)
        .args(["--root", root.to_str().unwrap(), "--no-daemon", "--check"])
        .output()
        .expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn unguarded_recompute_fails() {
    let d = sandbox("unguarded");
    fs::write(d.join("src/r.rs"),
        "fn recompute_thing() {\n    let pool = embed_graph(&edges, &cfg);\n    pool\n}\n").unwrap();
    let (code, err) = check(&d);
    assert_eq!(code, 2, "an unguarded recompute must fail --check; stderr:\n{err}");
    assert!(err.contains("unguarded-recompute"),
        "diag should name the rail code; stderr:\n{err}");
}

#[test]
fn digest_guard_passes() {
    let d = sandbox("guarded");
    fs::write(d.join("src/r.rs"),
        "fn recompute_thing() {\n    \
         if self.load_rel_digest(&key)? == Some(d) { return Ok(()); }\n    \
         let pool = embed_graph(&edges, &cfg);\n}\n").unwrap();
    let (code, err) = check(&d);
    assert_eq!(code, 0, "a fn with a load_rel_digest skip must pass; stderr:\n{err}");
}

#[test]
fn waiver_comment_passes() {
    let d = sandbox("waived");
    fs::write(d.join("src/r.rs"),
        "fn recompute_thing() {\n    \
         // @recompute unguarded: one-shot CLI, never reactive\n    \
         let pool = embed_graph(&edges, &cfg);\n}\n").unwrap();
    let (code, err) = check(&d);
    assert_eq!(code, 0, "a waived recompute must pass; stderr:\n{err}");
}

/// The primitive's own definition (`fn embed_graph`) is not a call site, so a
/// file that only DEFINES it (and never calls it unguarded) is clean.
#[test]
fn primitive_definition_is_not_a_call() {
    let d = sandbox("def_only");
    fs::write(d.join("src/r.rs"),
        "pub fn embed_graph(edges: &[(String, String)]) -> Vec<f32> {\n    vec![]\n}\n").unwrap();
    let (code, err) = check(&d);
    assert_eq!(code, 0, "defining embed_graph is not an unguarded call; stderr:\n{err}");
}
