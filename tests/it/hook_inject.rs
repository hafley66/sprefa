//! `dl --hook`: the harness-hook emit path. The program heads
//! `inject_skill(name)` / `block(reason)` over the agent built-ins; dl reads a
//! Claude Code PostToolUse event on stdin and emits the hook JSON
//! (additionalContext / decision:block) on stdout. "Load once" is declarative:
//! the rule negates the built-in `skill_loaded(harness, session, name)` relation
//! (derived from the transcript — explicit `Skill` calls plus dl's own prior
//! injection markers), so it never double-injects. No state files.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn cc_slug(p: &Path) -> String {
    p.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// A git repo with `src/foo_test.rs`, a `testing` skill, and a Claude Code
/// transcript store. Returns (root, store). The transcript is written per-test.
fn fixture(tag: &str) -> (PathBuf, PathBuf) {
    let root = fs::canonicalize({
        let d = std::env::temp_dir().join(format!("hookinj_{tag}"));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("src")).unwrap();
        fs::create_dir_all(d.join(".claude/skills/testing")).unwrap();
        d
    }).unwrap();
    let git = |args: &[&str]| {
        assert!(Command::new("git").arg("-C").arg(&root).args(args).output().unwrap().status.success());
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    fs::write(root.join("src/foo_test.rs"), "fn t(){}\n").unwrap();
    fs::write(root.join(".claude/skills/testing/SKILL.md"), "# Testing\n\nRun cargo test.\n").unwrap();

    let store = std::env::temp_dir().join(format!("hookinj_store_{tag}"));
    let _ = fs::remove_dir_all(&store);
    fs::create_dir_all(store.join(cc_slug(&root))).unwrap();
    (root, store)
}

/// One assistant turn that edits the test file (so agent_touch sees it).
fn edit_turn(root: &Path) -> String {
    format!(
        r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"tool_use","name":"Edit","input":{{"file_path":"{}/src/foo_test.rs"}}}}]}}}}"#,
        root.to_string_lossy())
}

/// Write the session transcript from a set of jsonl lines.
fn write_transcript(store: &Path, root: &Path, lines: &[String]) {
    let proj = store.join(cc_slug(root));
    let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
    fs::write(proj.join("sess1.jsonl"), body).unwrap();
}

/// Run `dl --hook PROG` with cwd set to ROOT, feeding `event` on stdin. Returns stdout.
fn run_hook(root: &Path, store: &Path, prog_path: &Path, event: &str) -> String {
    let mut child = Command::new(DL)
        .arg("--hook").arg(prog_path)
        .current_dir(root)
        .args(["--db", root.join("db").to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_CLAUDE_PROJECTS", store)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const INJECT_PROG: &str = concat!(
    "rel inject_skill(name: text).\n",
    "inject_skill(\"testing\") <- agent_touch(_, s, p), p =~ /_test\\./, !skill_loaded(_, s, \"testing\").\n",
);

const EVENT: &str =
    r#"{"session_id":"sess1","tool_name":"Read","tool_input":{"file_path":"src/foo_test.rs"}}"#;

#[test]
fn injects_skill_body_when_unloaded() {
    let (root, store) = fixture("inject");
    let prog = root.join("p.dl");
    fs::write(&prog, INJECT_PROG).unwrap();
    write_transcript(&store, &root, &[edit_turn(&root)]);

    let out = run_hook(&root, &store, &prog, EVENT);
    assert!(out.contains("additionalContext"), "expected an inject:\n{out}");
    assert!(out.contains("Run cargo test"), "skill body should be injected:\n{out}");
}

#[test]
fn explicit_skill_call_suppresses_inject() {
    let (root, store) = fixture("dedup_call");
    let prog = root.join("p.dl");
    fs::write(&prog, INJECT_PROG).unwrap();
    // The transcript already shows an explicit Skill("testing") load this session.
    let skill = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Skill","input":{"skill":"testing"}}]}}"#;
    write_transcript(&store, &root, &[edit_turn(&root), skill.to_string()]);

    let out = run_hook(&root, &store, &prog, EVENT);
    assert!(out.trim().is_empty(), "explicit Skill load should suppress inject:\n{out}");
}

#[test]
fn own_injection_marker_suppresses_reinject() {
    let (root, store) = fixture("dedup_marker");
    let prog = root.join("p.dl");
    fs::write(&prog, INJECT_PROG).unwrap();
    // A prior `dl --hook` injection: its additionalContext marker is in the transcript.
    let marker = r##"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"# Skill `testing` (auto-loaded by dl --hook)"}]}}"##;
    write_transcript(&store, &root, &[edit_turn(&root), marker.to_string()]);

    let out = run_hook(&root, &store, &prog, EVENT);
    assert!(out.trim().is_empty(), "dl's own prior injection should suppress re-inject:\n{out}");
}

/// End-to-end over the SHIPPED path: `dl setup --project` writes the hook
/// registration + the starter `.dl/hook-skill-on-test.dl` (the committed
/// `changed`-based example), then `dl --hook` with NO program argument
/// discovers that `.dl/` and fires. Proves the bootstrap chain, not just the
/// two halves in isolation.
#[test]
fn setup_bootstrap_then_hook_injects() {
    let (root, store) = fixture("setup_e2e");
    // src/foo_test.rs is an untracked worktree change, so the example's
    // `changed(p)` source sees it (git status -uall). No transcript = empty
    // skill_loaded, so the `!skill_loaded` guard passes.
    write_transcript(&store, &root, &[]);

    // --yes: this test runs non-interactively (piped), and the integrations are
    // TTY-gated; --yes wires them without a prompt.
    let setup_ok = Command::new(DL)
        .args(["setup", "--project", root.to_str().unwrap(), "--yes"])
        .output().unwrap().status.success();
    assert!(setup_ok, "dl setup --project should succeed");
    assert!(root.join(".dl/hook-skill-on-test.dl").exists(), "starter hook .dl written");
    assert!(root.join(".claude/settings.json").exists(), "settings.json written");

    // No program arg: discovery globs <root>/.dl/*.dl (the just-written example).
    let mut child = Command::new(DL)
        .arg("--hook")
        .current_dir(&root)
        .args(["--db", root.join("db").to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_CLAUDE_PROJECTS", &store)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(EVENT.as_bytes()).unwrap();
    let out = String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned();
    assert!(out.contains("additionalContext"), "bootstrapped example should inject:\n{out}");
    assert!(out.contains("Run cargo test"), "testing skill body should be injected:\n{out}");
}

/// Like `run_hook`, with extra dl args (e.g. --dialect).
fn run_hook_args(root: &Path, store: &Path, prog_path: &Path, event: &str, extra: &[&str]) -> String {
    let mut child = Command::new(DL)
        .arg("--hook").arg(prog_path)
        .args(extra)
        .current_dir(root)
        .args(["--db", root.join("db").to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_CLAUDE_PROJECTS", store)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// codex dialect e2e: a canned codex PostToolUse payload (field names from the
/// installed codex 0.144.1 binary's embedded hook schemas) through
/// `--dialect codex` emits the same Claude-shaped hookSpecificOutput.
#[test]
fn codex_dialect_injects_from_codex_shaped_payload() {
    let (root, store) = fixture("codex");
    let prog = root.join("p.dl");
    fs::write(&prog, INJECT_PROG).unwrap();
    write_transcript(&store, &root, &[edit_turn(&root)]);

    let event = r#"{"cwd":"/tmp/x","hook_event_name":"PostToolUse","model":"gpt-5.6","permission_mode":"default","session_id":"sess1","tool_input":{"file_path":"src/foo_test.rs"},"tool_name":"shell","tool_response":{},"tool_use_id":"tu1","transcript_path":null,"turn_id":"turn1"}"#;
    let out = run_hook_args(&root, &store, &prog, event, &["--dialect", "codex"]);
    assert!(out.contains("additionalContext"), "codex dialect should inject:\n{out}");
    assert!(out.contains("\"hookEventName\":\"PostToolUse\""), "echoes the event name:\n{out}");
    assert!(out.contains("Run cargo test"), "skill body injected:\n{out}");
    // Codex's output schemas set additionalProperties:false — nothing beyond
    // hookSpecificOutput may ride the reply.
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 1, "only hookSpecificOutput:\n{out}");
}

/// opencode dialect e2e: the neutral `{kind, session, json}` payload (what the
/// shipped .opencode plugin sends) yields the neutral `{"inject": ...}` reply.
#[test]
fn opencode_dialect_injects_neutral_shapes() {
    let (root, store) = fixture("opencode");
    let prog = root.join("p.dl");
    fs::write(&prog, INJECT_PROG).unwrap();
    write_transcript(&store, &root, &[edit_turn(&root)]);

    let event = r#"{"kind":"PostToolUse","session":"sess1","json":"{}"}"#;
    // No --dialect: the neutral shape auto-detects opencode.
    let out = run_hook_args(&root, &store, &prog, event, &[]);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert!(v["inject"].as_str().unwrap().contains("Run cargo test"),
        "neutral inject reply expected:\n{out}");
    assert!(v.get("hookSpecificOutput").is_none(), "no claude framing for opencode:\n{out}");
}

/// opencode block: neutral `{"block": reason}`.
#[test]
fn opencode_dialect_block_is_neutral() {
    let (root, store) = fixture("ocblock");
    let prog = root.join("b.dl");
    fs::write(&prog, concat!(
        "rel block(reason: text).\n",
        "block(\"no hand edits\") <- agent_touch(_, _, touched_path), touched_path =~ /_test\\./.\n",
    )).unwrap();
    write_transcript(&store, &root, &[edit_turn(&root)]);

    let event = r#"{"kind":"PreToolUse","session":"sess1","json":"{}"}"#;
    let out = run_hook_args(&root, &store, &prog, event, &["--dialect", "opencode"]);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["block"], "no hand edits", "neutral block reply expected:\n{out}");
}

/// A skill living under `.agents/skills/` (the cross-harness home, no
/// `.claude/skills` copy) resolves through inject_skill.
#[test]
fn skill_resolves_from_agents_skills_dir() {
    let (root, store) = fixture("agentsdir");
    // Move the skill out of .claude/skills into .agents/skills.
    fs::create_dir_all(root.join(".agents/skills/testing")).unwrap();
    fs::rename(
        root.join(".claude/skills/testing/SKILL.md"),
        root.join(".agents/skills/testing/SKILL.md"),
    ).unwrap();
    let prog = root.join("p.dl");
    fs::write(&prog, INJECT_PROG).unwrap();
    write_transcript(&store, &root, &[edit_turn(&root)]);

    let out = run_hook(&root, &store, &prog, EVENT);
    assert!(out.contains("Run cargo test"),
        ".agents/skills SKILL.md should resolve:\n{out}");
}

#[test]
fn block_relation_emits_decision_block() {
    let (root, store) = fixture("block");
    let prog = root.join("b.dl");
    fs::write(&prog, concat!(
        "rel block(reason: text).\n",
        "block(\"no hand edits\") <- agent_touch(_, _, p), p =~ /_test\\./.\n",
    )).unwrap();
    write_transcript(&store, &root, &[edit_turn(&root)]);

    let out = run_hook(&root, &store, &prog, EVENT);
    assert!(out.contains("\"decision\":\"block\""), "expected a block decision:\n{out}");
    assert!(out.contains("no hand edits"), "block reason should carry:\n{out}");
}
