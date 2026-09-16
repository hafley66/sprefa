//! The built-in `changed(path)` worktree-diff relation: rows are the paths
//! `git status --porcelain -uall` reports against HEAD (modified, added,
//! untracked), re-anchored to the scan root. The rails use case is a derived
//! rule joining a per-file hit relation with `changed(p)`, so a `--check` only
//! fires on what the current edit session touched.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("changed_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .args(extra)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repo with one committed-clean file, one committed-then-modified file, and
/// one untracked file. The `.dl` program and db land in the worktree too, so
/// the diff also contains them; assertions check membership, not exact sets.
fn fixture(tag: &str) -> PathBuf {
    let d = sandbox(tag);
    git(&d, &["init", "-q"]);
    git(&d, &["config", "user.email", "t@t"]);
    git(&d, &["config", "user.name", "t"]);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/clean.rs"), "fn a() {}\n").unwrap();
    fs::write(d.join("src/edited.rs"), "fn b() {}\n").unwrap();
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-qm", "base"]);
    fs::write(d.join("src/edited.rs"), "fn b() {}\nfn b2() {}\n").unwrap();
    fs::write(d.join("src/new.rs"), "fn c() {}\n").unwrap();
    d
}

/// (1) `? changed(p).` lists modified + untracked paths and not the clean one.
#[test]
fn changed_lists_worktree_diff() {
    let d = fixture("list");
    let (code, out, err) = run(&d, "? changed(p).\n", &[]);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(
        out.contains("src/edited.rs"),
        "modified file missing:\n{out}"
    );
    assert!(out.contains("src/new.rs"), "untracked file missing:\n{out}");
    assert!(
        !out.contains("src/clean.rs"),
        "clean file must not appear:\n{out}"
    );
}

/// (2) The rails shape: a per-file hit relation joined with `changed(p)` only
/// diags the edited file, even though the clean file has the same content hit.
#[test]
fn rail_joined_with_changed_skips_clean_files() {
    let d = fixture("rail");
    // both clean.rs and edited.rs contain `fn `, but only edited/new are in the diff
    let prog = concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), match(p, rev, /fn /, l).\n",
        "diag(path: p, line: l, severity: \"error\", code: \"rail\", msg: \"hit in changed file\") <- hit(p, l), changed(p).\n",
    );
    let (code, out, _err) = run(
        &d,
        &format!("{prog}? diag(path: p, line: l, severity: s, code: c, msg: m).\n"),
        &[],
    );
    assert_eq!(code, 0);
    assert!(
        out.contains("src/edited.rs"),
        "edited file must trip the rail:\n{out}"
    );
    assert!(
        out.contains("src/new.rs"),
        "untracked file must trip the rail:\n{out}"
    );
    assert!(
        !out.contains("src/clean.rs"),
        "clean file must not trip the rail:\n{out}"
    );
}

/// (3) Outside a git repo the relation is empty (no error), so rails programs
/// degrade to silent on non-repo roots.
#[test]
fn changed_is_empty_outside_git() {
    let d = sandbox("nogit");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    let (code, out, err) = run(&d, "? changed(p).\n", &[]);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(!out.contains("src/x.rs"), "no rows outside a repo:\n{out}");
}

/// (4) `changed` is a reserved name.
#[test]
fn changed_is_reserved() {
    let d = sandbox("reserved");
    let (code, _out, err) = run(&d, "rel changed(p: text).\n", &[]);
    assert_ne!(code, 0);
    assert!(
        err.contains("built-in"),
        "reserved-name error expected:\n{err}"
    );
}
