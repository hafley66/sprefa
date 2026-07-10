//! The `dl` command line: the clap `Cli` struct, the pre-clap subcommand
//! intercepts (`setup`/`examples`/`index`/`doctor`/`docs`/`update`/`daemon`),
//! and the run-mode dispatch. `main` is a thin shell over [`run`].
//!
//! Layout: [`root`] resolves the working root, [`daemon`] owns the daemon
//! subcommand + shared output helpers, and this module wires the two together.

mod daemon;
mod inputs;
mod root;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// Trailer for `dl --help`: the pre-clap subcommands (clap never sees them, so
/// list them by hand) plus where to read more. Public so `dl docs search` can
/// index the command surface alongside the guides.
pub(crate) const SUBCOMMANDS_HELP: &str = "\
QUICK START:
  dl prog.dl                 run a program (root = cwd; auto-attaches a daemon)
  dl 'head <- body. ? q.'    run inline source (root = cwd, in-process)
  dl some/rails/             merge + run every *.dl in a folder, once
  dl                         discovery: merge + run every <cwd>/.dl/*.dl
  dl watch prog.dl           serve it reactively (daemon watches + hot-reloads)
  dl docs search <words>     search every guide + the CLI help
  dl prog.dl --parse-only    parse + typecheck, NO scan (sub-second fast fail)

ROOT & DAEMON (the two things that bite):
  ROOT is the cwd. There is NO --root flag: point dl at a repo by running it
    from that directory (or set DL_DAEMON_ROOT for a spawned daemon).
  A one-shot AUTO-ATTACHES to a per-root daemon and auto-restarts it when the
    binary changed. Control it with the `dl daemon` subcommand:
  dl daemon status           is it up? build_id, tick_count, settled, program
  dl daemon restart          after `cargo install`, respawn with the new binary
  dl daemon stop             shut it down
  dl daemon rows REL         print a relation's live rows from the daemon
  dl --no-daemon / DL_NO_DAEMON=1   force in-process, bypass the daemon entirely

 RULE BASICS:
   head <- body.              a rule; head is a fact/relation, body joins atoms
   diag(path: p, line: l)     KWARGS: name head columns by name (order-free);
                              a bare `diag(p, l)` fills by position. Mix freely.
   ? rel(a, b).               a query, printed after the tick
   use std/<name>             splice an embedded std lib OR example (real std
                             wins on clash; examples fill in the std/ namespace
                             — e.g. std/gh-checkout resolves the embedded example
                             of that name). Demo query lines strip on splice so
                             an example loads as a library, not a demo
   scan/match/ast/sg/json     source ops (extract facts); everything else derives

SUBCOMMANDS (run `dl <cmd> -h` for detail):
  daemon     control the background daemon (status/start/stop/restart/rows/...)
  setup      install the skill + wire agents, hooks, and the pre-commit rail
  update     self-update to the latest published release (--check to preview)
  examples   browse the embedded programs (list / search / --show / --std)
  index      turnkey SCIP: detect the language(s), run the right indexer
  doctor     SCIP health screen (index freshness, indexer availability)
  docs       read the embedded guides (reference pages + the book)

LEARN MORE:
  dl docs        topic list, then `dl docs syntax` or `dl docs book 1`
  dl examples    browse the runnable programs baked into the binary

AUTHORING RULES:
  dl setup --print   dump the rules survival guide + language matrix (the skill)
  dl docs authoring  read that same guide via the docs reader";

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
    /// Re-tick on file changes in the source root (in-process watcher, the
    /// pre-daemon path). For the warm long-lived watcher, use the daemon.
    #[arg(long, help_heading = "Run modes")]
    watch: bool,
    /// Run as an LSP server over stdio: the program's `diag` relation becomes
    /// live editor diagnostics (lint on open/save). See docs/lsp.md. Accepts
    /// `--stdio` as an alias (vscode-languageclient, coc.nvim, and neovim's
    /// lspconfig all append it when spawning an LSP server; stdio is the only
    /// transport either way). Self-override lets `--lsp --stdio` coexist: a
    /// client that passes the flag AND a client library that appends the alias
    /// must not kill the server with clap's duplicate-arg error.
    #[arg(long, alias = "stdio", overrides_with = "lsp", help_heading = "Run modes")]
    lsp: bool,
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
    /// configured repo. Omitted = the cwd repo (self).
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
    /// Force the in-process path this invocation (do not auto-attach). Same as
    /// `DL_NO_DAEMON=1`. Useful when the daemon socket is wedged.
    #[arg(long, help_heading = "Daemon")]
    no_daemon: bool,
    /// Run the network/mutating sinks (`repo` pulls, `checkout` sweeps) on this
    /// one-shot. By default a bare `dl prog.dl` is a pure READ: a `?` query never
    /// triggers a fetch/reset. The daemon's poll loop and `--watch`/`--settle`
    /// always drain on their cadence; this flag opts a one-shot in (so
    /// `dl gh-checkout.dl --apply` actually sweeps). Same as `DL_APPLY_SINKS=1`.
    #[arg(long, help_heading = "Daemon")]
    apply: bool,
}

/// The `dl` entry point. `main` is a one-liner over this.
pub fn run() -> Result<()> {
    crate::trace::init();
    crate::engine::init_thread_pool();
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = dispatch_subcommand(&raw)? {
        std::process::exit(code);
    }
    // `dl load <target>` is a synonym for the one-shot `dl <target>` (the `load`
    // keyword is optional). Strip it, then let clap parse the remainder.
    let rest: Vec<String> = if raw.first().map(String::as_str) == Some("load") {
        raw[1..].to_vec()
    } else {
        raw
    };
    let cli = Cli::parse_from(std::iter::once("dl".to_string()).chain(rest));
    apply_global_toggles(&cli);
    dispatch_mode(cli)
}

/// Pre-clap subcommand intercepts. clap's flat positional parser would swallow
/// `setup`/`examples`/... as a program path, so match them by hand first.
/// Returns `Some(exit_code)` if a subcommand ran, `None` to fall through.
fn dispatch_subcommand(raw: &[String]) -> Result<Option<i32>> {
    let Some(verb) = raw.first().map(String::as_str) else {
        return Ok(None);
    };
    let rest = &raw[1..];
    let code = match verb {
        "setup" => crate::setup::run(rest)?,
        "examples" => crate::corpus::run(rest)?,
        "index" => crate::scip_setup::run_index(rest)?,
        "doctor" => crate::scip_setup::run_doctor(rest)?,
        "docs" => crate::docs_cmd::run(rest)?,
        "update" => crate::update::run(rest)?,
        "daemon" => daemon::run_cmd(rest)?,
        "watch" => daemon::run_watch(rest)?,
        _ => return Ok(None),
    };
    Ok(Some(code))
}

/// Process-wide toggles set from flags before any dispatch.
fn apply_global_toggles(cli: &Cli) {
    if cli.profile {
        crate::db::set_profile(true);
    }
    if cli.tick_audit {
        crate::engine::set_tick_audit(true);
    }
    if let Some(n) = cli.cmd_budget {
        crate::engine::set_cmd_budget(n);
    }
    if cli.no_daemon {
        // Propagate to children + the daemon module's enabled() check.
        std::env::set_var("DL_NO_DAEMON", "1");
    }
    if cli.apply {
        // Opt this one-shot into draining the network/mutating sinks. The daemon
        // poll loop and --watch/--settle ignore this (they always drain on
        // cadence); run_file_inproc is the only consumer.
        std::env::set_var("DL_APPLY_SINKS", "1");
    }
}

/// The run-mode dispatch: pick the output adapter (default `?` rows, --check,
/// --lsp, --mcp, --hook, --settle, --watch, --move, ...) for a one-shot run.
fn dispatch_mode(cli: Cli) -> Result<()> {
    let root = root::resolve(&cli.programs)?;
    if !cli.move_.is_empty() {
        return crate::run_move(cli.db.as_deref(), root, cli.repo, cli.move_, cli.fix);
    }
    // Expand positionals: inline `'head <- body.'` source (materialized to a
    // temp file), a folder (`some/rails/` merges its `*.dl`), or a plain file.
    // Inline + folder runs are ad-hoc, so force in-process: the daemon serves
    // its own `.dl/*.dl` set, never a positional we hand it.
    let expanded = inputs::expand(&cli.programs)?;
    if expanded.ephemeral {
        std::env::set_var("DL_NO_DAEMON", "1");
    }
    let programs = expanded.files;
    // `--parse-only`: no scan, no db. Dispatch BEFORE the db-defaulting block so
    // it never opens (or `.gitignore`-writes into) `<root>/.dl/`.
    if cli.parse_only {
        std::process::exit(crate::run_parse_only(&programs, root)?);
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
        let daemon_on = crate::daemon::enabled();
        let want_default =
            programs.is_empty() || (daemon_on && (cli.lsp || cli.check || cli.diag_json));
        if want_default && dir.is_dir() {
            let gi = dir.join(".gitignore");
            if !gi.exists() {
                let _ = std::fs::write(&gi, "cache.db*\n");
            }
            db = Some(dir.join("cache.db").to_string_lossy().into_owned());
            db_defaulted = true;
        }
    }
    // Every one-shot mode takes the full (expanded) positional set: multiple
    // files merge into one program in the given order (a rail file beside the
    // program it watches). Empty = `.dl/*.dl` discovery inside resolve_programs.
    let programs = &programs;
    if cli.lsp {
        crate::run_lsp(programs, db.as_deref(), root)
    } else if cli.hook {
        // Harness-hook: stdin event -> tick -> stdout hook JSON. Exit 0 normally
        // (block rides the JSON), 1 if the program is broken (user-facing only).
        let code = crate::hook::run_hook(programs, db.as_deref(), root)?;
        std::process::exit(code);
    } else if cli.mcp {
        crate::mcp::run_mcp(programs, db.as_deref(), root)
    } else if cli.check || cli.diag_json {
        // Exit contract: 0 clean, 2 rail violations (Claude Code's blocking-hook
        // code; stderr feeds the agent), 1 broken program (user-facing).
        let errors = crate::run_check(programs, db.as_deref(), root, cli.diag_json)?;
        if errors > 0 {
            eprintln!("{errors} error-severity diagnostic(s) found");
            std::process::exit(2);
        }
        Ok(())
    } else if let Some(cmd) = cli.verify.as_deref() {
        // Transactional codemod: apply gen edits, run the checker, keep-if-pass.
        let kept = crate::run_verify(programs, db.as_deref(), root, cmd)?;
        if !kept {
            std::process::exit(1);
        }
        Ok(())
    } else if cli.settle {
        // In-process settle loop: drive ticks + effect drains to a fixpoint,
        // then print `?` once. Runs in-process (it owns the effect drain the
        // daemon otherwise owns), so no attach/spawn.
        crate::run_settle(
            programs,
            db.as_deref(),
            root,
            cli.settle_max.unwrap_or(200),
            cli.query_json,
        )
    } else if !cli.changed.is_empty() {
        crate::run_changed(programs, db.as_deref(), root, cli.changed)
    } else if cli.watch {
        crate::run_watch(programs, db.as_deref(), root)
    } else {
        crate::run_file(programs, db.as_deref(), db_defaulted, root, cli.query_json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// vscode-languageclient appends `--stdio` (the alias) even when the
    /// spawning extension already passed `--lsp`; both spellings on one command
    /// line must parse instead of dying with clap's duplicate-arg error.
    #[test]
    fn lsp_flag_tolerates_the_stdio_alias_appended_by_lsp_clients() {
        for argv in [
            vec!["dl", "--lsp"],
            vec!["dl", "--stdio"],
            vec!["dl", "--lsp", "--stdio"],
            vec!["dl", "--stdio", "--lsp"],
        ] {
            let cli = Cli::try_parse_from(&argv)
                .unwrap_or_else(|e| panic!("{argv:?} must parse: {e}"));
            assert!(cli.lsp, "{argv:?} must set lsp");
        }
    }
}
