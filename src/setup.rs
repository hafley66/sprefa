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

/// The embedded skill body (the rules survival guide + language matrix + op
/// quickref). `pub` so `docs_cmd` re-exports it as the `authoring` topic and the
/// matrix-honesty test can parse the language matrix out of it without a second
/// `include_str!`.
pub const SKILL_MD: &str = include_str!("../assets/sprefa-dl.skill.md");
const STARTER_DL: &str = include_str!("../assets/starter.dl");
// The hook condition starter — same file as the documented example, so the
// bootstrap's `.dl/` set heads `inject_skill` for the wired `dl --hook` to read.
const STARTER_HOOK: &str = include_str!("../examples/hook-skill-on-test.dl");
const AGENTS_SECTION: &str = r#"
<!-- BEGIN: sprefa-dl -->
## Querying this codebase with `dl`

`dl` (sprefa v5) is datalog over code. Reach for it instead of grep when you need
structured facts: call graph, import/type graph, blast radius, lint rails, codemods.

- Run a program: `dl prog.dl --root .` (prints `?` rows). Let it use the daemon; `--no-daemon` is a last resort for a wedged socket, not the default.
- Discovery rail: `dl --check` runs every `.dl/*.dl`; exits 2 on a `diag` row.
- The `.dl/dl-self-lint.dl` rail makes a broken/mistyped `.dl` a `--check` failure
  (the engine lints `.dl` via the built-in `dl_diag` relation, like rust-analyzer).
- Surface reference: see the engine's generated `docs/reference/{relations,functions,syntax,examples}.md`.
- `agent_edit`/`agent_touch` are git-free (keyed on `--root` dir); `changed`/`created` need git.
- `dl setup --project` wired a `dl --hook` PostToolUse hook in `.claude/settings.json`:
  a `.dl/` rule heading `inject`/`inject_skill`/`block` fires on a matching tool use
  (see `.dl/hook-skill-on-test.dl`). Editor-independent context injection, no bash glue.
- It also wired a `.githooks/pre-commit` (`dl --check`) + `core.hooksPath`, so a
  `diag` rail in `.dl/*.dl` blocks a bad commit (`git commit -n` bypasses).
- Live editor squiggles: `dl setup --vscode` installs the bundled LSP extension.
- Compiler-precise facts (SCIP): `dl index` detects the language(s) and runs the
  right indexer (rust-analyzer / scip-typescript / scip-python / scip-go /
  scip-java / scip-clang), placing `<root>/.dl/index.scip` (gitignored). The
  `scip_*` relations then load automatically. `dl doctor` reports what is
  detected, whether an index is present/fresh, and its row counts. Syntactic
  facts are any-rev; SCIP covers the working tree only.

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
    let mut vscode = false;
    let mut assume_yes = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--print" => print = true,
            "--vscode" => vscode = true,
            "-y" | "--yes" => assume_yes = true,
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
    if vscode { return install_vscode_extension(); }
    if let Some(dir) = project {
        let target = dir.unwrap_or_else(|| ".".to_string());
        return bootstrap_project(Path::new(&target), assume_yes);
    }
    wire_global(skills_dir)
}

fn print_help() {
    eprintln!("usage: dl setup [--project [DIR]] [--vscode] [-y|--yes] [--skills-dir DIR] [--print]");
    eprintln!("  (no args)         install the global skill + wire detected agents");
    eprintln!("  --project [DIR]    bootstrap a repo: .dl/ rails + AGENTS/CLAUDE always;");
    eprintln!("                     Claude Code hook / git pre-commit / VSCode ext prompt on");
    eprintln!("                     a TTY, are skipped when piped, or forced with --yes");
    eprintln!("  --vscode           install the dl LSP VSCode extension (needs `code`); builds a");
    eprintln!("                     fresh VSIX from editors/vscode-dl in a checkout, else embedded");
    eprintln!("  -y, --yes          wire every integration without prompting (scripts / CI)");
    eprintln!("  --skills-dir DIR   override the skill destination");
    eprintln!("  --print            print the embedded skill to stdout");
}

/// The dl LSP VSCode extension, embedded so a prebuilt `dl` can install it with
/// no source tree (mirrors the SKILL_MD / starter embedding). Rebuild + bump:
/// `(cd editors/vscode-dl && npm run compile && npx vsce package)`, then point
/// this at the new VSIX. The committed VSIX is the install artifact.
const VSCODE_VSIX: &[u8] = include_bytes!("../editors/vscode-dl/dl-lsp-0.4.9.vsix");
const VSCODE_VSIX_NAME: &str = "dl-lsp-0.4.9.vsix";

/// `dl setup --vscode`: install the dl LSP VSCode extension via the `code` CLI.
/// The extension starts `dl --root <workspace> --lsp` and proxies LSP over
/// stdio, so `.dl/*.dl` rules give live squiggles. Turnkey both ways: from a
/// repo checkout it builds a FRESH VSIX from `editors/vscode-dl` (always current
/// — no "did I rebuild the vsix?" step); a prebuilt `dl` run outside the source
/// falls back to the VSIX embedded at build time. Either path installs
/// uninstall-first to dodge the same-version reinstall no-op. No-op with a clear
/// message if `code` is not on PATH.
fn install_vscode_extension() -> Result<i32> {
    // Prefer a fresh build from the checked-out source; fall back to embedded.
    let (vsix, from_source) = match build_vscode_vsix() {
        Some(built) => (built, true),
        None => {
            let embedded = std::env::temp_dir().join(VSCODE_VSIX_NAME);
            std::fs::write(&embedded, VSCODE_VSIX)
                .with_context(|| format!("write {}", embedded.display()))?;
            (embedded, false)
        }
    };
    // The same-version trap: `code --install-extension --force` silently no-ops
    // on a same-version reinstall while VSCode is running (a fresh build carries
    // the same version). Uninstall first so the new bits always land.
    let _ = std::process::Command::new("code")
        .args(["--uninstall-extension", "sprefa.dl-lsp"]).status();
    let status = std::process::Command::new("code")
        .arg("--install-extension").arg(&vsix).arg("--force")
        .status();
    match status {
        Ok(s) if s.success() => {
            let how = if from_source { "freshly built from editors/vscode-dl" } else { "embedded" };
            println!("[dl setup] installed the dl LSP VSCode extension ({how}).");
            println!("[dl setup] reload VSCode; open any file your .dl rules scan for live squiggles.");
            if !from_source { let _ = std::fs::remove_file(&vsix); }
            Ok(0)
        }
        Ok(s) => {
            eprintln!("[dl setup] `code --install-extension` exited {s}; VSIX left at {}", vsix.display());
            Ok(1)
        }
        Err(_) => {
            println!("[dl setup] VSCode `code` CLI not found on PATH. The extension VSIX is at:");
            println!("             {}", vsix.display());
            println!("[dl setup] install it by hand: `code --install-extension {}`", vsix.display());
            println!("[dl setup] (in VSCode, enable the `code` command via Shell Command: Install 'code' in PATH).");
            Ok(0)
        }
    }
}

/// Build a fresh VSIX from the checked-out extension source when the tree and
/// toolchain are present (`editors/vscode-dl` under CWD + `npm` + `npx vsce`).
/// Returns the built VSIX path, or `None` to fall back to the embedded artifact
/// (a prebuilt `dl` run outside the repo, or any build step failing). `npm ci`
/// only when `node_modules` is missing, so a repeat install just compiles.
fn build_vscode_vsix() -> Option<PathBuf> {
    let src = Path::new("editors/vscode-dl");
    if !src.join("package.json").exists() {
        return None;
    }
    let run = |args: &[&str]| -> bool {
        std::process::Command::new(args[0]).args(&args[1..]).current_dir(src)
            .status().map(|st| st.success()).unwrap_or(false)
    };
    eprintln!("[dl setup] building the extension from {} ...", src.display());
    if !src.join("node_modules").exists() && !run(&["npm", "ci"]) && !run(&["npm", "install"]) {
        eprintln!("[dl setup] npm install failed; using the embedded VSIX instead");
        return None;
    }
    if !run(&["npm", "run", "compile"]) {
        eprintln!("[dl setup] extension compile failed; using the embedded VSIX instead");
        return None;
    }
    let out = std::env::temp_dir().join("dl-lsp-fresh.vsix");
    let _ = std::fs::remove_file(&out);
    let packaged = std::process::Command::new("npx")
        .args(["vsce", "package", "-o"]).arg(&out).current_dir(src)
        .status().map(|st| st.success()).unwrap_or(false);
    if packaged && out.exists() {
        Some(out)
    } else {
        eprintln!("[dl setup] `vsce package` failed; using the embedded VSIX instead");
        None
    }
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
fn bootstrap_project(dir: &Path, assume_yes: bool) -> Result<i32> {
    let dir = dir.canonicalize().with_context(|| format!("no such dir: {}", dir.display()))?;
    println!("[dl setup] bootstrap {}", dir.display());
    // Base scaffolding is always safe (repo-local starter rules + agent docs).
    let dl_dir = dir.join(".dl");
    std::fs::create_dir_all(&dl_dir)?;
    write_starter(&dl_dir.join("dl-self-lint.dl"), STARTER_DL)?;
    write_starter(&dl_dir.join("hook-skill-on-test.dl"), STARTER_HOOK)?;
    append_section(&dir.join("AGENTS.md"))?;
    append_section(&dir.join("CLAUDE.md"))?;
    wire_repo_skills(&dir);
    // The integrations change how OTHER tools behave (Claude Code / git config /
    // the editor), so only wire them when the user is present to consent: a TTY
    // prompt, or an explicit `--yes`. A piped / CI run adds nothing (it would be
    // a surprising mutation of the agent + git config).
    let interactive = is_tty();
    if !interactive && !assume_yes {
        println!("[dl setup] non-interactive; wired base scaffolding only. Re-run in a \
                  terminal (or pass --yes) to add the Claude Code hook / git pre-commit / \
                  VSCode extension.");
    } else {
        if want("the Claude Code PostToolUse hook (.claude/settings.json)", assume_yes) {
            wire_claude_hook(&dir);
        }
        if want("the git pre-commit rail (.githooks/pre-commit + core.hooksPath)", assume_yes) {
            wire_git_hook(&dir);
        }
        if want("the VSCode dl LSP extension (needs the `code` CLI)", assume_yes) {
            install_vscode_extension()?;
        }
    }
    println!("[dl setup] run it:  (cd {} && dl --check)", dir.display());
    Ok(0)
}

/// Expose a repo's tracked skills as PROJECT skills: every `assets/*.skill.md`
/// gets a `.claude/skills/<name>/SKILL.md` symlink (relative, so the checkout
/// can move). `.claude/` is typically gitignored, which is why a fresh clone
/// lacks the links even though the skill text is tracked — this recreates
/// them. In this repo that wires the three maintainer checklists alongside the
/// embedded consumer skill; any repo adopting the `assets/*.skill.md`
/// convention gets the same. Idempotent: a link (or file) already at the
/// destination is left alone; a dangling symlink is re-pointed. Non-unix
/// falls back to a copy.
fn wire_repo_skills(dir: &Path) {
    let assets = dir.join("assets");
    let Ok(entries) = std::fs::read_dir(&assets) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let Some(name) = p.file_name().and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".skill.md")) else { continue };
        let skill_dir = dir.join(".claude/skills").join(name);
        let link = skill_dir.join("SKILL.md");
        if link.exists() { continue; } // present and resolvable (file or live link)
        let _ = std::fs::remove_file(&link); // a dangling symlink: exists()=false, remove_file works
        if std::fs::create_dir_all(&skill_dir).is_err() { continue; }
        #[cfg(unix)]
        let ok = std::os::unix::fs::symlink(
            Path::new("../../../assets").join(format!("{name}.skill.md")), &link).is_ok();
        #[cfg(not(unix))]
        let ok = std::fs::copy(&p, &link).is_ok();
        if ok {
            println!("[dl setup] project skill -> {}", link.display());
        }
    }
}

/// stdin AND stdout are a terminal — the session can answer a prompt. A piped
/// stdin (CI, `| tee`, a heredoc) reads false, so setup never blocks on input.
fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Decide whether to wire one integration. `--yes` wires all without asking;
/// otherwise (a TTY, per the caller's guard) ask a `[Y/n]` question, default Y.
fn want(name: &str, assume_yes: bool) -> bool {
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
fn wire_git_hook(dir: &Path) {
    if !dir.join(".git").exists() {
        println!("[dl setup] not a git repo ({}); skipped pre-commit hook", dir.display());
        return;
    }
    let hooks_dir = dir.join(".githooks");
    if std::fs::create_dir_all(&hooks_dir).is_err() {
        return;
    }
    let hook = hooks_dir.join("pre-commit");
    if hook.exists() {
        println!("[dl setup] kept existing {}", hook.display());
    } else {
        // `dl --check` with no program = discovery over <root>/.dl/*.dl.
        let body = "#!/bin/sh\n# sprefa dl rail: blocks a commit when a .dl/ diag rule trips.\nexec dl --check\n";
        if std::fs::write(&hook, body).is_err() {
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755));
        }
        println!("[dl setup] wrote {}", hook.display());
    }
    let set = std::process::Command::new("git")
        .arg("-C").arg(dir)
        .args(["config", "core.hooksPath", ".githooks"])
        .status();
    match set {
        Ok(s) if s.success() => println!("[dl setup] git core.hooksPath -> .githooks"),
        _ => println!("[dl setup] could not set core.hooksPath; run: git config core.hooksPath .githooks"),
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
                println!("[dl setup] {} is not valid JSON; add PostToolUse + \
                          UserPromptSubmit hooks running `dl --hook` by hand", cfg.display());
                return;
            }
        }
    };
    let Some(obj) = v.as_object_mut() else { return; };
    let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else { return; };
    // PostToolUse fires on file-touch tools; UserPromptSubmit fires on every user
    // message (no matcher — the whole prompt is the event).
    let mut wrote = false;
    wrote |= register_hook_event(hooks, "PostToolUse", Some("Read|Edit|Write|MultiEdit"), &cfg);
    wrote |= register_hook_event(hooks, "UserPromptSubmit", None, &cfg);
    if !wrote {
        return; // nothing changed (both already present) — leave the file as is
    }
    if std::fs::create_dir_all(&cdir).is_err() {
        return;
    }
    if let Ok(out) = serde_json::to_string_pretty(&v) {
        if std::fs::write(&cfg, out).is_ok() {
            println!("[dl setup] claude code hooks -> {}", cfg.display());
            println!("[dl setup]   the condition lives in your .dl: PostToolUse reads \
                      agent built-ins (examples/hook-skill-on-test.dl), UserPromptSubmit \
                      feeds hook_event (examples/chat-marks.dl).");
        }
    }
}

/// Register one `dl --hook` command under `hooks[event]`, idempotently. Returns
/// true if a new entry was added. `matcher` is the tool filter (PostToolUse) or
/// None (UserPromptSubmit, no matcher). One shared shape so a new event kind is a
/// one-line call, not a copy of the block above.
fn register_hook_event(
    hooks: &mut serde_json::Map<String, serde_json::Value>,
    event: &str,
    matcher: Option<&str>,
    cfg: &Path,
) -> bool {
    let arr = hooks.entry(event).or_insert_with(|| serde_json::json!([]));
    let Some(arr) = arr.as_array_mut() else { return false; };
    let already = arr.iter().any(|entry| {
        entry.get("hooks").and_then(|h| h.as_array()).is_some_and(|hs| {
            hs.iter().any(|h| {
                h.get("command").and_then(|c| c.as_str())
                    .is_some_and(|c| c.contains("dl --hook"))
            })
        })
    });
    if already {
        println!("[dl setup] claude code {event} hook already registered in {}", cfg.display());
        return false;
    }
    let mut entry = serde_json::json!({ "hooks": [ { "type": "command", "command": "dl --hook" } ] });
    if let Some(m) = matcher {
        entry.as_object_mut().unwrap().insert("matcher".into(), serde_json::json!(m));
    }
    arr.push(entry);
    true
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

    fn ups_cmds(v: &Value) -> Vec<String> {
        v["hooks"]["UserPromptSubmit"].as_array().unwrap().iter()
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
        assert!(v["hooks"]["UserPromptSubmit"][0].get("matcher").is_none(),
            "UserPromptSubmit hook has no tool matcher");
        assert_eq!(v["hooks"]["PostToolUse"][0]["matcher"], "Read|Edit|Write|MultiEdit");
        // Second call must not add a duplicate under either event.
        wire_claude_hook(&dir);
        let v = read_settings(&dir);
        assert_eq!(post_cmds(&v), vec!["dl --hook".to_string()]);
        assert_eq!(ups_cmds(&v), vec!["dl --hook".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn git_init(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        assert!(std::process::Command::new("git").arg("-C").arg(dir)
            .arg("init").arg("-q").status().unwrap().success());
    }

    #[test]
    fn bootstrap_noninteractive_skips_integrations() {
        // Under `cargo test`, stdin is not a TTY -> is_tty() is false. Without
        // --yes, base scaffolding lands but the integrations do not.
        let dir = std::env::temp_dir().join(format!("dlsetup_noint_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        git_init(&dir);
        assert_eq!(bootstrap_project(&dir, false).unwrap(), 0);
        assert!(dir.join(".dl/dl-self-lint.dl").exists(), "base scaffolding written");
        assert!(!dir.join(".claude/settings.json").exists(), "CC hook skipped (non-tty)");
        assert!(!dir.join(".githooks/pre-commit").exists(), "git hook skipped (non-tty)");
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
        assert!(dir.join(".claude/settings.json").exists(), "CC hook wired (--yes)");
        assert!(dir.join(".githooks/pre-commit").exists(), "git hook wired (--yes)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn git_hook_written_and_hooks_path_set() {
        let dir = std::env::temp_dir().join(format!("dlsetup_git_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(std::process::Command::new("git").arg("-C").arg(&dir)
            .arg("init").arg("-q").status().unwrap().success());
        wire_git_hook(&dir);
        let hook = dir.join(".githooks/pre-commit");
        assert!(hook.exists(), "pre-commit hook written");
        let body = std::fs::read_to_string(&hook).unwrap();
        assert!(body.contains("dl --check"), "hook runs dl --check");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(hook.metadata().unwrap().permissions().mode() & 0o111, 0, "executable");
        }
        let cfg = std::process::Command::new("git").arg("-C").arg(&dir)
            .args(["config", "core.hooksPath"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&cfg.stdout).trim(), ".githooks");
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
