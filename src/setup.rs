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
mod manifest;
mod vscode;
mod wire;
use manifest::SetupJournal;
use vscode::install_vscode_extension;
pub(crate) use wire::refresh_after_update;
use wire::wire_global;

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
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME not set")
}

/// `dl setup [args]`. Returns the process exit code.
pub fn run(args: &[String]) -> Result<i32> {
    let mut project: Option<Option<String>> = None; // Some(dir) once --project seen
    let mut skills_dir: Option<String> = None;
    let mut print = false;
    let mut vscode = false;
    let mut assume_yes = false;
    let mut list = false;
    let mut undo = false;
    let mut adopt = false;
    let mut dry_run = false;
    let mut undo_root: Option<PathBuf> = None;
    let mut undo_global = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--print" => print = true,
            "--list" => list = true,
            "--undo" => undo = true,
            "--adopt" => adopt = true,
            "--dry-run" => dry_run = true,
            "--global" => undo_global = true,
            "--root" => {
                i += 1;
                let Some(root) = args.get(i) else { eprintln!("dl setup: --root needs a path"); return Ok(2) };
                undo_root = Some(PathBuf::from(root));
            }
            "--vscode" => vscode = true,
            "-y" | "--yes" => assume_yes = true,
            "--skills-dir" => {
                i += 1;
                skills_dir = args.get(i).cloned();
            }
            "--project" => {
                // optional trailing DIR (defaults to cwd if next is a flag/absent)
                let next = args.get(i + 1);
                match next {
                    Some(d) if !d.starts_with("--") => {
                        project = Some(Some(d.clone()));
                        i += 1;
                    }
                    _ => project = Some(None),
                }
            }
            "-h" | "--help" => {
                print_help();
                return Ok(0);
            }
            other => {
                eprintln!("dl setup: unknown arg {other}");
                print_help();
                return Ok(2);
            }
        }
        i += 1;
    }

    if print {
        print!("{SKILL_MD}");
        return Ok(0);
    }
    if list {
        return SetupJournal::load()?.list();
    }
    if undo {
        if undo_global && undo_root.is_some() { eprintln!("dl setup: --root and --global are mutually exclusive"); return Ok(2); }
        let root = undo_root.map(|path| path.canonicalize()).transpose()?;
        return SetupJournal::load()?.undo(root.as_deref(), undo_global, dry_run);
    }
    if adopt {
        return SetupJournal::load()?.adopt();
    }
    if vscode {
        return install_vscode_extension();
    }
    if let Some(dir) = project {
        let target = dir.unwrap_or_else(|| ".".to_string());
        return bootstrap_project(Path::new(&target), assume_yes);
    }
    wire_global(skills_dir)
}

fn print_help() {
    eprintln!("usage: dl setup [--project [DIR]] [--undo|--list|--adopt] [--dry-run] [--vscode] [-y|--yes] [--skills-dir DIR] [--print]");
    eprintln!("  (no args)         install the global skill + wire detected agents");
    eprintln!("  --project [DIR]    bootstrap a repo: .dl/ rails + AGENTS/CLAUDE always;");
    eprintln!("                     Claude Code hook / git pre-commit / VSCode ext prompt on");
    eprintln!("                     a TTY, are skipped when piped, or forced with --yes");
    eprintln!("  --vscode           install the dl LSP VSCode extension (needs `code`); builds a");
    eprintln!(
        "                     fresh VSIX from editors/vscode-dl in a checkout, else embedded"
    );
    eprintln!("  -y, --yes          wire every integration without prompting (scripts / CI)");
    eprintln!("  --skills-dir DIR   override the skill destination");
    eprintln!("  --print            print the embedded skill to stdout");
}

// ── project: bootstrap a repo ─────────────────────────────────────────────────
fn bootstrap_project(dir: &Path, assume_yes: bool) -> Result<i32> {
    let dir = dir
        .canonicalize()
        .with_context(|| format!("no such dir: {}", dir.display()))?;
    println!("[dl setup] bootstrap {}", dir.display());
    // Base scaffolding is always safe (repo-local starter rules + agent docs).
    let mut journal = SetupJournal::load()?;
    let dl_dir = dir.join(".dl");
    write_starter_j(
        &mut journal,
        &dir,
        &dl_dir.join("dl-self-lint.dl"),
        STARTER_DL,
    )?;
    write_starter_j(
        &mut journal,
        &dir,
        &dl_dir.join("hook-skill-on-test.dl"),
        STARTER_HOOK,
    )?;
    append_section_j(&mut journal, &dir, &dir.join("AGENTS.md"))?;
    append_section_j(&mut journal, &dir, &dir.join("CLAUDE.md"))?;
    wire_repo_skills_j(&mut journal, &dir);
    // The integrations change how OTHER tools behave (Claude Code / git config /
    // the editor), so only wire them when the user is present to consent: a TTY
    // prompt, or an explicit `--yes`. A piped / CI run adds nothing (it would be
    // a surprising mutation of the agent + git config).
    let interactive = hooks::is_tty();
    if !interactive && !assume_yes {
        println!(
            "[dl setup] non-interactive; wired base scaffolding only. Re-run in a \
                  terminal (or pass --yes) to add the Claude Code / codex / opencode \
                  hooks, git pre-commit, or the VSCode extension."
        );
    } else {
        if hooks::want(
            "the Claude Code PostToolUse hook (.claude/settings.json)",
            assume_yes,
        ) {
            hooks::wire_claude_hook(&mut journal, &dir);
        }
        if hooks::want("the codex hooks (.codex/hooks.json)", assume_yes) {
            hooks::wire_codex_hook(&mut journal, &dir);
        }
        if hooks::want("the opencode plugin (.opencode/plugins/dl.js)", assume_yes) {
            hooks::wire_opencode_plugin(&mut journal, &dir);
        }
        if hooks::want(
            "the git pre-commit rail (.githooks/pre-commit + core.hooksPath)",
            assume_yes,
        ) {
            hooks::wire_git_hook(&mut journal, &dir);
        }
        if hooks::want(
            "the VSCode dl LSP extension (needs the `code` CLI)",
            assume_yes,
        ) {
            install_vscode_extension()?;
        }
    }
    journal.save()?;
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
fn wire_repo_skills_j(journal: &mut SetupJournal, dir: &Path) {
    let assets = dir.join("assets");
    if let Ok(entries) = std::fs::read_dir(&assets) {
        for e in entries.flatten() {
            let p = e.path();
            let Some(name) = p
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_suffix(".skill.md"))
            else {
                continue;
            };
            link_repo_skill(
                journal,
                dir,
                name,
                Path::new("../../../assets").join(format!("{name}.skill.md")),
                &p,
            );
        }
    }
    // `.agents/skills/<name>/SKILL.md` is the cross-harness authoring home
    // (codex and opencode read it natively; only Claude Code needs the
    // `.claude/skills` shim, so symlink each into it).
    if let Ok(entries) = std::fs::read_dir(dir.join(".agents/skills")) {
        for e in entries.flatten() {
            let src = e.path().join("SKILL.md");
            if !src.is_file() {
                continue;
            }
            let Some(name) = e.file_name().to_str().map(str::to_string) else {
                continue;
            };
            link_repo_skill(
                journal,
                dir,
                &name,
                Path::new("../../../.agents/skills")
                    .join(&name)
                    .join("SKILL.md"),
                &src,
            );
        }
    }
}

/// One `.claude/skills/<name>/SKILL.md` shim: a relative symlink to `target`
/// (copy of `src` on non-unix). Idempotent: a file or live link already at the
/// destination is left alone; a dangling symlink is re-pointed.
fn link_repo_skill(
    journal: &mut SetupJournal,
    dir: &Path,
    name: &str,
    target: PathBuf,
    src: &Path,
) {
    let skill_dir = dir.join(".claude/skills").join(name);
    let link = skill_dir.join("SKILL.md");
    let ok = journal.symlink(Some(dir), &link, &target).unwrap_or(false);
    if ok {
        println!("[dl setup] project skill -> {}", link.display());
    }
}

/// Write a starter `.dl` if absent; never clobber a user's edited rail.
fn write_starter_j(journal: &mut SetupJournal, root: &Path, path: &Path, body: &str) -> Result<()> {
    if !journal.create_file(Some(root), path, body.as_bytes())? {
        println!("[dl setup] kept existing {}", path.display());
    } else {
        println!("[dl setup] wrote {}", path.display());
    }
    Ok(())
}

/// Idempotently append the dl section to an AGENTS.md / CLAUDE.md (skip if the
/// marker is already present). Creates the file if absent.
fn append_section_j(journal: &mut SetupJournal, root: &Path, f: &Path) -> Result<()> {
    if !journal.append_marked(
        Some(root),
        f,
        AGENTS_SECTION,
        "<!-- BEGIN: sprefa-dl -->",
        "<!-- END: sprefa-dl -->",
    )? {
        println!("[dl setup] section already in {}", f.display());
        return Ok(());
    }
    println!(
        "[dl setup] appended dl section to {}",
        f.file_name().unwrap().to_string_lossy()
    );
    Ok(())
}

/// Remove only journal-owned setup artifacts. The executable is intentionally left installed.
pub fn uninstall() -> Result<i32> {
    let mut journal = SetupJournal::load()?;
    journal.undo(None, false, false)?;
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sprefa/setup-manifest.json");
    if state.exists() {
        std::fs::remove_file(&state)?;
    }
    if let Some(dir) = state.parent() {
        match std::fs::remove_dir(dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                println!("[dl uninstall] state directory not empty, left in place: {}", dir.display());
            }
            Err(error) => return Err(error.into()),
        }
    }
    println!("[dl uninstall] wiring removed; binary left installed.");
    println!("[dl uninstall] manual: cargo uninstall dl; remove any codex trust entry in its UI.");
    Ok(0)
}
