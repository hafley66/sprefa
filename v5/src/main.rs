use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dl", about = "datalog over files in repo/rev/time space")]
struct Cli {
    /// The .dl program to run. Optional only with --move (which synthesizes its own).
    program: Option<String>,
    #[arg(long)]
    db: Option<String>,
    /// Source root. When omitted, defaults to the nearest `.git` ancestor of
    /// the program file (the repo it lives in), else the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long)]
    watch: bool,
    /// Run as an LSP server over stdio: the program's `diag` relation becomes
    /// live editor diagnostics (lint on open/save). See docs/lsp.md.
    #[arg(long)]
    lsp: bool,
    /// Lint/ban mode: render the `diag` relation to stderr and exit non-zero if
    /// any `error`-severity row exists. For pre-commit / CI / Claude Code hooks.
    #[arg(long)]
    check: bool,
    /// Like --check but emit the diagnostics as a JSON array on stdout.
    #[arg(long)]
    diag_json: bool,
    /// Drive one incremental tick for these changed paths (the delta path the
    /// watcher uses), instead of a full run. Repeatable.
    #[arg(long)]
    changed: Vec<PathBuf>,
    /// Auto-refactor: rewrite `use`-path references for a module move
    /// `OLD_FILE=NEW_FILE` (repo-relative Rust paths). Dry-run unless --fix.
    /// Repeatable. Ignores the `program` positional.
    #[arg(long = "move")]
    move_: Vec<String>,
    /// With --move, which repo to rewrite: a config slug, or `*`/`all` for every
    /// configured repo. Omitted = the --root repo (self).
    #[arg(long)]
    repo: Option<String>,
    /// With --move, write the rewritten files instead of previewing.
    #[arg(long)]
    fix: bool,
}

/// Explicit `--root` wins (canonicalized). Otherwise default to the repo the
/// program file lives in (nearest `.git` ancestor of its dir), falling back to
/// the current directory. With `--move` (no program), use the current dir.
fn resolve_root(cli: &Cli) -> Result<PathBuf> {
    if let Some(r) = &cli.root {
        return Ok(r.canonicalize()?);
    }
    let base = cli.program.as_deref()
        .and_then(|p| std::fs::canonicalize(p).ok())
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .map(Ok)
        .unwrap_or_else(std::env::current_dir)?;
    Ok(sprefa_v5::repo::nearest_git(&base).unwrap_or(base))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = resolve_root(&cli)?;
    if !cli.move_.is_empty() {
        return sprefa_v5::run_move(cli.db.as_deref(), root, cli.repo, cli.move_, cli.fix);
    }
    let program = cli.program.ok_or_else(|| anyhow::anyhow!("a .dl program path is required"))?;
    if cli.lsp {
        sprefa_v5::run_lsp(&program, cli.db.as_deref(), root)
    } else if cli.check || cli.diag_json {
        sprefa_v5::run_check(&program, cli.db.as_deref(), root, cli.diag_json)
    } else if !cli.changed.is_empty() {
        sprefa_v5::run_changed(&program, cli.db.as_deref(), root, cli.changed)
    } else if cli.watch {
        sprefa_v5::run_watch(&program, cli.db.as_deref(), root)
    } else {
        sprefa_v5::run_file(&program, cli.db.as_deref(), root)
    }
}
