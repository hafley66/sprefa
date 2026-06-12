//! The `cmd` op: shell out once per matched file, one row per stdout line.
//! Caching rides the existing source machinery (file content hash + rule-text
//! digest), so an unchanged file with an unchanged command never re-runs: the
//! docker-layer contract. Exit codes follow the diff-tool convention: nonzero
//! WITH stdout = findings (rows bind); nonzero with empty stdout = broken
//! command (loud program error).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cmd_op_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn stdout_lines_become_rows() {
    let d = sandbox("rows");
    fs::write(d.join("a.txt"), "alpha\nbeta\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel row(p: file, l: int, v: text).\n",
        "row(p, l, v) <- scan(\"WORK\", \"*.txt\", p, rev), cmd(p, rev, \"cat {file}\", l, v).\n",
        "? row(p, l, v).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("alpha") && out.contains("beta"), "{out}");
    assert!(out.contains("\t1\t") && out.contains("\t2\t"), "1-based line binds:\n{out}");
}

#[test]
fn nonzero_exit_with_stdout_is_findings() {
    let d = sandbox("findings");
    fs::write(d.join("a.txt"), "x\n").unwrap();
    let (code, out, err) = run(&d, concat!(
        "rel f(p: file, l: int, v: text).\n",
        "f(p, l, v) <- scan(\"WORK\", \"*.txt\", p, rev), cmd(p, rev, \"echo breaking; exit 1\", l, v).\n",
        "? f(p, l, v).\n"));
    assert_eq!(code, 0, "diff-tool convention: exit 1 + stdout is not an error: {err}");
    assert!(out.contains("breaking"), "{out}");
}

#[test]
fn broken_command_is_loud() {
    let d = sandbox("broken");
    fs::write(d.join("a.txt"), "x\n").unwrap();
    let (code, _, err) = run(&d, concat!(
        "rel f(p: file, l: int, v: text).\n",
        "f(p, l, v) <- scan(\"WORK\", \"*.txt\", p, rev), cmd(p, rev, \"no_such_tool_zzz {file}\", l, v).\n",
        "? f(p, l, v).\n"));
    assert_eq!(code, 1, "exit-127 with no stdout must be a program error:\n{err}");
    assert!(err.contains("no_such_tool_zzz"), "{err}");
}

/// `--cmd-budget N`: more than N invocations in a tick is a loud error naming
/// the command, never a silent truncation. Under budget runs clean.
#[test]
fn cmd_budget_caps_invocations() {
    let d = sandbox("budget");
    fs::write(d.join("a.txt"), "x\n").unwrap();
    fs::write(d.join("b.txt"), "y\n").unwrap();
    fs::write(d.join("c.txt"), "z\n").unwrap();
    let prog = concat!(
        "rel f(p: file, l: int, v: text).\n",
        "f(p, l, v) <- scan(\"WORK\", \"*.txt\", p, rev), cmd(p, rev, \"cat {file}\", l, v).\n",
        "? f(p, l, v).\n");
    fs::write(d.join("p.dl"), prog).unwrap();
    let over = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", d.to_str().unwrap(), "--db", d.join("db1").to_str().unwrap(),
               "--cmd-budget", "2"])
        .output().expect("run dl");
    assert_eq!(over.status.code(), Some(1), "3 files > budget 2 must fail");
    let err = String::from_utf8_lossy(&over.stderr);
    assert!(err.contains("cmd budget exceeded"), "{err}");
    assert!(err.contains("cat {file}"), "diag names the command: {err}");
    let under = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", d.to_str().unwrap(), "--db", d.join("db2").to_str().unwrap(),
               "--cmd-budget", "3"])
        .output().expect("run dl");
    assert_eq!(under.status.code(), Some(0), "3 files at budget 3 runs clean: {}",
        String::from_utf8_lossy(&under.stderr));
}

/// Warm db + unchanged file + unchanged command: the second run must not
/// re-execute the command. Observable via a side-effect counter file the
/// command appends to.
#[test]
fn unchanged_inputs_skip_the_command() {
    let d = sandbox("cache");
    fs::write(d.join("a.txt"), "x\n").unwrap();
    let counter = d.join("count.log");
    let prog = format!(concat!(
        "rel f(p: file, l: int, v: text).\n",
        "f(p, l, v) <- scan(\"WORK\", \"a.txt\", p, rev), ",
        "cmd(p, rev, \"echo ran >> {} && cat {{file}}\", l, v).\n",
        "? f(p, l, v).\n"), counter.display());
    let (code, _, err) = run(&d, &prog);
    assert_eq!(code, 0, "{err}");
    let (code, _, err) = run(&d, &prog);
    assert_eq!(code, 0, "{err}");
    let runs = fs::read_to_string(&counter).unwrap().lines().count();
    assert_eq!(runs, 1, "command must run once across two warm ticks");
}
