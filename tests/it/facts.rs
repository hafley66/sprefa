//! Ground facts: a rule with no body (`slide(1, "intro").`) inserts its
//! literal row. Editing a fact on a warm db must follow the program text
//! (covered by the derived-layer digest, the derived twin of rule_edit.rs).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("facts_{tag}"));
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
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn ground_facts_insert_and_join() {
    let d = sandbox("basic");
    let (code, out, err) = run(&d, concat!(
        "rel slide(n: int, title: text).\n",
        "rel cue(n: int).\n",
        "slide(1, \"intro\").\n",
        "slide(2, \"the tick\").\n",
        "cue(2).\n",
        "rel current(t: text).\n",
        "current(t) <- slide(n, t), cue(n).\n",
        "? current(t).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("the tick"), "fact join must select slide 2:\n{out}");
    assert!(!out.contains("intro"), "uncued slide must not appear:\n{out}");
}

#[test]
fn edited_fact_follows_program_on_warm_db() {
    let d = sandbox("edit");
    let prog = |title: &str| format!(
        "rel slide(n: int, title: text).\nslide(1, \"{title}\").\n? slide(n, t).\n");
    let (code, out, err) = run(&d, &prog("v1"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("v1"));

    let (code, out, err) = run(&d, &prog("v2"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("v2"), "edited fact must replace the row:\n{out}");
    assert!(!out.contains("v1"), "stale fact row survived the edit:\n{out}");
}

#[test]
fn fact_head_rejects_unbound_vars() {
    let d = sandbox("unbound");
    let (code, _, err) = run(&d, concat!(
        "rel slide(n: int, title: text).\n",
        "slide(1, t).\n",
        "? slide(n, t).\n"));
    assert_eq!(code, 1, "unbound fact var is a program error:\n{err}");
    assert!(err.contains("unbound"), "{err}");
}
