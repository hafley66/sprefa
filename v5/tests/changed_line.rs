//! The built-in `changed_line(path, line)` line-level worktree-diff relation:
//! rows are the new-side line numbers of every `git diff -U0 HEAD` hunk (the
//! lines that exist in the worktree and differ from HEAD), plus every line of
//! untracked files (which the diff omits). The rails use case tightens a
//! path-scoped `changed(p)` rail to line scope: a touch on a big file surfaces
//! only the hits on edited lines, not every hit in the file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("changed_line_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .args(extra)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repo with one committed-clean file, one committed-then-appended file (line
/// 1 unchanged, a new line 2 added), and one untracked file. `git diff -U0 HEAD`
/// for the append is `@@ -1,0 +2 @@`, so only line 2 of edited.rs is a row; a
/// pure deletion emits `+c,0` and contributes nothing. The .dl program and db
/// land in the worktree too, so the diff also contains them; assertions check
/// membership, not exact sets.
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
    // append: line 1 stays, line 2 is new -> hunk `@@ -1,0 +2 @@` -> line 2 only.
    fs::write(d.join("src/edited.rs"), "fn b() {}\nfn b2() {}\n").unwrap();
    fs::write(d.join("src/new.rs"), "fn c() {}\n").unwrap();
    d
}

/// (1) `? changed_line(p, l).` lists the appended line and every untracked line,
/// and neither the clean file nor the unchanged first line of the edited file.
#[test]
fn changed_line_lists_edited_lines_only() {
    let d = fixture("list");
    let (code, out, err) = run(&d, "? changed_line(p, l).\n", &[]);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(out.contains("src/edited.rs\t2"), "appended line 2 must appear:\n{out}");
    assert!(out.contains("src/new.rs\t1"), "untracked file's line must appear:\n{out}");
    assert!(!out.contains("src/clean.rs"), "clean file must not appear:\n{out}");
    // precision: line 1 of edited.rs is unchanged even though the file is touched
    assert!(!out.contains("src/edited.rs\t1"), "unchanged line 1 must not appear:\n{out}");
}

/// (2) The rails shape tightened to line scope: a /fn / hit on every line joined
/// with `changed_line(p, l)` fires only on the edited line and the untracked
/// file, NOT on edited.rs line 1 (unchanged) or the clean file. This is the
/// path-scope noise fix: the same rail over `changed(p)` would fire line 1 too.
#[test]
fn rail_joined_with_changed_line_scopes_to_the_edited_line() {
    let d = fixture("rail");
    let prog = concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), match(p, rev, /fn /, l).\n",
        "rel diag(path: text, line: int, severity: text, code: text, msg: text).\n",
        "diag(p, l, \"error\", \"rail\", \"hit on changed line\") <- hit(p, l), changed_line(p, l).\n",
    );
    let (code, out, _err) = run(&d, &format!("{prog}? diag(p, l, s, c, m).\n"), &[]);
    assert_eq!(code, 0);
    assert!(out.contains("src/edited.rs\t2"), "the appended line must trip the rail:\n{out}");
    assert!(out.contains("src/new.rs\t1"), "the untracked file must trip the rail:\n{out}");
    assert!(!out.contains("src/clean.rs"), "clean file must not trip the rail:\n{out}");
    // the precision win: edited.rs is touched but its unchanged line 1 stays silent
    assert!(!out.contains("src/edited.rs\t1"),
        "line-scope rail must skip the unchanged line 1 of a touched file:\n{out}");
}

/// (3) Outside a git repo the relation is empty (no error).
#[test]
fn changed_line_is_empty_outside_git() {
    let d = sandbox("nogit");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn a() {}\n").unwrap();
    let (code, out, err) = run(&d, "? changed_line(p, l).\n", &[]);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(!out.contains("src/x.rs"), "no rows outside a repo:\n{out}");
}

/// (4) `changed_line` is a reserved name.
#[test]
fn changed_line_is_reserved() {
    let d = sandbox("reserved");
    let (code, _out, err) = run(&d, "rel changed_line(p: text, l: int).\n", &[]);
    assert_ne!(code, 0);
    assert!(err.contains("built-in"), "reserved-name error expected:\n{err}");
}
