//! `dl setup` — wire a (possibly prebuilt) `dl` binary into AI coding agents,
//! with NO dependency on the source tree: the skill text, the starter `.dl`
//! rail, and the AGENTS/CLAUDE section are embedded via `include_str!`, so
//! `curl … -o dl && dl setup` works on a fresh machine.
//!
//!   dl setup                  install the global skill + wire detected agents
//!   dl setup --project [DIR]   bootstrap a repo: starter .dl + AGENTS/CLAUDE block
//!   dl setup --skills-dir DIR  override the skill destination
//!   dl setup --print           print the embedded skill to stdout (piping/inspection)

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const SKILL_MD: &str = include_str!("../assets/sprefa-dl.skill.md");
const STARTER_DL: &str = include_str!("../assets/starter.dl");
// The hook condition starter — same file as the documented example, so the
// bootstrap's `.dl/` set heads `inject_skill` for the wired `dl --hook` to read.
const STARTER_HOOK: &str = include_str!("../examples/hook-skill-on-test.dl");
const AGENTS_SECTION: &str = r#"
<!-- BEGIN: sprefa-dl -->
## Querying this codebase with `dl`

`dl` (sprefa v5) is datalog over code. Reach for it instead of grep when you need
structured facts: call graph, import/type graph, blast radius, lint rails, codemods.

- Run a program: `dl prog.dl --root .` (prints `?` rows). `--no-daemon` for ad-hoc.
- Discovery rail: `dl --check` runs every `.dl/*.dl`; exits 2 on a `diag` row.
- The `.dl/dl-self-lint.dl` rail makes a broken/mistyped `.dl` a `--check` failure
  (the engine lints `.dl` via the built-in `dl_diag` relation, like rust-analyzer).
- Surface reference: see the engine's generated `docs/reference/{relations,functions,syntax,examples}.md`.
- `agent_edit`/`agent_touch` are git-free (keyed on `--root` dir); `changed`/`created` need git.
- `dl setup --project` wired a `dl --hook` PostToolUse hook in `.claude/settings.json`:
  a `.dl/` rule heading `inject`/`inject_skill`/`block` fires on a matching tool use
  (see `.dl/hook-skill-on-test.dl`). Editor-independent context injection, no bash glue.

See the `sprefa-dl` skill for the full surface and authoring gotchas.
<!-- END: sprefa-dl -->
"#;

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).context("HOME not set")
}

/// `dl setup [args]`. Returns the process exit code.
pub fn run(args: &[String]) -> Result<i32> {
    let mut project: Option<Option<String>> = None; // Some(dir) once --project seen
    let mut skills_dir: Option<String> = None;
    let mut print = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--print" => print = true,
            "--skills-dir" => { i += 1; skills_dir = args.get(i).cloned(); }
            "--project" => {
                // optional trailing DIR (defaults to cwd if next is a flag/absent)
                let next = args.get(i + 1);
                match next {
                    Some(d) if !d.starts_with("--") => { project = Some(Some(d.clone())); i += 1; }
                    _ => project = Some(None),
                }
            }
            "-h" | "--help" => { print_help(); return Ok(0); }
            other => { eprintln!("dl setup: unknown arg {other}"); print_help(); return Ok(2); }
        }
        i += 1;
    }

    if print { print!("{SKILL_MD}"); return Ok(0); }
    if let Some(dir) = project {
        let target = dir.unwrap_or_else(|| ".".to_string());
        return bootstrap_project(Path::new(&target));
    }
    wire_global(skills_dir)
}

fn print_help() {
    eprintln!("usage: dl setup [--project [DIR]] [--skills-dir DIR] [--print]");
}

// ── global: install the skill where the agents read it ────────────────────────
fn wire_global(skills_dir: Option<String>) -> Result<i32> {
    let dest = resolve_skills_dir(skills_dir)?;
    let skill_dir = dest.join("sprefa-dl");
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(skill_dir.join("SKILL.md"), SKILL_MD)?;
    println!("[dl setup] skill -> {}", skill_dir.join("SKILL.md").display());

    // Claude Code reads ~/.claude/skills. If that is not already the dest, drop a
    // copy there too (a copy, not a symlink — most portable across machines).
    // Detect Claude Code by ~/.claude (its config dir, present after first run);
    // the skills/ subdir may not exist yet on a fresh machine, so create it.
    if let Ok(h) = home() {
        let cc = h.join(".claude/skills");
        if h.join(".claude").is_dir() && cc.canonicalize().ok() != dest.canonicalize().ok() {
            let ccd = cc.join("sprefa-dl");
            if std::fs::create_dir_all(&ccd).is_ok()
                && std::fs::write(ccd.join("SKILL.md"), SKILL_MD).is_ok() {
                println!("[dl setup] claude code -> {}", ccd.join("SKILL.md").display());
            }
        }
        // opencode: ensure skills.paths contains `dest`.
        wire_opencode(&h, &dest);
    }
    println!("[dl setup] done. agents will pick up the skill on next launch.");
    Ok(0)
}

/// Pick the skill destination: explicit flag > $SPREFA_SKILLS_DIR > opencode's
/// first configured path > ~/.claude/skills > ~/.config/sprefa/skills.
fn resolve_skills_dir(flag: Option<String>) -> Result<PathBuf> {
    if let Some(d) = flag { return Ok(PathBuf::from(d)); }
    if let Some(d) = std::env::var_os("SPREFA_SKILLS_DIR") { return Ok(PathBuf::from(d)); }
    let h = home()?;
    if let Some(p) = opencode_first_skill_path(&h) {
        if p.is_dir() { return Ok(p); }
    }
    // Claude Code: prefer its skills dir whenever Claude Code is present
    // (~/.claude exists), even if the skills/ subdir hasn't been created yet on a
    // fresh machine — wire_global creates it. (The old `cc.is_dir()` check fell
    // through to the no-agent stash below, where Claude Code never reads it.)
    let cc = h.join(".claude/skills");
    if cc.is_dir() || h.join(".claude").is_dir() { return Ok(cc); }
    // No agent detected: stash under XDG. Warn, since nothing reads it until an
    // agent is pointed at it (SPREFA_SKILLS_DIR or opencode skills.paths).
    eprintln!("[dl setup] no Claude Code (~/.claude) or opencode config found; \
               stashing the skill under ~/.config/sprefa/skills. Point an agent at \
               it with SPREFA_SKILLS_DIR=<dir> or `dl setup --skills-dir <dir>`.");
    Ok(h.join(".config/sprefa/skills"))
}

fn opencode_cfg(h: &Path) -> PathBuf { h.join(".config/opencode/opencode.json") }

fn opencode_first_skill_path(h: &Path) -> Option<PathBuf> {
    let txt = std::fs::read_to_string(opencode_cfg(h)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let p = v.get("skills")?.get("paths")?.as_array()?.first()?.as_str()?;
    Some(PathBuf::from(p))
}

/// Add `dest` to opencode.json `skills.paths` if missing, preserving every other
/// key. Pretty-prints back. If the file is absent or unparseable, prints the
/// snippet for the user to add by hand.
fn wire_opencode(h: &Path, dest: &Path) {
    let cfg = opencode_cfg(h);
    let dest_s = dest.to_string_lossy().into_owned();
    let Ok(txt) = std::fs::read_to_string(&cfg) else {
        return; // opencode not installed; nothing to wire
    };
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&txt) else {
        println!("[dl setup] add to {}: \"skills\": {{ \"paths\": [\"{dest_s}\"] }}", cfg.display());
        return;
    };
    let obj = v.as_object_mut();
    let Some(obj) = obj else { return; };
    let skills = obj.entry("skills").or_insert_with(|| serde_json::json!({ "paths": [] }));
    let paths = skills.as_object_mut()
        .and_then(|s| s.entry("paths").or_insert_with(|| serde_json::json!([])).as_array_mut());
    let Some(paths) = paths else { return; };
    if paths.iter().any(|p| p.as_str() == Some(dest_s.as_str())) {
        println!("[dl setup] opencode already reads {dest_s}");
        return;
    }
    paths.push(serde_json::Value::String(dest_s.clone()));
    match serde_json::to_string_pretty(&v) {
        Ok(out) => {
            if std::fs::write(&cfg, out).is_ok() {
                println!("[dl setup] opencode skills.paths += {dest_s}");
            }
        }
        Err(_) => {}
    }
}

// ── project: bootstrap a repo ─────────────────────────────────────────────────
fn bootstrap_project(dir: &Path) -> Result<i32> {
    let dir = dir.canonicalize().with_context(|| format!("no such dir: {}", dir.display()))?;
    println!("[dl setup] bootstrap {}", dir.display());
    let dl_dir = dir.join(".dl");
    std::fs::create_dir_all(&dl_dir)?;
    write_starter(&dl_dir.join("dl-self-lint.dl"), STARTER_DL)?;
    write_starter(&dl_dir.join("hook-skill-on-test.dl"), STARTER_HOOK)?;
    append_section(&dir.join("AGENTS.md"))?;
    append_section(&dir.join("CLAUDE.md"))?;
    wire_claude_hook(&dir);
    println!("[dl setup] run it:  (cd {} && dl --check)", dir.display());
    Ok(0)
}

/// Register the `dl --hook` PostToolUse hook in `<repo>/.claude/settings.json`,
/// merging into any existing settings (preserving every other key). This is the
/// editor-independent channel by which a `.dl` rule heading `inject`/
/// `inject_skill`/`block` can force context into the agent on a tool use. The
/// matcher fires `dl --hook` after Read/Edit/Write/MultiEdit; `dl --hook` reads
/// the PostToolUse event on stdin and emits the hook JSON (no-op when no rule
/// matches). Idempotent: skips if a `dl --hook` command is already registered.
fn wire_claude_hook(dir: &Path) {
    let cdir = dir.join(".claude");
    let cfg = cdir.join("settings.json");
    let txt = std::fs::read_to_string(&cfg).unwrap_or_default();
    let mut v: serde_json::Value = if txt.trim().is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(&txt) {
            Ok(v) => v,
            Err(_) => {
                println!("[dl setup] {} is not valid JSON; add a PostToolUse hook \
                          running `dl --hook` by hand", cfg.display());
                return;
            }
        }
    };
    let Some(obj) = v.as_object_mut() else { return; };
    let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else { return; };
    let post = hooks.entry("PostToolUse").or_insert_with(|| serde_json::json!([]));
    let Some(post) = post.as_array_mut() else { return; };
    // Already wired? Look for any nested hook command containing `dl --hook`.
    let already = post.iter().any(|entry| {
        entry.get("hooks").and_then(|h| h.as_array()).is_some_and(|hs| {
            hs.iter().any(|h| {
                h.get("command").and_then(|c| c.as_str())
                    .is_some_and(|c| c.contains("dl --hook"))
            })
        })
    });
    if already {
        println!("[dl setup] claude code hook already registered in {}", cfg.display());
        return;
    }
    post.push(serde_json::json!({
        "matcher": "Read|Edit|Write|MultiEdit",
        "hooks": [ { "type": "command", "command": "dl --hook" } ]
    }));
    if std::fs::create_dir_all(&cdir).is_err() {
        return;
    }
    match serde_json::to_string_pretty(&v) {
        Ok(out) => {
            if std::fs::write(&cfg, out).is_ok() {
                println!("[dl setup] claude code PostToolUse hook -> {}", cfg.display());
                println!("[dl setup]   condition lives in your .dl (see \
                          examples/hook-skill-on-test.dl): a rule heading \
                          inject/inject_skill/block fires on a matching tool use.");
            }
        }
        Err(_) => {}
    }
}

/// Write a starter `.dl` if absent; never clobber a user's edited rail.
fn write_starter(path: &Path, body: &str) -> Result<()> {
    if path.exists() {
        println!("[dl setup] kept existing {}", path.display());
    } else {
        std::fs::write(path, body)?;
        println!("[dl setup] wrote {}", path.display());
    }
    Ok(())
}

/// Idempotently append the dl section to an AGENTS.md / CLAUDE.md (skip if the
/// marker is already present). Creates the file if absent.
fn append_section(f: &Path) -> Result<()> {
    let existing = std::fs::read_to_string(f).unwrap_or_default();
    if existing.contains("BEGIN: sprefa-dl") {
        println!("[dl setup] section already in {}", f.display());
        return Ok(());
    }
    let mut out = existing;
    out.push_str(AGENTS_SECTION);
    std::fs::write(f, out)?;
    println!("[dl setup] appended dl section to {}", f.file_name().unwrap().to_string_lossy());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn read_settings(dir: &Path) -> Value {
        let txt = std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap();
        serde_json::from_str(&txt).unwrap()
    }

    fn post_cmds(v: &Value) -> Vec<String> {
        v["hooks"]["PostToolUse"].as_array().unwrap().iter()
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
        let cmds = post_cmds(&read_settings(&dir));
        assert_eq!(cmds, vec!["dl --hook".to_string()]);
        // Second call must not add a duplicate.
        wire_claude_hook(&dir);
        assert_eq!(post_cmds(&read_settings(&dir)), vec!["dl --hook".to_string()]);
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
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "echo hi");
        assert_eq!(post_cmds(&v), vec!["dl --hook".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
