//! A `_` head slot (explicit, or named-arg padding) lowers to SQL NULL, and a
//! NULL row never dedups in the fixpoint delta (NULL != NULL under INSERT OR
//! IGNORE) — a recursive rule with a padded head re-inserts the same row every
//! iteration and never converges (measured: 2^24 rows before kill). The engine
//! must refuse it, both at typecheck (`recursive-null-pad`, --check/LSP) and at
//! tick time (`rebuild_derived` bail). Sink use (diag-style, non-recursive)
//! stays legal.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("recursive_null_pad_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str], prog: &str) -> (i32, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const HANG: &str = r#"
rel seed(a: text).
rel n(a: text, b: text).
seed("s").
n(x, x) <- seed(x).
n(a: y) <- n(y, _).
? n(a, b).
"#;

#[test]
fn recursive_named_pad_head_bails_instead_of_hanging() {
    let d = sandbox("tick");
    let (code, stderr) = run(&d, &[], HANG);
    assert_ne!(code, 0, "expected a non-zero exit; stderr: {stderr}");
    assert!(
        stderr.contains("NULL") && stderr.contains('n'),
        "expected the NULL-pad bail naming `n`; got: {stderr}"
    );
}

#[test]
fn recursive_null_pad_flagged_by_check() {
    let d = sandbox("check");
    let (code, stderr) = run(&d, &["--check"], HANG);
    assert_ne!(code, 0, "expected --check to fail; stderr: {stderr}");
    assert!(
        stderr.contains("recursive-null-pad"),
        "expected the recursive-null-pad diag; got: {stderr}"
    );
}

#[test]
fn recursive_explicit_wild_head_bails_too() {
    let d = sandbox("wild");
    let prog = r#"
rel seed(a: text).
rel n(a: text, b: text).
seed("s").
n(x, x) <- seed(x).
n(y, _) <- n(y, _).
? n(a, b).
"#;
    let (code, stderr) = run(&d, &[], prog);
    assert_ne!(code, 0, "expected a non-zero exit; stderr: {stderr}");
    assert!(
        stderr.contains("NULL"),
        "expected the NULL-pad bail; got: {stderr}"
    );
}

#[test]
fn non_recursive_padded_sink_head_still_works() {
    let d = sandbox("sink");
    // The sanctioned diag shape: a padded head on a non-recursive rel.
    let prog = r#"
rel seed(a: text).
rel n(a: text, b: text).
seed("s").
n(a: x) <- seed(x).
? n(a, b).
"#;
    let (code, stderr) = run(&d, &[], prog);
    assert_eq!(code, 0, "padded sink head should succeed; stderr: {stderr}");
}

#[test]
fn recursive_rule_without_padding_still_converges() {
    let d = sandbox("control");
    let prog = r#"
rel seed(a: text).
rel n(a: text, b: text).
seed("s").
n(x, x) <- seed(x).
n(y, "k") <- n(y, _).
? n(a, b).
"#;
    let (code, stderr) = run(&d, &[], prog);
    assert_eq!(
        code, 0,
        "plain recursive rule should converge; stderr: {stderr}"
    );
}
