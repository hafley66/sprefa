//! Arithmetic inside body atoms (2026-07-15 fix). `idx + 1` in a positive or
//! negated body atom lowers as `column = expr`: the expr references vars bound
//! by earlier body items, the same binding-only discipline a literal arg
//! follows. Was a parse error ("got Plus") before parse.rs and lower.rs were
//! relaxed to mirror head/comparison arithmetic. Same harness as body_binds.rs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arith_in_body_{tag}"));
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

/// Negated body atom with arithmetic: `!step(url, idx + 1)` keeps rows whose
/// successor index does not exist.
#[test]
fn negated_body_atom_arithmetic() {
    let dir = sandbox("neg");
    let prog = concat!(
        "rel step(url: text, idx: int).\n",
        "step(\"a\", 1).\n",
        "step(\"a\", 2).\n",
        "step(\"a\", 3).\n",
        "step(\"b\", 5).\n",
        "step(\"b\", 6).\n",
        "rel gap(url: text, idx: int).\n",
        "gap(url, idx) <- step(url, idx), !step(url, idx + 1).\n",
        "? gap(url, idx).\n"
    );
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "negated body-atom arithmetic must run:\n{err}");
    assert!(out.contains("a\t3"), "a,3 has no successor:\n{out}");
    assert!(out.contains("b\t6"), "b,6 has no successor:\n{out}");
    assert!(
        !out.contains("a\t1") && !out.contains("a\t2"),
        "a,1 and a,2 have successors:\n{out}"
    );
    assert!(!out.contains("b\t5"), "b,5 has a successor:\n{out}");
    assert!(out.contains("(2 rows)"), "exactly two gap rows:\n{out}");
}

/// Positive body atom with arithmetic: `next(url, idx + 1)` joins a base row to
/// a next row whose idx is one greater.
#[test]
fn positive_body_atom_arithmetic() {
    let dir = sandbox("pos");
    let prog = concat!(
        "rel base(url: text, idx: int).\n",
        "base(\"a\", 1).\n",
        "base(\"a\", 2).\n",
        "base(\"a\", 3).\n",
        "base(\"b\", 5).\n",
        "base(\"b\", 6).\n",
        "rel next(url: text, idx: int).\n",
        "next(\"a\", 2).\n",
        "next(\"a\", 3).\n",
        "next(\"b\", 6).\n",
        "rel follow(url: text, idx: int).\n",
        "follow(url, idx) <- base(url, idx), next(url, idx + 1).\n",
        "? follow(url, idx).\n"
    );
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "positive body-atom arithmetic must run:\n{err}");
    assert!(out.contains("a\t1"), "a,1 matched by next a,2:\n{out}");
    assert!(out.contains("a\t2"), "a,2 matched by next a,3:\n{out}");
    assert!(out.contains("b\t5"), "b,5 matched by next b,6:\n{out}");
    assert!(!out.contains("a\t3"), "a,3 has no next a,4:\n{out}");
    assert!(!out.contains("b\t6"), "b,6 has no next b,7:\n{out}");
    assert!(
        out.contains("(3 rows)"),
        "exactly three follow rows:\n{out}"
    );
}
