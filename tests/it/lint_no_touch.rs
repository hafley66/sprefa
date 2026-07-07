//! No-touch guard (examples/lint-no-touch.dl): an agent turn that edits inside a
//! protected range (a sprefa autogen `BEGIN/END` block, or a hand-placed
//! `NO-TOUCH` comment marker in any language) becomes an `error` diag. The diag
//! is the three-way join agent_touch ⨝ changed_line ⨝ no_touch_zone — the agent
//! touched the file this turn AND a moved line lands inside the range.
//!
//! Fixture mirrors tests/it/agent_rels.rs: a Claude Code JSONL store (via the
//! SPREFA_CLAUDE_PROJECTS override) whose latest turn edits the doc, plus a git
//! repo whose worktree moves one line inside an autogen block and one inside a
//! manual NO-TOUCH block. A non-agent edit (clean store) must stay silent.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const PROG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/lint-no-touch.dl");

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// Claude Code's project-dir slug: abs path, every non-alphanumeric byte -> '-'.
fn cc_slug(p: &Path) -> String {
    p.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// A repo with a doc carrying an autogen block (lines 3-7) and a manual NO-TOUCH
/// block (lines 9-12). Returns (repo_root, store_root). The store's latest turn
/// edits doc.md. The worktree moves a line inside EACH block plus one free line.
fn fixture(tag: &str) -> (PathBuf, PathBuf) {
    let d = fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("notouch_{tag}"));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    git(&d, &["init", "-q"]);
    git(&d, &["config", "user.email", "t@t"]);
    git(&d, &["config", "user.name", "t"]);
    git(&d, &["config", "commit.gpgsign", "false"]);
    // 1 # Title
    // 2 intro
    // 3 <!-- BEGIN: demo -->
    // 4 | a | b |
    // 5 |---|---|
    // 6 | 1 | 2 |
    // 7 <!-- END: demo -->
    // 8 free prose line
    // 9 <!-- NO-TOUCH: hand tuned -->
    // 10 protected one
    // 11 protected two
    // 12 <!-- END-NO-TOUCH -->
    let base = "# Title\nintro\n<!-- BEGIN: demo -->\n| a | b |\n|---|---|\n| 1 | 2 |\n<!-- END: demo -->\nfree prose line\n<!-- NO-TOUCH: hand tuned -->\nprotected one\nprotected two\n<!-- END-NO-TOUCH -->\n";
    fs::write(d.join("doc.md"), base).unwrap();
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-qm", "base"]);
    // worktree edits: line 6 (inside autogen), line 10 (inside manual), line 8 (free).
    let edited = "# Title\nintro\n<!-- BEGIN: demo -->\n| a | b |\n|---|---|\n| 9 | 9 |\n<!-- END: demo -->\nFREE prose changed\n<!-- NO-TOUCH: hand tuned -->\nprotected ONE\nprotected two\n<!-- END-NO-TOUCH -->\n";
    fs::write(d.join("doc.md"), edited).unwrap();

    // Claude Code store: latest turn edits doc.md.
    let store = fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("notouch_store_{tag}"));
    let _ = fs::remove_dir_all(&store);
    let proj = store.join(cc_slug(&d));
    fs::create_dir_all(&proj).unwrap();
    let fp = format!("{}/doc.md", d.to_string_lossy());
    let asst = format!(
        r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"tool_use","name":"Edit","input":{{"file_path":"{fp}"}}}}]}}}}"#);
    let mut jsonl = String::new();
    jsonl.push_str("{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"go\"}]}}\n");
    jsonl.push_str(&asst);
    jsonl.push('\n');
    fs::write(proj.join("sess1.jsonl"), jsonl).unwrap();
    (d, store)
}

fn run(dir: &Path, store: Option<&Path>) -> String {
    let mut cmd = Command::new(DL);
    cmd.arg(PROG)
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir);
    if let Some(s) = store { cmd.env("SPREFA_CLAUDE_PROJECTS", s); }
    let out = cmd.output().expect("run dl");
    assert!(out.status.success(), "dl failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn diag_block(stdout: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut in_q = false;
    for line in stdout.lines() {
        if line.starts_with("? diag") { in_q = true; continue; }
        if line.starts_with('?') { in_q = false; continue; }
        if in_q {
            let t = line.trim();
            if t.is_empty() || t.starts_with('(') { continue; }
            rows.push(t.split('\t').map(|s| s.to_string()).collect());
        }
    }
    rows
}

#[test]
fn agent_edit_inside_protected_ranges_diags_both_kinds() {
    let (d, store) = fixture("hit");
    let out = run(&d, Some(&store));

    // both zone kinds are detected as facts.
    assert!(out.contains("autogen"), "autogen zone fact missing:\n{out}");
    assert!(out.contains("manual"), "manual zone fact missing:\n{out}");

    let diags = diag_block(&out);
    // line 6 (autogen block) edited by the agent -> diag.
    assert!(diags.iter().any(|r| r.get(1).map(|s| s == "6").unwrap_or(false)),
        "edited autogen line 6 must diag: {diags:?}\n{out}");
    // line 10 (manual NO-TOUCH block) edited by the agent -> diag.
    assert!(diags.iter().any(|r| r.get(1).map(|s| s == "10").unwrap_or(false)),
        "edited manual no-touch line 10 must diag: {diags:?}\n{out}");
    // line 8 is a free edit, outside any protected range -> no diag.
    assert!(!diags.iter().any(|r| r.get(1).map(|s| s == "8").unwrap_or(false)),
        "free line 8 is outside any zone, no diag: {diags:?}");
}

#[test]
fn no_agent_store_means_no_diag() {
    let (d, _store) = fixture("silent");
    let empty = std::env::temp_dir().join("notouch_store_none");
    let _ = fs::create_dir_all(&empty);
    // same worktree edits, but no agent touched the file (empty store) -> silent.
    let out = run(&d, Some(&empty));
    assert!(diag_block(&out).is_empty(), "no agent => no no-touch diag:\n{out}");
}
