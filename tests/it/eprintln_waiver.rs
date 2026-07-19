//! The eprintln containment ratchet (.dl/no-new-eprintln.dl) as a GATE.
//!
//! The rail's header promises: a comment containing `@eprintln-ok: <reason>`
//! on the hit line or the line above exempts that hit. The trailing form
//! (`eprintln!(...); // @eprintln-ok: reason`) is the dominant shape in the
//! codebase, so the rail must honor it. These fixtures prove the rail BITES
//! on a bare hit and clears on both waiver placements.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
/// The shipped rail, run against a sandbox root.
const RAIL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/.dl/no-new-eprintln.dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eprintln_waiver_{tag}"));
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
        .args(["--no-daemon", "--check"])
        .current_dir(root)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A bare eprintln! in a file with no baseline row warns at the hit line —
/// the rail bites, so the green cases below are not vacuous.
#[test]
fn bare_eprintln_in_new_file_warns() {
    let sandbox_dir = sandbox("bare");
    fs::write(
        sandbox_dir.join("src/fresh.rs"),
        "fn report() {\n    eprintln!(\"boom\");\n}\n",
    )
    .unwrap();
    let (_code, err) = check(&sandbox_dir);
    assert!(
        err.contains("[eprintln-new-file]"),
        "a new-file eprintln must warn; stderr:\n{err}"
    );
}

/// The waiver comment on the line above the hit exempts it.
#[test]
fn waiver_on_line_above_exempts() {
    let sandbox_dir = sandbox("above");
    fs::write(
        sandbox_dir.join("src/fresh.rs"),
        "fn report() {\n    \
         // @eprintln-ok: usage/help text\n    \
         eprintln!(\"usage: tool [ARGS]\");\n}\n",
    )
    .unwrap();
    let (_code, err) = check(&sandbox_dir);
    assert!(
        !err.contains("eprintln-new-file"),
        "a line-above waiver must exempt the hit; stderr:\n{err}"
    );
}

/// The TRAILING waiver form — `eprintln!(...); // @eprintln-ok: reason` on the
/// hit line itself — exempts the hit. This is the placement the rail header
/// documents ("on the hit line") and the shape nearly every live waiver in
/// src/** uses.
#[test]
fn trailing_waiver_on_hit_line_exempts() {
    let sandbox_dir = sandbox("trailing");
    fs::write(
        sandbox_dir.join("src/fresh.rs"),
        "fn report() {\n    \
         eprintln!(\"usage: tool [ARGS]\"); // @eprintln-ok: usage/help text\n}\n",
    )
    .unwrap();
    let (_code, err) = check(&sandbox_dir);
    assert!(
        !err.contains("eprintln-new-file"),
        "a trailing same-line waiver must exempt the hit; stderr:\n{err}"
    );
}

/// A waiver two lines above the hit does NOT reach: the exemption window is
/// exactly the hit line and the line above.
#[test]
fn waiver_two_lines_above_does_not_reach() {
    let sandbox_dir = sandbox("too_far");
    fs::write(
        sandbox_dir.join("src/fresh.rs"),
        "fn report() {\n    \
         // @eprintln-ok: usage/help text\n    \
         let width = 80;\n    \
         eprintln!(\"usage: tool [ARGS]\");\n}\n",
    )
    .unwrap();
    let (_code, err) = check(&sandbox_dir);
    assert!(
        err.contains("[eprintln-new-file]"),
        "a waiver two lines up must not exempt the hit; stderr:\n{err}"
    );
}
