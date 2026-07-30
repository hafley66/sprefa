
use super::super::{bootstrap_project, wire_repo_skills_j, SetupJournal};
use super::*;
use serde_json::Value;

fn read_settings(dir: &Path) -> Value {
    let txt = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
    serde_json::from_str(&txt).unwrap()
}

fn post_cmds(v: &Value) -> Vec<String> {
    v["hooks"]["PostToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|e| e["hooks"].as_array().unwrap().iter())
        .map(|h| h["command"].as_str().unwrap().to_string())
        .collect()
}

fn ups_cmds(v: &Value) -> Vec<String> {
    v["hooks"]["UserPromptSubmit"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|e| e["hooks"].as_array().unwrap().iter())
        .map(|h| h["command"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn hook_wired_fresh_then_idempotent() {
    let dir = std::env::temp_dir().join(format!("dlsetup_fresh_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut journal = SetupJournal::load().unwrap();
    wire_claude_hook(&mut journal, &dir);
    let v = read_settings(&dir);
    assert_eq!(post_cmds(&v), vec!["dl --hook".to_string()]);
    // Both events register; UserPromptSubmit has no matcher (whole prompt).
    assert_eq!(ups_cmds(&v), vec!["dl --hook".to_string()]);
    assert!(
        v["hooks"]["UserPromptSubmit"][0].get("matcher").is_none(),
        "UserPromptSubmit hook has no tool matcher"
    );
    assert_eq!(
        v["hooks"]["PostToolUse"][0]["matcher"],
        "Read|Edit|Write|MultiEdit"
    );
    // Second call must not add a duplicate under either event.
    wire_claude_hook(&mut journal, &dir);
    let v = read_settings(&dir);
    assert_eq!(post_cmds(&v), vec!["dl --hook".to_string()]);
    assert_eq!(ups_cmds(&v), vec!["dl --hook".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

fn git_init(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("init")
        .arg("-q")
        .status()
        .unwrap()
        .success());
}

#[test]
fn bootstrap_noninteractive_skips_integrations() {
    // Under `cargo test`, stdin is not a TTY -> is_tty() is false. Without
    // --yes, base scaffolding lands but the integrations do not.
    let dir = std::env::temp_dir().join(format!("dlsetup_noint_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    git_init(&dir);
    assert_eq!(bootstrap_project(&dir, false).unwrap(), 0);
    assert!(
        dir.join(".dl/dl-self-lint.dl").exists(),
        "base scaffolding written"
    );
    assert!(
        !dir.join(".claude/settings.json").exists(),
        "CC hook skipped (non-tty)"
    );
    assert!(
        !dir.join(".githooks/pre-commit").exists(),
        "git hook skipped (non-tty)"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bootstrap_assume_yes_wires_integrations() {
    std::env::set_var("SPREFA_SETUP_NO_VSCODE", "1");
    let dir = std::env::temp_dir().join(format!("dlsetup_yes_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    git_init(&dir);
    // --yes wires without prompting even off a TTY (VSCode install no-ops
    // without `code`, but must not fail the bootstrap).
    assert_eq!(bootstrap_project(&dir, true).unwrap(), 0);
    assert!(
        dir.join(".claude/settings.json").exists(),
        "CC hook wired (--yes)"
    );
    assert!(
        dir.join(".githooks/pre-commit").exists(),
        "git hook wired (--yes)"
    );
    let _ = std::fs::remove_dir_all(&dir);
    std::env::remove_var("SPREFA_SETUP_NO_VSCODE");
}

#[test]
fn git_hook_written_and_hooks_path_set() {
    let dir = std::env::temp_dir().join(format!("dlsetup_git_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .arg("init")
        .arg("-q")
        .status()
        .unwrap()
        .success());
    let mut journal = SetupJournal::load().unwrap();
    wire_git_hook(&mut journal, &dir);
    let hook = dir.join(".githooks/pre-commit");
    assert!(hook.exists(), "pre-commit hook written");
    let body = std::fs::read_to_string(&hook).unwrap();
    assert!(body.contains("dl --check"), "hook runs dl --check");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            hook.metadata().unwrap().permissions().mode() & 0o111,
            0,
            "executable"
        );
    }
    let cfg = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["config", "core.hooksPath"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&cfg.stdout).trim(), ".githooks");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn codex_hooks_json_fresh_then_idempotent() {
    let dir = std::env::temp_dir().join(format!("dlsetup_codex_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut journal = SetupJournal::load().unwrap();
    wire_codex_hook(&mut journal, &dir);
    let read = |dir: &Path| -> Value {
        serde_json::from_str(&std::fs::read_to_string(dir.join(".codex/hooks.json")).unwrap())
            .unwrap()
    };
    let v = read(&dir);
    assert_eq!(
        v["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
        "dl --hook --dialect codex"
    );
    assert_eq!(
        v["hooks"]["PostToolUse"][0]["matcher"],
        "Read|Edit|Write|MultiEdit"
    );
    assert_eq!(
        v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
        "dl --hook --dialect codex"
    );
    assert!(v["hooks"]["UserPromptSubmit"][0].get("matcher").is_none());
    // Second run: no duplicates, byte-identical file.
    let before = std::fs::read_to_string(dir.join(".codex/hooks.json")).unwrap();
    wire_codex_hook(&mut journal, &dir);
    let after = std::fs::read_to_string(dir.join(".codex/hooks.json")).unwrap();
    assert_eq!(
        before, after,
        "second wire must not change .codex/hooks.json"
    );
    assert_eq!(
        read(&dir)["hooks"]["PostToolUse"].as_array().unwrap().len(),
        1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opencode_plugin_written_then_idempotent_and_preserves_modified() {
    let dir = std::env::temp_dir().join(format!("dlsetup_oc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut journal = SetupJournal::load().unwrap();
    wire_opencode_plugin(&mut journal, &dir);
    let dest = dir.join(".opencode/plugins/dl.js");
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), OPENCODE_PLUGIN_JS);
    // Idempotent second run.
    wire_opencode_plugin(&mut journal, &dir);
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), OPENCODE_PLUGIN_JS);
    // A modified plugin is user content and is never replaced.
    std::fs::write(&dest, "// old plugin\n").unwrap();
    wire_opencode_plugin(&mut journal, &dir);
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "// old plugin\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn repo_skills_links_agents_skills_dir_idempotently() {
    let dir = std::env::temp_dir().join(format!("dlsetup_agsk_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".agents/skills/my-repo-skill")).unwrap();
    std::fs::write(
        dir.join(".agents/skills/my-repo-skill/SKILL.md"),
        "# My repo skill\n",
    )
    .unwrap();
    let mut journal = SetupJournal::load().unwrap();
    wire_repo_skills_j(&mut journal, &dir);
    journal.save().unwrap();
    let link = dir.join(".claude/skills/my-repo-skill/SKILL.md");
    assert_eq!(
        std::fs::read_to_string(&link).unwrap(),
        "# My repo skill\n",
        "the .claude shim must resolve to the .agents source"
    );
    #[cfg(unix)]
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    // Second run leaves it alone.
    let mut journal = SetupJournal::load().unwrap();
    wire_repo_skills_j(&mut journal, &dir);
    journal.save().unwrap();
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "# My repo skill\n");
    let _ = std::fs::remove_dir_all(&dir);
}
