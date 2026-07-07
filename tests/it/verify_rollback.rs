//! christmas #14: `--verify <cmd>` is a transactional codemod. The program's
//! `gen` edits apply, then `<cmd>` runs as a checker in the root; the edits are
//! kept only if it exits 0, else every touched file is restored to its pre-run
//! bytes and dl exits 1. Apply, test, keep-if-pass.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

const PROG: &str = r#"
rel hit(p: file, x: text, id: text).
hit(p, X, id) <- scan("WORK","src/**/*.rs",p,rev),
  sg(p, rev, :rust, "$X.unwrap()", _l, _c, _el, _ec, id).
gen(:replace, p, lo, hi, "{x}.expect(\"checked\")") <- hit(p, x, id), ref(id, _s, _f, lo, hi).
"#;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("verify_rollback_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.rs"), "let a = foo.unwrap();\n").unwrap();
    fs::write(dir.join("p.dl"), PROG).unwrap();
    dir
}

fn run_verify(dir: &std::path::Path, checker: &str) -> std::process::Output {
    Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(),
               "--no-daemon", "--verify", checker])
        .current_dir(dir)
        .output().expect("run dl")
}

#[test]
fn checker_fail_rolls_back_and_exits_1() {
    let d = sandbox("fail");
    let out = run_verify(&d, "false");
    assert_eq!(out.status.code(), Some(1), "rollback exits 1: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(fs::read_to_string(d.join("src/a.rs")).unwrap(), "let a = foo.unwrap();\n",
        "file restored to pre-run bytes on checker failure");
}

#[test]
fn checker_pass_keeps_and_exits_0() {
    let d = sandbox("pass");
    let out = run_verify(&d, "true");
    assert!(out.status.success(), "keep exits 0: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(fs::read_to_string(d.join("src/a.rs")).unwrap(), "let a = foo.expect(\"checked\");\n",
        "edit kept on checker success");
}
