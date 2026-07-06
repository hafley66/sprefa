use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// Trailer for `dl --help`: the five pre-clap subcommands (clap never sees
/// them, so list them by hand) plus where to read more.
const SUBCOMMANDS_HELP: &str = "\
SUBCOMMANDS (run `dl <cmd> -h` for detail):
  setup      install the skill + wire agents, hooks, and the pre-commit rail
  examples   browse the embedded programs (list / search / --show / --std)
  index      turnkey SCIP: detect the language(s), run the right indexer
  doctor     SCIP health screen (index freshness, indexer availability)
  docs       read the embedded guides (reference pages + the book)

LEARN MORE:
  dl docs        topic list, then `dl docs syntax` or `dl docs book 1`
  dl examples    browse the runnable programs baked into the binary

AUTHORING RULES:
  dl setup --print   dump the rules survival guide + language matrix (the skill)
  dl docs authoring  read that same guide via the docs reader
  dl PROG --parse-only   parse + typecheck a program, no scan (the fast fail)";

#[derive(Parser)]
#[command(name = "dl", about = "datalog over files in repo/rev/time space", after_help = SUBCOMMANDS_HELP)]
struct Cli {
    /// The .dl program(s) to run. Multiple files merge into one program (in the
    /// given order, with `use` includes spliced). When omitted, discovery: every
    /// `<root>/.dl/*.dl` file (lexicographic) merges instead. (--move synthesizes
    /// its own and ignores this.)
    programs: Vec<String>,
    /// Persist derived tables to a SQLite db at this path (default: in-memory;
    /// discovery mode defaults to `<root>/.dl/cache.db`). Derived relations land
    /// as plain-TEXT `rel_<name>` tables, queryable by anything that reads SQLite.
    #[arg(long, help_heading = "Output & storage")]
    db: Option<String>,
    /// Source root. When omitted, defaults to the nearest `.git` ancestor of
    /// the program file (the repo it lives in), else the current directory.
    #[arg(long, help_heading = "Output & storage")]
    root: Option<PathBuf>,
    /// Re-tick on file changes in the source root (in-process watcher, the
    /// pre-daemon path). For the warm long-lived watcher, use `--daemon`.
    #[arg(long, help_heading = "Run modes")]
    watch: bool,
    /// Run as an LSP server over stdio: the program's `diag` relation becomes
    /// live editor diagnostics (lint on open/save). See docs/lsp.md.
    #[arg(long, help_heading = "Run modes")]
    lsp: bool,
    /// Ignored no-op alias for `--lsp`. vscode-languageclient, coc.nvim, and
    /// neovim's lspconfig all append `--stdio` when spawning an LSP server;
    /// accept it so `dl` drops into any client without extension-specific
    /// arg gymnastics. Stdio is the only transport either way.
    #[arg(long, help_heading = "Run modes")]
    stdio: bool,
    /// Lint/ban mode: render the `diag` relation to stderr. Exit 0 clean, 2 if
    /// any `error`-severity row exists (Claude Code's blocking-hook code), 1 on
    /// a broken program. For pre-commit / CI / Claude Code hooks. See docs/rails.md.
    #[arg(long, help_heading = "Run modes")]
    check: bool,
    /// Harness-hook mode: read a Claude Code hook event (PostToolUse JSON) on
    /// stdin, tick the rules, emit the hook output (additionalContext / block)
    /// on stdout. The program heads `inject`/`inject_skill`/`block` over the
    /// agent built-ins. The condition is a dl rule; no editor, no bash. See
    /// docs/skill-injection.md.
    #[arg(long, help_heading = "Run modes")]
    hook: bool,
    /// Serve the program as an MCP (JSON-RPC stdio, newline-delimited) server:
    /// binds the program's rpc-class ports to stdio. Each inbound request
    /// injects into the `@in(rpc)` rel (envelope id, method, params), runs a
    /// tick, and the `@out(rpc)` rel's rows (id, result) drain back as
    /// responses. Dispatch is a lattice rel (`key(id) merge(MaxBy(prio))`).
    /// See examples/mcp-echo.dl.
    #[arg(long, help_heading = "Run modes")]
    mcp: bool,
    /// Like --check but emit the diagnostics as a JSON array on stdout.
    #[arg(long, help_heading = "Run modes")]
    diag_json: bool,
    /// Parse + typecheck + op resolution + metavar sanity over the program
    /// file(s), with NO scan and NO db writes (sub-second). The authoring
    /// fast-fail: a parse/type error surfaces without paying a full scan. Exit 0
    /// clean, 1 on any error; diagnostics render to stderr in the --check style.
    #[arg(long, help_heading = "Run modes")]
    parse_only: bool,
    /// Emit `?` query results as JSON-lines (one object per query:
    /// {query, columns, rows, count}) instead of the human TSV block.
    #[arg(long, help_heading = "Run modes")]
    query_json: bool,
    /// Run in-process until the program SETTLES: drive ticks (draining `@async`/
    /// `sh`/`sh*` effects off-tick, the way the daemon does) until no non-timer
    /// relation moves, no `@next` carry is pending, and no effect is in-flight —
    /// then print `?` results once. Guarantees every cascade (effects, demand
    /// hops, repo-sink pulls) ran at least once. Bails loudly if the program
    /// cannot settle within --settle-max ticks. See plans/2026-07-06-settle-quiescence.md.
    #[arg(long, help_heading = "Run modes")]
    settle: bool,
    /// Tick budget for --settle (default 200). A program that has not settled by
    /// this many ticks bails, naming the relations/effects still moving.
    #[arg(long, help_heading = "Run modes")]
    settle_max: Option<usize>,
    /// Drive one incremental tick for these changed paths (the delta path the
    /// watcher uses), instead of a full run. Repeatable.
    #[arg(long, help_heading = "Run modes")]
    changed: Vec<PathBuf>,
    /// Auto-refactor: rewrite `use`-path references for a module move
    /// `OLD_FILE=NEW_FILE` (repo-relative Rust paths). Dry-run unless --fix.
    /// Repeatable. Ignores the `program` positional.
    #[arg(long = "move", help_heading = "Refactor")]
    move_: Vec<String>,
    /// With --move, which repo to rewrite: a config slug, or `*`/`all` for every
    /// configured repo. Omitted = the --root repo (self).
    #[arg(long, help_heading = "Refactor")]
    repo: Option<String>,
    /// With --move, write the rewritten files instead of previewing.
    #[arg(long, help_heading = "Refactor")]
    fix: bool,
    /// Verify-rollback: run the program (applying `gen` edits), then run this
    /// shell command as a checker in the root. Keep the edits only if it exits
    /// 0; otherwise restore every touched file to its pre-run state and exit 1.
    /// Transactional codemod — apply, test, keep-if-pass. See christmas #14.
    #[arg(long, help_heading = "Refactor")]
    verify: Option<String>,
    /// Profile mode (or DL_PROFILE=1): log slow SQL statements (threshold
    /// DL_PROFILE_SQL_MS, default 25), per-repo scan times, tick phase
    /// breakdown, and per-tick statement counts.
    #[arg(long, help_heading = "Perf & debug")]
    profile: bool,
    /// Cap `cmd` invocations per tick (or DL_CMD_BUDGET); over budget is a loud
    /// error, never a silent truncation. Default: unlimited.
    #[arg(long, help_heading = "Perf & debug")]
    cmd_budget: Option<u32>,
    /// After each tick, print every relation's row count (or DL_TICK_AUDIT=1).
    #[arg(long, help_heading = "Perf & debug")]
    tick_audit: bool,
    /// Run as the long-lived daemon foreground (logs to stderr, ignores idle
    /// timeout). Usually invoked internally by spawn-if-missing; passing this
    /// flag explicitly is the debug path. See plans/2026-06-21-daemon-and-menu-bar.md.
    #[arg(long, help_heading = "Daemon")]
    daemon: bool,
    /// With --daemon: spawn the menu bar tray icon (macOS v1; Windows/Linux
    /// deferred). The main thread runs the tray event loop; the accept loop
    /// moves off-main. Implies --daemon.
    #[arg(long, help_heading = "Daemon")]
    tray: bool,
    /// Send `shutdown` to the daemon on `<root>/.dl/daemon.sock` and exit.
    #[arg(long, help_heading = "Daemon")]
    stop: bool,
    /// Block until the running daemon reports the program QUIESCENT (every
    /// cascade — effects, demand hops, repo pulls — has run at least once), then
    /// print `settled=<bool> tick=<n>` and exit 0 (settled) or 3 (timed out).
    /// The daemon-side twin of `--settle`. Omit `--root` for the rootless daemon.
    #[arg(long, help_heading = "Daemon")]
    await_settle: bool,
    /// Timeout in ms for `--await-settle` (default 30000).
    #[arg(long, help_heading = "Daemon")]
    await_settle_ms: Option<u64>,
    /// Force the in-process path this invocation (do not auto-attach). Same as
    /// `DL_NO_DAEMON=1`. Useful when the daemon socket is wedged.
    #[arg(long, help_heading = "Daemon")]
    no_daemon: bool,
    /// Load a script into the running daemon as a WATCHED program: joins the
    /// loaded set, runs on every tick, hot-reloads on edit. Omit `--root` to
    /// target the global rootless serving daemon.
    #[arg(long = "load", help_heading = "Daemon")]
    load: Option<String>,
    /// Load a script ONE-TIME: eval it on a throwaway engine, print the `?`
    /// query results, persist nothing. Same target rules as `--load`.
    #[arg(long, help_heading = "Daemon")]
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
    // `dl index` / `dl doctor`: turnkey SCIP generation + health screen. Same
    // pre-clap intercept as `setup`/`examples` (a bare positional would otherwise
    // be read as a program path).
    if raw.first().map(String::as_str) == Some("index") {
        std::process::exit(sprefa_v5::scip_setup::run_index(&raw[1..])?);
    }
    if raw.first().map(String::as_str) == Some("doctor") {
        std::process::exit(sprefa_v5::scip_setup::run_doctor(&raw[1..])?);
    }
    // `dl docs [topic]`: read the guides embedded at build time (reference
    // pages + book chapters), so a bare prebuilt binary is self-documenting.
    // Same pre-clap intercept as `examples`.
    if raw.first().map(String::as_str) == Some("docs") {
        std::process::exit(sprefa_v5::docs_cmd::run(&raw[1..])?);
    }
    // `dl update`: self-update to the latest published release (turnkey; reuses
    // the cargo-dist release installer). Same pre-clap intercept as the others.
    if raw.first().map(String::as_str) == Some("update") {
        std::process::exit(sprefa_v5::update::run(&raw[1..])?);
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
    if cli.await_settle {
        let (settled, tick) = sprefa_v5::daemon::await_quiescent(
            root_opt.as_deref(), cli.await_settle_ms.unwrap_or(30_000))?;
        println!("settled={settled} tick={tick}");
        if !settled { std::process::exit(3); }
        return Ok(());
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
    // `--parse-only`: no scan, no db. Dispatch BEFORE the db-defaulting block so
    // it never opens (or `.gitignore`-writes into) `<root>/.dl/`.
    if cli.parse_only {
        std::process::exit(sprefa_v5::run_parse_only(&cli.programs, root)?);
    }
    // Discovery mode (no positional) defaults the db to <root>/.dl/cache.db so
    // repeated hook/check invocations get warm incremental ticks instead of a
    // cold in-memory rescan. With the daemon enabled, every mode (incl. --lsp)
    // defaults to the same cache so the daemon's writes are visible to the LSP
    // process via SQLite WAL. A generated .gitignore keeps the cache out of git.
    let mut db = cli.db;
    let mut db_defaulted = false;
    if db.is_none() {
        let dir = root.join(".dl");
        let daemon_on = sprefa_v5::daemon::enabled();
        let want_default = cli.programs.is_empty() || (daemon_on && (cli.lsp || cli.check || cli.diag_json));
        if want_default && dir.is_dir() {
            let gi = dir.join(".gitignore");
            if !gi.exists() { let _ = std::fs::write(&gi, "cache.db*\n"); }
            db = Some(dir.join("cache.db").to_string_lossy().into_owned());
            db_defaulted = true;
        }
    }
    // Every one-shot mode takes the full positional set: multiple files merge
    // into one program in the given order (a rail file beside the program it
    // watches). Empty = `.dl/*.dl` discovery inside resolve_programs.
    let programs = &cli.programs;
    if cli.lsp || cli.stdio {
        sprefa_v5::run_lsp(programs, db.as_deref(), root)
    } else if cli.hook {
        // Harness-hook: stdin event -> tick -> stdout hook JSON. Exit 0 normally
        // (block rides the JSON), 1 if the program is broken (user-facing only).
        let code = sprefa_v5::hook::run_hook(programs, db.as_deref(), root)?;
        std::process::exit(code);
    } else if cli.mcp {
        sprefa_v5::mcp::run_mcp(programs, db.as_deref(), root)
    } else if cli.check || cli.diag_json {
        // Exit contract: 0 clean, 2 rail violations (Claude Code's blocking-hook
        // code; stderr feeds the agent), 1 broken program (user-facing).
        let errors = sprefa_v5::run_check(programs, db.as_deref(), root, cli.diag_json)?;
        if errors > 0 {
            eprintln!("{errors} error-severity diagnostic(s) found");
            std::process::exit(2);
        }
        Ok(())
    } else if let Some(cmd) = cli.verify.as_deref() {
        // Transactional codemod: apply gen edits, run the checker, keep-if-pass.
        let kept = sprefa_v5::run_verify(programs, db.as_deref(), root, cmd)?;
        if !kept { std::process::exit(1); }
        Ok(())
    } else if cli.settle {
        // In-process settle loop: drive ticks + effect drains to a fixpoint,
        // then print `?` once. Runs in-process (it owns the effect drain the
        // daemon otherwise owns), so no attach/spawn.
        sprefa_v5::run_settle(programs, db.as_deref(), root,
            cli.settle_max.unwrap_or(200), cli.query_json)
    } else if !cli.changed.is_empty() {
        sprefa_v5::run_changed(programs, db.as_deref(), root, cli.changed)
    } else if cli.watch {
        sprefa_v5::run_watch(programs, db.as_deref(), root)
    } else {
        sprefa_v5::run_file(programs, db.as_deref(), db_defaulted, root, cli.query_json)
    }
}
