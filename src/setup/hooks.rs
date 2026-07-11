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
pub(super) fn wire_git_hook(journal: &mut SetupJournal, dir: &Path) {
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
        if !journal
            .create_file(Some(dir), &hook, body.as_bytes())
            .unwrap_or(false)
        {
            return;
        }
        #[cfg(unix)]
        {
            if journal.make_executable(&hook).is_err() { return; }
        }
        println!("[dl setup] wrote {}", hook.display());
    }
    match journal.git_hooks_path(dir) {
        Ok(true) => println!("[dl setup] git core.hooksPath -> .githooks"),
        Ok(false) => {}
        Err(_) => println!("[dl setup] could not set core.hooksPath; run: git config core.hooksPath .githooks"),
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
pub(super) fn wire_claude_hook(journal: &mut SetupJournal, dir: &Path) {
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
    let post_added = register_hook_event(
        hooks,
        "claude code",
        "dl --hook",
        "PostToolUse",
        Some("Read|Edit|Write|MultiEdit"),
        &cfg,
    );
    let prompt_added = register_hook_event(
        hooks,
        "claude code",
        "dl --hook",
        "UserPromptSubmit",
        None,
        &cfg,
    );
    if !post_added { journal.record_hook(dir, &cfg, "PostToolUse", "dl --hook"); }
    if !prompt_added { journal.record_hook(dir, &cfg, "UserPromptSubmit", "dl --hook"); }
    if !post_added && !prompt_added {
        return; // nothing changed (both already present) — leave the file as is
    }
    if let Ok(_) = serde_json::to_string_pretty(&v) {
        let mut added = Vec::new();
        if post_added { added.push(("PostToolUse".into(), "dl --hook".into())); }
        if prompt_added { added.push(("UserPromptSubmit".into(), "dl --hook".into())); }
        if journal.hook_config(dir, &cfg, &v, &added).is_ok() {
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
pub(super) fn wire_codex_hook(journal: &mut SetupJournal, dir: &Path) {
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
    let post_added = register_hook_event(
        hooks,
        "codex",
        "dl --hook --dialect codex",
        "PostToolUse",
        Some("Read|Edit|Write|MultiEdit"),
        &cfg,
    );
    let prompt_added = register_hook_event(
        hooks,
        "codex",
        "dl --hook --dialect codex",
        "UserPromptSubmit",
        None,
        &cfg,
    );
    if !post_added { journal.record_hook(dir, &cfg, "PostToolUse", "dl --hook --dialect codex"); }
    if !prompt_added { journal.record_hook(dir, &cfg, "UserPromptSubmit", "dl --hook --dialect codex"); }
    if !post_added && !prompt_added {
        return; // both already present — leave the file as is
    }
    if let Ok(_) = serde_json::to_string_pretty(&v) {
        let mut added = Vec::new();
        if post_added { added.push(("PostToolUse".into(), "dl --hook --dialect codex".into())); }
        if prompt_added { added.push(("UserPromptSubmit".into(), "dl --hook --dialect codex".into())); }
        if journal.hook_config(dir, &cfg, &v, &added).is_ok() {
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
pub(super) fn wire_opencode_plugin(journal: &mut SetupJournal, dir: &Path) {
    let pdir = dir.join(".opencode/plugins");
    let dest = pdir.join("dl.js");
    if std::fs::read_to_string(&dest).ok().as_deref() == Some(OPENCODE_PLUGIN_JS) {
        println!(
            "[dl setup] opencode plugin already current at {}",
            dest.display()
        );
        return;
    }
    if journal
        .create_file(Some(dir), &dest, OPENCODE_PLUGIN_JS.as_bytes())
        .unwrap_or(false)
    {
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
#[path = "hooks_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hooks_idempotency_tests.rs"]
mod idempotency_tests;
