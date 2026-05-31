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
    #[arg(long, default_value = ".")]
    root: PathBuf,
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
    /// With --move, write the rewritten files instead of previewing.
    #[arg(long)]
    fix: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.canonicalize()?;
    if !cli.move_.is_empty() {
        return sprefa_v5::run_move(cli.db.as_deref(), root, cli.move_, cli.fix);
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
