use super::manifest::SetupJournal;
use std::path::Path;

/// The opencode bridge plugin (opencode has no native hook config; a JS plugin
/// translates its events to `dl --hook --dialect opencode`). Embedded so a
/// prebuilt `dl` can wire it with no source tree.
const OPENCODE_PLUGIN_JS: &str = include_str!("../../assets/dl-opencode-plugin.js");

/// stdin AND stdout are a terminal — the session can answer a prompt. A piped
/// stdin (CI, `| tee`, a heredoc) reads false, so setup never blocks on input.
pub(super) fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Decide whether to wire one integration. `--yes` wires all without asking;
/// otherwise (a TTY, per the caller's guard) ask a `[Y/n]` question, default Y.
pub(super) fn want(name: &str, assume_yes: bool) -> bool {
    if assume_yes {
        return true;
    }
    use std::io::Write;
    print!("[dl setup] wire {name}? [Y/n] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    let a = line.trim().to_ascii_lowercase();
    a.is_empty() || a == "y" || a == "yes"
}

/// Wire a git pre-commit rail: write `.githooks/pre-commit` (runs `dl --check`,
/// which discovers `<repo>/.dl/*.dl`) and point the repo's `core.hooksPath` at
/// `.githooks`. A non-zero `dl --check` (exit 2 = a `diag` rail tripped) blocks
/// the commit; `git commit -n` bypasses. Idempotent: keeps an existing hook
/// file, and `git config` is a set-or-overwrite of one value. No-op outside a
/// git work tree.
pub(super) fn wire_git_hook(dir: &Path) {
    if !dir.join(".git").exists() {
        println!(
            "[dl setup] not a git repo ({}); skipped pre-commit hook",
            dir.display()
        );
        return;
    }
    let hook = dir.join(".githooks/pre-commit");
    if hook.exists() {
        println!("[dl setup] kept existing {}", hook.display());
    } else {
        // `dl --check` with no program = discovery over <root>/.dl/*.dl.
        let body = "#!/bin/sh\n# sprefa dl rail: blocks a commit when a .dl/ diag rule trips.\nexec dl --check\n";
        let mut journal = match SetupJournal::load() {
            Ok(j) => j,
            Err(_) => return,
        };
        if !journal
            .create_file(Some(dir), &hook, body.as_bytes())
            .unwrap_or(false)
        {
            return;
        }
        let _ = journal.save();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755));
        }
        println!("[dl setup] wrote {}", hook.display());
    }
    let set = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["config", "core.hooksPath", ".githooks"])
        .status();
    match set {
        Ok(s) if s.success() => println!("[dl setup] git core.hooksPath -> .githooks"),
        _ => println!(
            "[dl setup] could not set core.hooksPath; run: git config core.hooksPath .githooks"
        ),
    }
}

/// Register the `dl --hook` hooks in `<repo>/.claude/settings.json`, merging into
/// any existing settings (preserving every other key). This is the
/// editor-independent channel by which a `.dl` rule reacts to the agent: a
/// `PostToolUse` block (matched on Read/Edit/Write/MultiEdit) drives
/// `inject`/`inject_skill`/`block`, and a `UserPromptSubmit` block feeds every
/// user message into `hook_event` (the chat-marks example reads it). `dl --hook`
/// reads the event on stdin and emits the hook JSON (no-op when no rule matches).
/// Idempotent per event: skips an event whose `dl --hook` command is already
/// registered.
pub(super) fn wire_claude_hook(dir: &Path) {
    let cdir = dir.join(".claude");
    let cfg = cdir.join("settings.json");
    let txt = std::fs::read_to_string(&cfg).unwrap_or_default();
    let mut v: serde_json::Value = if txt.trim().is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(&txt) {
            Ok(v) => v,
            Err(_) => {
                println!(
                    "[dl setup] {} is not valid JSON; add PostToolUse + \
                          UserPromptSubmit hooks running `dl --hook` by hand",
                    cfg.display()
                );
                return;
            }
        }
    };
    let Some(obj) = v.as_object_mut() else {
        return;
    };
    let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return;
    };
    // PostToolUse fires on file-touch tools; UserPromptSubmit fires on every user
    // message (no matcher — the whole prompt is the event).
    let mut wrote = false;
    wrote |= register_hook_event(
        hooks,
        "claude code",
        "dl --hook",
        "PostToolUse",
        Some("Read|Edit|Write|MultiEdit"),
        &cfg,
    );
    wrote |= register_hook_event(
        hooks,
        "claude code",
        "dl --hook",
        "UserPromptSubmit",
        None,
        &cfg,
    );
    if !wrote {
        return; // nothing changed (both already present) — leave the file as is
    }
    if let Ok(_) = serde_json::to_string_pretty(&v) {
        let mut journal = match SetupJournal::load() {
            Ok(j) => j,
            Err(_) => return,
        };
        let added = [
            ("PostToolUse".into(), "dl --hook".into()),
            ("UserPromptSubmit".into(), "dl --hook".into()),
        ];
        if journal.hook_config(dir, &cfg, &v, &added).is_ok() {
            let _ = journal.save();
            println!("[dl setup] claude code hooks -> {}", cfg.display());
            println!(
                "[dl setup]   the condition lives in your .dl: PostToolUse reads \
                      agent built-ins (examples/hook-skill-on-test.dl), UserPromptSubmit \
                      feeds hook_event (examples/chat-marks.dl)."
            );
        }
    }
}

/// Register the `dl --hook --dialect codex` hooks in `<repo>/.codex/hooks.json`.
/// Codex CLI reads hooks from this JSON file in the same shape as Claude Code's
/// `settings.json` `hooks` key (verified against the installed binary's embedded
/// schemas, codex 0.144.1) — NOT from `config.toml`, which only stores
/// codex-managed `[hooks.state]` trust hashes. Same merge/idempotency path as
/// wire_claude_hook. Codex runs a hook only after the user trusts it in codex's
/// own UI; dl never writes trust hashes.
pub(super) fn wire_codex_hook(dir: &Path) {
    let cdir = dir.join(".codex");
    let cfg = cdir.join("hooks.json");
    let txt = std::fs::read_to_string(&cfg).unwrap_or_default();
    let mut v: serde_json::Value = if txt.trim().is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(&txt) {
            Ok(v) => v,
            Err(_) => {
                println!(
                    "[dl setup] {} is not valid JSON; add PostToolUse + \
                          UserPromptSubmit hooks running `dl --hook --dialect codex` by hand",
                    cfg.display()
                );
                return;
            }
        }
    };
    let Some(obj) = v.as_object_mut() else {
        return;
    };
    let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return;
    };
    let mut wrote = false;
    wrote |= register_hook_event(
        hooks,
        "codex",
        "dl --hook --dialect codex",
        "PostToolUse",
        Some("Read|Edit|Write|MultiEdit"),
        &cfg,
    );
    wrote |= register_hook_event(
        hooks,
        "codex",
        "dl --hook --dialect codex",
        "UserPromptSubmit",
        None,
        &cfg,
    );
    if !wrote {
        return; // both already present — leave the file as is
    }
    if let Ok(_) = serde_json::to_string_pretty(&v) {
        let mut journal = match SetupJournal::load() {
            Ok(j) => j,
            Err(_) => return,
        };
        let added = [
            ("PostToolUse".into(), "dl --hook --dialect codex".into()),
            (
                "UserPromptSubmit".into(),
                "dl --hook --dialect codex".into(),
            ),
        ];
        if journal.hook_config(dir, &cfg, &v, &added).is_ok() {
            let _ = journal.save();
            println!("[dl setup] codex hooks -> {}", cfg.display());
            println!(
                "[dl setup]   IMPORTANT: codex runs a hook only after you trust it — \
                      open codex in this repo and approve the dl hooks when prompted \
                      (dl never writes trust hashes)."
            );
        }
    }
}

/// Write the opencode bridge plugin to `<repo>/.opencode/plugins/dl.js` from the
/// embedded asset. opencode has no native hook config; the plugin translates its
/// lifecycle events into `dl --hook --dialect opencode`. Idempotent: identical
/// content is left untouched; stale content (an older dl) is refreshed.
pub(super) fn wire_opencode_plugin(dir: &Path) {
    let pdir = dir.join(".opencode/plugins");
    let dest = pdir.join("dl.js");
    if std::fs::read_to_string(&dest).ok().as_deref() == Some(OPENCODE_PLUGIN_JS) {
        println!(
            "[dl setup] opencode plugin already current at {}",
            dest.display()
        );
        return;
    }
    let mut journal = match SetupJournal::load() {
        Ok(j) => j,
        Err(_) => return,
    };
    if journal
        .create_file(Some(dir), &dest, OPENCODE_PLUGIN_JS.as_bytes())
        .unwrap_or(false)
    {
        let _ = journal.save();
        println!("[dl setup] opencode plugin -> {}", dest.display());
    }
}

/// Register one dl hook command under `hooks[event]`, idempotently. Returns
/// true if a new entry was added. `label` names the harness in messages;
/// `command` is the exact hook command; `matcher` is the tool filter
/// (PostToolUse) or None (UserPromptSubmit, no matcher). One shared shape so a
/// new event kind or harness is a one-line call, not a copy of the block above.
fn register_hook_event(
    hooks: &mut serde_json::Map<String, serde_json::Value>,
    label: &str,
    command: &str,
    event: &str,
    matcher: Option<&str>,
    cfg: &Path,
) -> bool {
    let arr = hooks.entry(event).or_insert_with(|| serde_json::json!([]));
    let Some(arr) = arr.as_array_mut() else {
        return false;
    };
    let already = arr.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .is_some_and(|hs| {
                hs.iter().any(|h| {
                    h.get("command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| c.contains("dl --hook"))
                })
            })
    });
    if already {
        println!(
            "[dl setup] {label} {event} hook already registered in {}",
            cfg.display()
        );
        return false;
    }
    let mut entry = serde_json::json!({ "hooks": [ { "type": "command", "command": command } ] });
    if let Some(m) = matcher {
        entry
            .as_object_mut()
            .unwrap()
            .insert("matcher".into(), serde_json::json!(m));
    }
    arr.push(entry);
    true
}

#[cfg(test)]
mod tests {
    use super::super::{
        append_section, bootstrap_project, wire_repo_skills, write_starter, STARTER_DL,
        STARTER_HOOK,
    };
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
        wire_claude_hook(&dir);
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
        wire_claude_hook(&dir);
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
        wire_git_hook(&dir);
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
        wire_codex_hook(&dir);
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
        wire_codex_hook(&dir);
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
        wire_opencode_plugin(&dir);
        let dest = dir.join(".opencode/plugins/dl.js");
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), OPENCODE_PLUGIN_JS);
        // Idempotent second run.
        wire_opencode_plugin(&dir);
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), OPENCODE_PLUGIN_JS);
        // A modified plugin is user content and is never replaced.
        std::fs::write(&dest, "// old plugin\n").unwrap();
        wire_opencode_plugin(&dir);
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
        wire_repo_skills(&dir);
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
        wire_repo_skills(&dir);
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "# My repo skill\n");
        let _ = std::fs::remove_dir_all(&dir);
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
            let dl_dir = dir.join(".dl");
            std::fs::create_dir_all(&dl_dir).unwrap();
            write_starter(&dl_dir.join("dl-self-lint.dl"), STARTER_DL).unwrap();
            write_starter(&dl_dir.join("hook-skill-on-test.dl"), STARTER_HOOK).unwrap();
            append_section(&dir.join("AGENTS.md")).unwrap();
            append_section(&dir.join("CLAUDE.md")).unwrap();
            wire_repo_skills(dir);
            wire_claude_hook(dir);
            wire_codex_hook(dir);
            wire_opencode_plugin(dir);
            wire_git_hook(dir);
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
        wire_claude_hook(&dir);
        let v = read_settings(&dir);
        assert_eq!(v["permissions"]["allow"][0], "Bash(ls:*)");
        assert_eq!(
            v["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            "echo hi"
        );
        assert_eq!(post_cmds(&v), vec!["dl --hook".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
