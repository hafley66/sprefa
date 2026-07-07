//! Errors that name the fix (agent-sharp-edges arc): the "head var not bound"
//! error appends the `$$$` structural-metavar note when the source op is
//! `sg`/`ast_yaml` with a matching `$$$NAME`; the "unbound var in constraint"
//! error appends the put-it-in-the-head note; and the lowercase-`$name` lint
//! warns (exit still 0) through the `--check` diag path. Drives the real binary
//! `--no-daemon` so the in-process render (which includes type diagnostics) is
//! under test.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("authedges_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/x.rs"), "fn alpha() { let y = 1; }\n").unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let mut cmd = Command::new(DL);
    cmd.arg(dir.join("p.dl"))
        .arg("--no-daemon")
        .current_dir(dir)
        .args(extra);
    let out = cmd.output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// A head var bound off a `$$$NAME` multi-metavar is never bound (structure, not
/// a single-node capture): the not-bound error appends the note naming `$NAME`
/// / the span outputs as the fix.
#[test]
fn head_var_from_triple_metavar_names_the_fix() {
    let d = sandbox("triple");
    let db = d.join("db");
    let (code, _out, err) = run(&d, concat!(
        "rel body_hit(b: text, l: int).\n",
        "body_hit(B, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "sg(p, rev, :rust, \"fn $NAME() { $$$B }\", l).\n",
        "? body_hit(b, l).\n",
    ), &["--db", db.to_str().unwrap()]);
    assert_ne!(code, 0, "the unbound head var is a hard error:\n{err}");
    assert!(err.contains("not bound by any source op"), "keeps the base message:\n{err}");
    assert!(err.contains("$$$B is pattern structure only"), "appends the $$$ note:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// A source-rule constraint that tries to COMPUTE a new value on an unbound var
/// (`l2 = l + 1`) appends the put-the-expression-in-the-head note.
#[test]
fn unbound_constraint_var_names_the_head_fix() {
    let d = sandbox("compute");
    let db = d.join("db");
    let (code, _out, err) = run(&d, concat!(
        "rel hit(p: file, l2: int).\n",
        "hit(p, l2) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "match(p, rev, /alpha/, l), l2 = l + 1.\n",
        "? hit(p, l2).\n",
    ), &["--db", db.to_str().unwrap()]);
    assert_ne!(code, 0, "the unbound constraint var is a hard error:\n{err}");
    assert!(err.contains("unbound var l2 in constraint"), "keeps the base message:\n{err}");
    assert!(err.contains("put the expression in the rule head"), "appends the head note:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// The lowercase-`$name` lint warns through the `--check` diag path and, being a
/// warn (not error), leaves the exit at 0.
#[test]
fn lowercase_metavar_warns_under_check() {
    let d = sandbox("check");
    let db = d.join("db");
    let (code, _out, err) = run(&d, concat!(
        "rel body_hit(l: int).\n",
        "body_hit(l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "sg(p, rev, :rust, \"fn $body() {}\", l).\n",
    ), &["--check", "--db", db.to_str().unwrap()]);
    assert!(err.contains("lowercase-metavar"), "warn code surfaces under --check:\n{err}");
    assert!(err.contains("$body is literal text"), "names the offending metavar:\n{err}");
    assert_eq!(code, 0, "warn-only --check exits 0:\n{err}");
    let _ = fs::remove_dir_all(&d);
}
