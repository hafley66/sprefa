//! Built-in agent-harness relations `agent_edit(harness, session, idx, path)` and
//! `agent_touch(harness, session, path)`: the files the latest agent turn edited,
//! read from the harness's at-rest session store (here Claude Code JSONL via the
//! SPREFA_CLAUDE_PROJECTS override), repo-relative and session-tagged. The rails
//! shape is `diag(p,...) <- changed(p), agent_touch(_, _, p).` — the worktree
//! change the latest turn produced.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("agentrel_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    // canonicalize: the engine canonicalizes --root before matching the harness
    // store, so the slug and the transcript file_paths must use the real path.
    fs::canonicalize(&dir).unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// Claude Code's project-dir slug: abs path, every non-alphanumeric byte -> '-'.
fn cc_slug(p: &Path) -> String {
    p.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

fn line(rec: &str) -> String { format!("{rec}\n") }

/// repo: a/b/c committed, a+b modified in the worktree. store: a Claude Code
/// transcript whose older turn edits b, latest turn edits a + c.
fn fixture(tag: &str) -> (PathBuf, PathBuf) {
    let d = sandbox(tag);
    git(&d, &["init", "-q"]);
    git(&d, &["config", "user.email", "t@t"]);
    git(&d, &["config", "user.name", "t"]);
    for f in ["a.txt", "b.txt", "c.txt"] { fs::write(d.join(f), "v0\n").unwrap(); }
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-qm", "base"]);
    fs::write(d.join("a.txt"), "v0\nv1\n").unwrap();
    fs::write(d.join("b.txt"), "v0\nv1\n").unwrap();

    let store = std::env::temp_dir().join(format!("agentrel_store_{tag}"));
    let _ = fs::remove_dir_all(&store);
    let proj = store.join(cc_slug(&d));
    fs::create_dir_all(&proj).unwrap();
    let ds = d.to_string_lossy();
    let edit = |name: &str, fp: &str| format!(
        r#"{{"type":"tool_use","name":"{name}","input":{{"file_path":"{fp}"}}}}"#);
    let asst = |parts: &str| format!(
        r#"{{"type":"assistant","message":{{"role":"assistant","content":[{parts}]}}}}"#);
    let mut jsonl = String::new();
    jsonl.push_str(&line(r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"go"}]}}"#));
    jsonl.push_str(&line(&asst(&edit("Edit", &format!("{ds}/b.txt")))));            // older turn
    jsonl.push_str(&line(&asst(&format!("{},{}",
        edit("Edit", &format!("{ds}/a.txt")), edit("Write", &format!("{ds}/c.txt")))))); // latest turn
    fs::write(proj.join("sess1.jsonl"), jsonl).unwrap();
    (d, store)
}

fn run(dir: &Path, store: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .env("SPREFA_CLAUDE_PROJECTS", store)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// agent_edit is fully tagged; agent_touch is the latest turn; diag is the
/// intersection with the worktree diff (a.txt only: b is older, c is clean).
#[test]
fn agent_touch_intersects_changed_and_stays_session_tagged() {
    let (d, store) = fixture("hit");
    let prog = concat!(
        "diag(path: p, line: 0, col: 0, end_col: 0, severity: \"info\", code: \"latest-turn-change\", msg: \"latest turn\") <- changed(p), agent_touch(_, _, p).\n",
        "? agent_edit(h, s, i, p).\n",
        "? agent_touch(h, s, p).\n",
        "? diag(path: p, line: l, col: c, end_col: e, severity: sev, code, msg).\n",
    );
    let (code, out, err) = run(&d, &store, prog);
    assert_eq!(code, 0, "stderr: {err}");
    // session-tagged edits, all three present
    assert!(out.contains("claude-code"), "harness tag missing:\n{out}");
    assert!(out.contains("sess1"), "session tag missing:\n{out}");
    // latest turn = a.txt + c.txt; b.txt is an older turn so not in agent_touch
    let touch = out.split("agent_touch =>").nth(1).unwrap_or("").split("diag =>").next().unwrap_or("");
    assert!(touch.contains("a.txt"), "a.txt should be in latest touch:\n{out}");
    assert!(touch.contains("c.txt"), "c.txt should be in latest touch:\n{out}");
    assert!(!touch.contains("b.txt"), "b.txt is an older turn, not latest:\n{out}");
    // diag = changed {a,b} INTERSECT touch {a,c} = {a}
    let diag = out.split("diag =>").nth(1).unwrap_or("");
    assert!(diag.contains("a.txt"), "a.txt must diag (changed + latest):\n{out}");
    assert!(!diag.contains("c.txt"), "c.txt is latest but clean, no diag:\n{out}");
    assert!(!diag.contains("b.txt"), "b.txt is changed but older, no diag:\n{out}");
}

/// Empty harness store => no rows, no error (rails degrade silent).
#[test]
fn agent_rels_empty_without_a_store() {
    let (d, _store) = fixture("empty");
    let empty = std::env::temp_dir().join("agentrel_store_none");
    let _ = fs::create_dir_all(&empty);
    let (code, out, err) = run(&d, &empty, "? agent_touch(h, s, p).\n");
    assert_eq!(code, 0, "stderr: {err}");
    assert!(!out.contains(".txt"), "no rows without a store:\n{out}");
}

/// agent_edit / agent_touch are reserved names.
#[test]
fn agent_rels_are_reserved() {
    let (d, store) = fixture("reserved");
    let (code, _out, err) = run(&d, &store, "rel agent_touch(p: text).\n");
    assert_ne!(code, 0);
    assert!(err.contains("built-in"), "reserved-name error expected:\n{err}");
}
