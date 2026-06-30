use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dl", about = "datalog over files in repo/rev/time space")]
struct Cli {
    /// The .dl program(s) to run. Multiple files merge into one program (in the
    /// given order, with `use` includes spliced). When omitted, discovery: every
    /// `<root>/.dl/*.dl` file (lexicographic) merges instead. (--move synthesizes
    /// its own and ignores this.)
    programs: Vec<String>,
    /// Persist derived tables to a SQLite db at this path (default: in-memory;
    /// discovery mode defaults to `<root>/.dl/cache.db`). Derived relations land
    /// as plain-TEXT `rel_<name>` tables, queryable by anything that reads SQLite.
    #[arg(long)]
    db: Option<String>,
    /// Source root. When omitted, defaults to the nearest `.git` ancestor of
    /// the program file (the repo it lives in), else the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Re-tick on file changes in the source root (in-process watcher, the
    /// pre-daemon path). For the warm long-lived watcher, use `--daemon`.
    #[arg(long)]
    watch: bool,
    /// Run as an LSP server over stdio: the program's `diag` relation becomes
    /// live editor diagnostics (lint on open/save). See docs/lsp.md.
    #[arg(long)]
    lsp: bool,
    /// Ignored no-op alias for `--lsp`. vscode-languageclient, coc.nvim, and
    /// neovim's lspconfig all append `--stdio` when spawning an LSP server;
    /// accept it so `dl` drops into any client without extension-specific
    /// arg gymnastics. Stdio is the only transport either way.
    #[arg(long)]
    stdio: bool,
    /// Lint/ban mode: render the `diag` relation to stderr. Exit 0 clean, 2 if
    /// any `error`-severity row exists (Claude Code's blocking-hook code), 1 on
    /// a broken program. For pre-commit / CI / Claude Code hooks. See docs/rails.md.
    #[arg(long)]
    check: bool,
    /// Harness-hook mode: read a Claude Code hook event (PostToolUse JSON) on
    /// stdin, tick the rules, emit the hook output (additionalContext / block)
    /// on stdout. The program heads `inject`/`inject_skill`/`block` over the
    /// agent built-ins. The condition is a dl rule; no editor, no bash. See
    /// docs/skill-injection.md.
    #[arg(long)]
    hook: bool,
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
    /// Verify-rollback: run the program (applying `gen` edits), then run this
    /// shell command as a checker in the root. Keep the edits only if it exits
    /// 0; otherwise restore every touched file to its pre-run state and exit 1.
    /// Transactional codemod — apply, test, keep-if-pass. See christmas #14.
    #[arg(long)]
    verify: Option<String>,
    /// Profile mode (or DL_PROFILE=1): log slow SQL statements (threshold
    /// DL_PROFILE_SQL_MS, default 25), per-repo scan times, tick phase
    /// breakdown, and per-tick statement counts.
    #[arg(long)]
    profile: bool,
    /// Cap `cmd` invocations per tick (or DL_CMD_BUDGET); over budget is a loud
    /// error, never a silent truncation. Default: unlimited.
    #[arg(long)]
    cmd_budget: Option<u32>,
    /// After each tick, print every relation's row count (or DL_TICK_AUDIT=1).
    #[arg(long)]
    tick_audit: bool,
    /// Run as the long-lived daemon foreground (logs to stderr, ignores idle
    /// timeout). Usually invoked internally by spawn-if-missing; passing this
    /// flag explicitly is the debug path. See plans/2026-06-21-daemon-and-menu-bar.md.
    #[arg(long)]
    daemon: bool,
    /// With --daemon: spawn the menu bar tray icon (macOS v1; Windows/Linux
    /// deferred). The main thread runs the tray event loop; the accept loop
    /// moves off-main. Implies --daemon.
    #[arg(long)]
    tray: bool,
    /// Send `shutdown` to the daemon on `<root>/.dl/daemon.sock` and exit.
    #[arg(long)]
    stop: bool,
    /// Force the in-process path this invocation (do not auto-attach). Same as
    /// `DL_NO_DAEMON=1`. Useful when the daemon socket is wedged.
    #[arg(long)]
    no_daemon: bool,
    /// Load a script into the running daemon as a WATCHED program: joins the
    /// loaded set, runs on every tick, hot-reloads on edit. Omit `--root` to
    /// target the global rootless serving daemon.
    #[arg(long = "load")]
    load: Option<String>,
    /// Load a script ONE-TIME: eval it on a throwaway engine, print the `?`
    /// query results, persist nothing. Same target rules as `--load`.
    #[arg(long)]
    load_once: Option<String>,
}

/// Explicit `--root` wins (canonicalized). When `--tray` is on and no
/// `--root` is given, walk up from cwd to find the nearest `.dl/`
/// directory and use its parent — the tray auto-discovers its workspace.
/// Otherwise default to the repo the program file lives in (nearest `.git`
/// ancestor of its dir), falling back to the current directory.
fn resolve_root(cli: &Cli) -> Result<PathBuf> {
    if let Some(r) = &cli.root {
        return Ok(r.canonicalize()?);
    }
    if cli.tray {
        if let Some(root) = find_workspace_root() {
            return Ok(root);
        }
    }
    let base = cli.programs.first().map(|s| s.as_str())
        .and_then(|p| std::fs::canonicalize(p).ok())
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .map(Ok)
        .unwrap_or_else(std::env::current_dir)?;
    Ok(sprefa_v5::repo::nearest_git(&base).unwrap_or(base))
}

/// Walk up from cwd, looking for a `.dl/` directory. Return its parent
/// (the workspace root) if found. This lets `dl --tray` work from any
/// subdirectory of a workspace.
fn find_workspace_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    for dir in cwd.ancestors() {
        if dir.join(".dl").is_dir() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

/// Print a `--load` / `--load-once` RPC response: the JSON result on success,
/// the error message (exit 1) on failure.
fn print_load_response(resp: sprefa_v5::rpc::Response) -> Result<()> {
    if let Some(e) = resp.error {
        eprintln!("{}", e.message);
        std::process::exit(1);
    }
    if let Some(res) = resp.result {
        println!("{}", serde_json::to_string_pretty(&res)?);
    }
    Ok(())
}

fn main() -> Result<()> {
    sprefa_v5::trace::init();
    // `dl setup …` is a subcommand, not a `.dl` program: intercept it before
    // clap's flat positional parser would swallow "setup" as a program path.
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.first().map(String::as_str) == Some("setup") {
        std::process::exit(sprefa_v5::setup::run(&raw[1..])?);
    }
    if raw.first().map(String::as_str) == Some("examples") {
        std::process::exit(sprefa_v5::corpus::run(&raw[1..])?);
    }
    let cli = Cli::parse();
    if cli.profile { sprefa_v5::db::set_profile(true); }
    if cli.tick_audit { sprefa_v5::engine::set_tick_audit(true); }
    if let Some(n) = cli.cmd_budget { sprefa_v5::engine::set_cmd_budget(n); }
    if cli.no_daemon {
        // Propagate to children + the daemon module's enabled() check.
        std::env::set_var("DL_NO_DAEMON", "1");
    }
    // The daemon and `--stop` take `--root` as an OPTION: omitted = the
    // singleton rootless serving daemon at the XDG home, serving the config
    // folders. Move + one-shots still resolve a concrete root.
    let root_opt: Option<PathBuf> = match &cli.root {
        Some(r) => Some(r.canonicalize()?),
        None => None,
    };
    if cli.stop {
        return sprefa_v5::daemon::stop(root_opt.as_deref());
    }
    // `--load` / `--load-once`: push a script to the running daemon and exit.
    // watched joins the program (reactive); once evals ephemerally + prints.
    if let Some(p) = cli.load_once.clone() {
        let resp = sprefa_v5::daemon::load(root_opt.as_deref(), &p, "once")?;
        return print_load_response(resp);
    }
    if let Some(p) = cli.load.clone() {
        let resp = sprefa_v5::daemon::load(root_opt.as_deref(), &p, "watched")?;
        return print_load_response(resp);
    }
    if cli.daemon || cli.tray {
        return sprefa_v5::daemon::run_daemon(
            &cli.programs,
            cli.db.as_deref(),
            root_opt,
            true,
            cli.tray,
        );
    }
    let root = resolve_root(&cli)?;
    if !cli.move_.is_empty() {
        return sprefa_v5::run_move(cli.db.as_deref(), root, cli.repo, cli.move_, cli.fix);
    }
    // Discovery mode (no positional) defaults the db to <root>/.dl/cache.db so
    // repeated hook/check invocations get warm incremental ticks instead of a
    // cold in-memory rescan. With the daemon enabled, every mode (incl. --lsp)
    // defaults to the same cache so the daemon's writes are visible to the LSP
    // process via SQLite WAL. A generated .gitignore keeps the cache out of git.
    let mut db = cli.db;
    if db.is_none() {
        let dir = root.join(".dl");
        let daemon_on = sprefa_v5::daemon::enabled();
        let want_default = cli.programs.is_empty() || (daemon_on && (cli.lsp || cli.check || cli.diag_json));
        if want_default && dir.is_dir() {
            let gi = dir.join(".gitignore");
            if !gi.exists() { let _ = std::fs::write(&gi, "cache.db*\n"); }
            db = Some(dir.join("cache.db").to_string_lossy().into_owned());
        }
    }
    // One-shot modes consume a single program (or discovery when empty); only
    // the daemon merges multiple positionals today.
    let program = cli.programs.first().map(|s| s.as_str());
    if cli.lsp || cli.stdio {
        sprefa_v5::run_lsp(program, db.as_deref(), root)
    } else if cli.hook {
        // Harness-hook: stdin event -> tick -> stdout hook JSON. Exit 0 normally
        // (block rides the JSON), 1 if the program is broken (user-facing only).
        let code = sprefa_v5::hook::run_hook(program, db.as_deref(), root)?;
        std::process::exit(code);
    } else if cli.check || cli.diag_json {
        // Exit contract: 0 clean, 2 rail violations (Claude Code's blocking-hook
        // code; stderr feeds the agent), 1 broken program (user-facing).
        let errors = sprefa_v5::run_check(program, db.as_deref(), root, cli.diag_json)?;
        if errors > 0 {
            eprintln!("{errors} error-severity diagnostic(s) found");
            std::process::exit(2);
        }
        Ok(())
    } else if let Some(cmd) = cli.verify.as_deref() {
        // Transactional codemod: apply gen edits, run the checker, keep-if-pass.
        let kept = sprefa_v5::run_verify(program, db.as_deref(), root, cmd)?;
        if !kept { std::process::exit(1); }
        Ok(())
    } else if !cli.changed.is_empty() {
        sprefa_v5::run_changed(program, db.as_deref(), root, cli.changed)
    } else if cli.watch {
        sprefa_v5::run_watch(program, db.as_deref(), root)
    } else {
        sprefa_v5::run_file(program, db.as_deref(), root, cli.query_json)
    }
}
