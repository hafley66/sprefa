use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dl", about = "datalog over files in repo/rev/time space")]
struct Cli {
    /// The .dl program to run. When omitted, discovery: every `<root>/.dl/*.dl`
    /// file (lexicographic) merges into one program. (--move synthesizes its own
    /// and ignores this.)
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
    /// Lint/ban mode: render the `diag` relation to stderr. Exit 0 clean, 2 if
    /// any `error`-severity row exists (Claude Code's blocking-hook code), 1 on
    /// a broken program. For pre-commit / CI / Claude Code hooks. See docs/rails.md.
    #[arg(long)]
    check: bool,
    /// Like --check but emit the diagnostics as a JSON array on stdout.
    #[arg(long)]
    diag_json: bool,
    /// Emit `?` query results as JSON-lines (one object per query:
    /// {query, columns, rows, count}) instead of the human TSV block.
    #[arg(long)]
    query_json: bool,
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
    /// Profile mode (or DL_PROFILE=1): log slow SQL statements (threshold
    /// DL_PROFILE_SQL_MS, default 25), per-repo scan times, tick phase
    /// breakdown, and per-tick statement counts.
    #[arg(long)]
    profile: bool,
    /// Cap `cmd` invocations per tick (or DL_CMD_BUDGET); over budget is a loud
    /// error, never a silent truncation. Default: unlimited.
    #[arg(long)]
    cmd_budget: Option<u32>,
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
    if cli.profile { sprefa_v5::db::set_profile(true); }
    if let Some(n) = cli.cmd_budget { sprefa_v5::engine::set_cmd_budget(n); }
    let root = resolve_root(&cli)?;
    if !cli.move_.is_empty() {
        return sprefa_v5::run_move(cli.db.as_deref(), root, cli.repo, cli.move_, cli.fix);
    }
    // Discovery mode (no positional) defaults the db to <root>/.dl/cache.db so
    // repeated hook/check invocations get warm incremental ticks instead of a
    // cold in-memory rescan. A generated .gitignore keeps the cache out of git.
    let mut db = cli.db;
    if cli.program.is_none() && db.is_none() {
        let dir = root.join(".dl");
        if dir.is_dir() {
            let gi = dir.join(".gitignore");
            if !gi.exists() { let _ = std::fs::write(&gi, "cache.db*\n"); }
            db = Some(dir.join("cache.db").to_string_lossy().into_owned());
        }
    }
    let program = cli.program.as_deref();
    if cli.lsp {
        sprefa_v5::run_lsp(program, db.as_deref(), root)
    } else if cli.check || cli.diag_json {
        // Exit contract: 0 clean, 2 rail violations (Claude Code's blocking-hook
        // code; stderr feeds the agent), 1 broken program (user-facing).
        let errors = sprefa_v5::run_check(program, db.as_deref(), root, cli.diag_json)?;
        if errors > 0 {
            eprintln!("{errors} error-severity diagnostic(s) found");
            std::process::exit(2);
        }
        Ok(())
    } else if !cli.changed.is_empty() {
        sprefa_v5::run_changed(program, db.as_deref(), root, cli.changed)
    } else if cli.watch {
        sprefa_v5::run_watch(program, db.as_deref(), root)
    } else {
        sprefa_v5::run_file(program, db.as_deref(), root, cli.query_json)
    }
}
