use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dl", about = "datalog over files in repo/rev/time space")]
struct Cli {
    program: String,
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.canonicalize()?;
    if cli.lsp {
        sprefa_v5::run_lsp(&cli.program, cli.db.as_deref(), root)
    } else if cli.check || cli.diag_json {
        sprefa_v5::run_check(&cli.program, cli.db.as_deref(), root, cli.diag_json)
    } else if !cli.changed.is_empty() {
        sprefa_v5::run_changed(&cli.program, cli.db.as_deref(), root, cli.changed)
    } else if cli.watch {
        sprefa_v5::run_watch(&cli.program, cli.db.as_deref(), root)
    } else {
        sprefa_v5::run_file(&cli.program, cli.db.as_deref(), root)
    }
}
