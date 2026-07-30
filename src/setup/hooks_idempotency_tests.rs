use super::super::{
    append_section_j, wire_repo_skills_j, write_starter_j, SetupJournal, STARTER_DL, STARTER_HOOK,
};
use super::*;
use serde_json::Value;

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

fn read_settings(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap())
        .unwrap()
}

fn post_cmds(value: &Value) -> Vec<String> {
    value["hooks"]["PostToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|entry| entry["hooks"].as_array().unwrap())
        .map(|hook| hook["command"].as_str().unwrap().to_string())
        .collect()
}

/// The full setup wiring run twice -> identical tree (paths + bytes). Runs
/// the same sequence bootstrap_project wires, minus the VSCode extension
/// arm (install_vscode_extension mutates the machine's `code` extension
/// state, not the project tree — the tree identity claim is unaffected).
#[test]
fn setup_wiring_twice_produces_identical_tree() {
    let dir = std::env::temp_dir().join(format!("dlsetup_idem_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    git_init(&dir);
    let wire_all = |dir: &Path| {
        let mut journal = SetupJournal::load().unwrap();
        let dl_dir = dir.join(".dl");
        write_starter_j(
            &mut journal,
            dir,
            &dl_dir.join("dl-self-lint.dl"),
            STARTER_DL,
        )
        .unwrap();
        write_starter_j(
            &mut journal,
            dir,
            &dl_dir.join("hook-skill-on-test.dl"),
            STARTER_HOOK,
        )
        .unwrap();
        append_section_j(&mut journal, dir, &dir.join("AGENTS.md")).unwrap();
        append_section_j(&mut journal, dir, &dir.join("CLAUDE.md")).unwrap();
        wire_repo_skills_j(&mut journal, dir);
        journal.save().unwrap();
        wire_claude_hook(&mut journal, dir);
        wire_codex_hook(&mut journal, dir);
        wire_opencode_plugin(&mut journal, dir);
        wire_git_hook(&mut journal, dir);
    };
    wire_all(&dir);
    let snapshot = |dir: &Path| {
        let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let path = e.path();
                if path.file_name().is_some_and(|n| n == ".git") {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path
                        .strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    entries.push((rel, std::fs::read(&path).unwrap_or_default()));
                }
            }
        }
        entries.sort();
        entries
    };
    let first = snapshot(&dir);
    assert!(first.iter().any(|(p, _)| p == ".codex/hooks.json"));
    assert!(first.iter().any(|(p, _)| p == ".opencode/plugins/dl.js"));
    assert!(first.iter().any(|(p, _)| p == ".claude/settings.json"));
    wire_all(&dir);
    let second = snapshot(&dir);
    assert_eq!(
        first, second,
        "second setup wiring must not change the tree"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hook_merges_preserving_other_keys() {
    let dir = std::env::temp_dir().join(format!("dlsetup_merge_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".claude")).unwrap();
    std::fs::write(dir.join(".claude/settings.json"), r#"{
            "permissions": {"allow": ["Bash(ls:*)"]},
            "hooks": {"PreToolUse": [{"matcher":"Bash","hooks":[{"type":"command","command":"echo hi"}]}]}
        }"#).unwrap();
    let mut journal = SetupJournal::load().unwrap();
    wire_claude_hook(&mut journal, &dir);
    let v = read_settings(&dir);
    assert_eq!(v["permissions"]["allow"][0], "Bash(ls:*)");
    assert_eq!(
        v["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "echo hi"
    );
    assert_eq!(post_cmds(&v), vec!["dl --hook".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}
