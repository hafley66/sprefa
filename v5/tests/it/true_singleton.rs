//! The built-in `true()` singleton: always holds exactly one zero-column row.
//! Its purpose is the range anchor for negation-only rules, so a body of
//! `true(), !rel(_,_)` fires iff `rel` is empty — "if empty, derive this."

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("true_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const PROG: &str = concat!(
    "rel hit(p: file).\n",
    "\n",
    "hit(p) <- scan(\"WORK\", \"*.rs\", p, rev), match(p, rev, /fn \\w+/, line).\n",
    "\n",
    "diag(path: \"p.dl\", line: 1, severity: \"info\", code: \"empty-match\", msg: \"hit matched 0 rows\") <- true(), !hit(_).\n",
    "\n",
    "? diag(path, line, severity, code, msg).\n",
);

#[test]
fn fires_when_source_rel_is_empty() {
    let d = sandbox("empty");
    let (code, out, err) = run(&d, PROG);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("empty-match"),
        "diag must fire when hit has 0 rows:\n{out}"
    );
}

#[test]
fn silent_when_source_rel_has_rows() {
    let d = sandbox("nonempty");
    fs::write(d.join("a.rs"), "fn foo() {}\n").unwrap();
    let (code, out, err) = run(&d, PROG);
    assert_eq!(code, 0, "{err}");
    assert!(
        !out.contains("empty-match"),
        "diag must NOT fire when hit has rows:\n{out}"
    );
    assert!(
        out.contains("(0 rows)"),
        "query should report 0 diag rows:\n{out}"
    );
}
