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

mod hooks;

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

- Run a program: `dl prog.dl` (prints `?` rows; the root is the cwd — there is no `--root`). Let it use the daemon; `--no-daemon` is a last resort for a wedged socket, not the default.
- Discovery rail: `dl --check` runs every `.dl/*.dl`; exits 2 on a `diag` row.
- The `.dl/dl-self-lint.dl` rail makes a broken/mistyped `.dl` a `--check` failure
  (the engine lints `.dl` via the built-in `dl_diag` relation, like rust-analyzer).
- Surface reference: see the engine's generated `docs/reference/{relations,functions,syntax,examples}.md`.
- `agent_edit`/`agent_touch` are git-free (keyed on the cwd root dir); `changed`/`created` need git.
- `dl setup --project` wired `dl --hook` hooks for Claude Code (`.claude/settings.json`),
  codex (`.codex/hooks.json` — trust them in codex's UI before they fire), and opencode
  (`.opencode/plugins/dl.js`): a `.dl/` rule heading `inject`/`inject_skill`/`block`
  fires on a matching tool use (see `.dl/hook-skill-on-test.dl`). Editor-independent
  context injection, no bash glue. Repo skills live in `.agents/skills/` (all three
  harnesses read them; Claude Code via generated `.claude/skills` symlinks).
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
/// no source tree (mirrors the SKILL_MD / starter embedding). The filename is
/// FIXED (`dl-lsp.vsix`, no version) so this line never changes per release; the
/// extension is same-versioned with the binary because `scripts/build-vsix.sh`
/// stamps the extension's package.json to the crate version and rebuilds this
/// artifact, and `build.rs` refuses to compile if the two drift. The committed
/// VSIX is the install artifact for a prebuilt `dl`.
const VSCODE_VSIX: &[u8] = include_bytes!("../editors/vscode-dl/dl-lsp.vsix");
const VSCODE_VSIX_NAME: &str = concat!("dl-lsp-", env!("CARGO_PKG_VERSION"), ".vsix");

/// `dl setup --vscode`: install the dl LSP VSCode extension via the `code` CLI.
/// The extension starts `dl --lsp` with the workspace folder as cwd and proxies LSP over
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
pub(crate) fn wire_global(skills_dir: Option<String>) -> Result<i32> {
    let dest = resolve_skills_dir(skills_dir)?;
    let skill_dir = dest.join("sprefa-dl");
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(skill_dir.join("SKILL.md"), SKILL_MD)?;
    println!("[dl setup] skill -> {}", skill_dir.join("SKILL.md").display());

    if let Ok(h) = home() {
        // ~/.agents/skills is the cross-harness home (codex and opencode read
        // it natively; dl's own resolve_skill checks it first). Write the
        // primary copy there whenever it is not already the dest.
        let agents = h.join(".agents/skills");
        if agents.canonicalize().ok() != dest.canonicalize().ok() {
            let ad = agents.join("sprefa-dl");
            if std::fs::create_dir_all(&ad).is_ok()
                && std::fs::write(ad.join("SKILL.md"), SKILL_MD).is_ok() {
                println!("[dl setup] cross-harness -> {}", ad.join("SKILL.md").display());
            }
        }
    }

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

/// Refresh the on-disk artifacts that embed version-pinned content, so a
/// `dl update` that swapped in a new binary also updates what agents/editors
/// read off disk. Called from `dl update` after a successful install.
///
/// - **Skill (global):** re-writes `SKILL.md` from the new binary's embedded
///   copy at every location `dl setup` wrote it (the resolved skills dir, the
///   `~/.claude/skills` copy, and the opencode path). Idempotent overwrite.
/// - **VSCode extension:** reinstalled from the new embedded VSIX ONLY when the
///   user already has it (preserves their opt-in; never installs on a machine
///   that opted out).
///
/// Project-level wiring (`.claude/settings.json` hooks, `.githooks`, the
/// AGENTS/CLAUDE dl section, `.dl/` starters) lives in arbitrary repos, so it
/// cannot be refreshed from here without walking the filesystem; the caller
/// prints a hint to re-run `dl setup --project` in any bootstrapped repo. MCP
/// has no on-disk wiring (agents spawn `dl --mcp` fresh, picking up the new
/// binary automatically), so there is nothing to refresh there.
pub(crate) fn refresh_after_update() {
    println!("[dl update] refreshing the on-disk skill from the new binary...");
    match wire_global(None) {
        Ok(_) => {}
        Err(e) => eprintln!("[dl update] skill refresh failed ({e}); run `dl setup` by hand"),
    }
    if vscode_extension_installed() {
        println!("[dl update] reinstalling the dl LSP VSCode extension...");
        let _ = install_vscode_extension();
    }
}

/// Whether the dl LSP VSCode extension is currently installed (so `dl update`
/// reinstalls only for users who opted in). `code --list-extensions` prints one
/// id per line; we look for the extension's publisher.id. Returns false when
/// `code` is absent (not installed / not on PATH).
fn vscode_extension_installed() -> bool {
    let out = match std::process::Command::new("code")
        .arg("--list-extensions").output() {
        Ok(o) => o,
        Err(_) => return false, // `code` CLI not present
    };
    out.status.success()
        && String::from_utf8_lossy(&out.stdout).lines().any(|l| l == "sprefa.dl-lsp")
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
    let interactive = hooks::is_tty();
    if !interactive && !assume_yes {
        println!("[dl setup] non-interactive; wired base scaffolding only. Re-run in a \
                  terminal (or pass --yes) to add the Claude Code / codex / opencode \
                  hooks, git pre-commit, or the VSCode extension.");
    } else {
        if hooks::want("the Claude Code PostToolUse hook (.claude/settings.json)", assume_yes) {
            hooks::wire_claude_hook(&dir);
        }
        if hooks::want("the codex hooks (.codex/hooks.json)", assume_yes) {
            hooks::wire_codex_hook(&dir);
        }
        if hooks::want("the opencode plugin (.opencode/plugins/dl.js)", assume_yes) {
            hooks::wire_opencode_plugin(&dir);
        }
        if hooks::want("the git pre-commit rail (.githooks/pre-commit + core.hooksPath)", assume_yes) {
            hooks::wire_git_hook(&dir);
        }
        if hooks::want("the VSCode dl LSP extension (needs the `code` CLI)", assume_yes) {
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
    if let Ok(entries) = std::fs::read_dir(&assets) {
        for e in entries.flatten() {
            let p = e.path();
            let Some(name) = p.file_name().and_then(|s| s.to_str())
                .and_then(|s| s.strip_suffix(".skill.md")) else { continue };
            link_repo_skill(dir, name, Path::new("../../../assets").join(format!("{name}.skill.md")), &p);
        }
    }
    // `.agents/skills/<name>/SKILL.md` is the cross-harness authoring home
    // (codex and opencode read it natively; only Claude Code needs the
    // `.claude/skills` shim, so symlink each into it).
    if let Ok(entries) = std::fs::read_dir(dir.join(".agents/skills")) {
        for e in entries.flatten() {
            let src = e.path().join("SKILL.md");
            if !src.is_file() { continue; }
            let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
            link_repo_skill(
                dir, &name,
                Path::new("../../../.agents/skills").join(&name).join("SKILL.md"),
                &src,
            );
        }
    }
}

/// One `.claude/skills/<name>/SKILL.md` shim: a relative symlink to `target`
/// (copy of `src` on non-unix). Idempotent: a file or live link already at the
/// destination is left alone; a dangling symlink is re-pointed.
fn link_repo_skill(dir: &Path, name: &str, target: PathBuf, src: &Path) {
    let skill_dir = dir.join(".claude/skills").join(name);
    let link = skill_dir.join("SKILL.md");
    if link.exists() { return; } // present and resolvable (file or live link)
    let _ = std::fs::remove_file(&link); // a dangling symlink: exists()=false, remove_file works
    if std::fs::create_dir_all(&skill_dir).is_err() { return; }
    #[cfg(unix)]
    let ok = std::os::unix::fs::symlink(target, &link).is_ok();
    #[cfg(not(unix))]
    let ok = { let _ = target; std::fs::copy(src, &link).is_ok() };
    #[cfg(unix)]
    let _ = src;
    if ok {
        println!("[dl setup] project skill -> {}", link.display());
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
