//! Error-reporting usability edges reported from a real session:
//!   1. one failed `?` query must not abort the rest of the query chain
//!   2. a scan rule that matches no files must warn (repo/glob/root), not fail
//!      silently as "0 rows" downstream
//!   3. a bare `//` (C-style comment habit) must give a clear message, not a
//!      baffling `got Regex("")` parse error
//!
//! Runs the real `dl` binary with `--no-daemon` so the in-process path is what's
//! under test (no warm daemon db in the way).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("errrep_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/x.rs"), "fn alpha() {}\n").unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .arg("--no-daemon")
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A first `?` query that fails at evaluation (here: wrong arity on a declared
/// rel) reports its own failure, and the SECOND (valid) query still runs and
/// prints its rows. Before the fix, the `?` eval loop aborted on the first
/// failure, hiding every later answer. (An UNDECLARED rel is a separate case now
/// caught at typecheck — see `undeclared_relation_is_a_clear_error`.)
#[test]
fn failed_query_does_not_abort_the_chain() {
    let d = sandbox("chain");
    let (code, out, err) = run(&d, concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"src/**/*.rs\", p, rev), match(p, rev, /alpha/, l).\n",
        "? hit(z).\n",          // wrong arity (z is not a column, so no shorthand): fails at eval, not typecheck
        "? hit(p, l).\n",
    ));
    assert!(err.contains("query `hit` failed"), "first query reports its failure:\n{err}");
    assert!(out.contains("? hit =>"), "second query still runs:\n{out}");
    assert!(out.contains("src/x.rs"), "second query prints its row:\n{out}");
    assert_eq!(code, 0, "a failed query is not a fatal error:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// An undeclared relation is a clear typecheck error naming the rel, not a raw
/// SQLite `no such table: rel_X` leak from execution.
#[test]
fn undeclared_relation_is_a_clear_error() {
    let d = sandbox("undecl");
    let (code, _out, err) = run(&d, concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"src/**/*.rs\", p, rev), match(p, rev, /alpha/, l).\n",
        "? missingrel(x).\n",
    ));
    assert_ne!(code, 0);
    assert!(err.contains("relation `missingrel`") && err.contains("never declared"),
        "names the undeclared rel:\n{err}");
    assert!(!err.contains("no such table"), "no raw SQLite leak:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// A scan whose glob matches nothing warns with the rule, glob, and where it
/// looked, so the miss is self-diagnosing instead of a silent "0 rows".
#[test]
fn scan_matching_no_files_warns() {
    let d = sandbox("zero");
    let (_code, _out, err) = run(&d, concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"nope/**/*.zzz\", p, rev), match(p, rev, /alpha/, l).\n",
        "? hit(p, l).\n",
    ));
    assert!(err.contains("source `hit` matched 0 files"), "zero-match warning names the rule:\n{err}");
    assert!(err.contains("nope/**/*.zzz"), "warning names the glob:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// Polyglot fan-out: a rel headed by several scans (one per language) where a
/// SIBLING glob matched must not warn about the empty ones — the rel has rows,
/// the empty glob is intentional (this sandbox has `.rs` but no `.zzz`).
#[test]
fn polyglot_sibling_zero_match_is_silent() {
    let d = sandbox("sibling");
    let (_code, _out, err) = run(&d, concat!(
        "rel hit(p: file).\n",
        "hit(p) <- scan(\"src/**/*.rs\", p, rev).\n",
        "hit(p) <- scan(\"nope/**/*.zzz\", p, rev).\n",
        "? hit(p).\n",
    ));
    assert!(!err.contains("matched 0 files"),
        "a sibling glob matched, so the empty polyglot glob stays silent:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// A scan whose rel feeds a downstream rule (consumed) but matched nothing gets
/// the QUIET one-liner, not the loud fix-it note — an empty helper mid-edit is
/// transient, not a glob/root mistake.
#[test]
fn consumed_zero_match_scan_is_quiet_not_loud() {
    let d = sandbox("consumed");
    let (_code, _out, err) = run(&d, concat!(
        "rel helper(p: file).\n",
        "helper(p) <- scan(\"nope/**/*.zzz\", p, rev).\n",
        "rel uses(p: file).\n",
        "uses(p) <- helper(p).\n",
        "? uses(p).\n",
    ));
    assert!(err.contains("matched 0 files this tick"), "consumed helper warns quietly:\n{err}");
    assert!(!err.contains("working root"), "the quiet form drops the fix-it note:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// A scan that DOES match must not spuriously warn.
#[test]
fn scan_with_matches_does_not_warn() {
    let d = sandbox("nowarn");
    let (_code, _out, err) = run(&d, concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"src/**/*.rs\", p, rev), match(p, rev, /alpha/, l).\n",
        "? hit(p, l).\n",
    ));
    assert!(!err.contains("matched 0 files"), "no zero-match warning when files matched:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// A bare `//` gives a clear message pointing at `#`, not `got Regex("")`.
#[test]
fn double_slash_is_a_clear_error() {
    let d = sandbox("slash");
    let (code, _out, err) = run(&d, concat!(
        "rel x(a: int).\n",
        "// C-style comment habit\n",
        "x(1).\n",
        "? x(a).\n",
    ));
    assert!(err.contains("dl comments start with `#`"), "clear comment hint:\n{err}");
    assert!(!err.contains("Regex(\"\")"), "no baffling Regex(\"\") in the message:\n{err}");
    assert_ne!(code, 0, "a broken program is a non-zero exit:\n{err}");
    let _ = fs::remove_dir_all(&d);
}
