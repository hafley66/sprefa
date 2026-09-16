//! The `dl` command line: the clap `Cli` struct, the pre-clap subcommand
//! intercepts (`setup`/`examples`/`index`/`doctor`/`docs`/`update`/`daemon`),
//! and the run-mode dispatch. `main` is a thin shell over [`run`].
//!
//! Layout: [`root`] resolves the working root, [`daemon_cmd`] owns the daemon
//! subcommand + shared output helpers, and this module wires the two together.

mod check_deadline;
mod daemon_cmd;
mod health;
mod inputs;
mod query;
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
    binary changed. There is ONE server code path — the daemon. Control it
    with the `dl daemon` subcommand:
  dl daemon status           is it up? build_id, tick_count, settled, program
  dl daemon restart          after `cargo install`, respawn with the new binary
  dl daemon stop             shut it down
  dl daemon rows REL         print a relation's live rows from the daemon

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
  what       resolve an anchor (name / glob / path / path:line) + its neighborhood
  summary    per-file report: entities, imports in/out, callable fan, doc coverage
  q          concept verbs: `dl q who-calls <name>` / `dl q where-defined <name>`
  daemon     control the background daemon (status/start/stop/restart/rows/...)
  setup      install, inspect, adopt, or undo agent wiring
  uninstall  undo setup wiring and remove dl state (binary remains)
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
#[command(name = "dl", version, about = "datalog over files in repo/rev/time space", after_help = SUBCOMMANDS_HELP)]
struct Cli {
    /// The .dl program(s) to run. Multiple files merge into one program (in the
    /// given order, with `use` includes spliced). When omitted, discovery: every
    /// `<root>/.dl/*.dl` file (lexicographic) merges instead. (--move synthesizes
    /// its own and ignores this.)
    programs: Vec<String>,
    /// Persist derived tables to a SQLite db at this path (default: in-memory;
    /// discovery mode defaults to the per-root
    /// `$XDG_STATE_HOME/sprefa/roots/<key>/db.sqlite` the daemon also serves).
    /// Derived relations land as plain-TEXT `rel_<name>` tables, queryable by
    /// anything that reads SQLite.
    #[arg(long, help_heading = "Output & storage")]
    db: Option<String>,
    /// Attach a file-scoped run to the REAL served-root db
    /// (`<state-home>/roots/<key>/db.sqlite`, the warm cache the daemon serves)
    /// instead of the default ephemeral in-memory db. Opt-in only: a bare
    /// `dl <file> --check`/`--diag-json`/`--lsp` defaults to `:memory:` so a
    /// read-shaped check never narrows the real analysis cache (failure-modes
    /// class 29). The no-positional discovery mode still attaches by default;
    /// this flag restores the warm path for a named file. `--db` (an explicit
    /// path) overrides both.
    #[arg(long, help_heading = "Output & storage")]
    attach: bool,
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
    #[arg(
        long,
        alias = "stdio",
        overrides_with = "lsp",
        help_heading = "Run modes"
    )]
    lsp: bool,
    /// M5.3: additive `--lsp` source mode. Instead of booting the dl engine,
    /// poll this sqlite file's `diag_v5` view (path, line, col, end_line,
    /// end_col, severity, code, msg, hint) on a 500ms cadence and republish
    /// its rows as LSP diagnostics, grouped by path. The db need not exist
    /// yet at startup (the poller retries the open). No engine boot happens
    /// in this mode; the default `--lsp` behavior without this flag is
    /// unchanged. The view is created and maintained by the node-side v6
    /// runtime.
    #[arg(long, value_name = "PATH", help_heading = "Run modes")]
    diag_db: Option<PathBuf>,
    /// Lint/ban mode: render the `diag` relation to stderr. Exit 0 clean, 2 if
    /// any `error`-severity row exists (Claude Code's blocking-hook code), 1 on
    /// a broken program. For pre-commit / CI / Claude Code hooks. See docs/rails.md.
    #[arg(long, help_heading = "Run modes")]
    check: bool,
    /// Harness-hook mode: read a coding-agent hook event (PostToolUse /
    /// UserPromptSubmit JSON) on stdin, tick the rules, emit the hook output
    /// (additionalContext / block) on stdout. The program heads
    /// `inject`/`inject_skill`/`block` over the agent built-ins. The condition
    /// is a dl rule; no editor, no bash. See docs/skill-injection.md.
    #[arg(long, help_heading = "Run modes")]
    hook: bool,
    /// With --hook: which harness's hook JSON to read on stdin and write on
    /// stdout — claude, codex, or opencode. Omitted = auto-detect (claude when
    /// the payload carries hook_event_name; opencode on the neutral {kind,...}
    /// shape sent by the shipped opencode plugin). codex is byte-compatible
    /// with claude and is only selected explicitly, never auto-detected.
    #[arg(long, help_heading = "Run modes")]
    dialect: Option<String>,
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
    /// R7 diag routing stage whose diagnostics to surface — `live` | `commit` |
    /// `agent-turn` | `agent-session`. Default `commit` (the pre-commit /
    /// --check surface, unchanged). A code with no `diag_stage` row routes by
    /// severity: an error reaches every stage, a warning only `commit`. Rails
    /// opt a code into other stages by heading `diag_stage(code, stage)`.
    #[arg(long, help_heading = "Run modes")]
    stage: Option<String>,
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
    /// Emit `?` query results as JSON instead of the human TSV block: one
    /// JSON array of row-objects (each row keyed by column name), one array
    /// per query. Only `json` is recognized today; any other value (or
    /// omitting the flag) leaves the default text output — or `--query-json`
    /// unchanged — in place. A multi-`?` program under `--format json` prints
    /// one array per query with no per-line tag naming which query it is; use
    /// `--query-json` instead if that matters.
    #[arg(long, help_heading = "Run modes", value_name = "FORMAT")]
    format: Option<String>,
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
    /// Wall-clock deadline for `--check` in seconds. On expiry prints a loud
    /// partial report plus a check-timed-out warning and exits 0.
    #[arg(long, help_heading = "Perf & debug")]
    max_wall: Option<u64>,
    /// After each tick, print every relation's row count (or DL_TICK_AUDIT=1).
    #[arg(long, help_heading = "Perf & debug")]
    tick_audit: bool,
    /// INTERNAL-ONLY (user directive 2026-07-18: one server code path; the
    /// public --no-daemon split is erased). Force the in-process path this
    /// invocation (do not auto-attach); same as `DL_NO_DAEMON=1`. Still parsed
    /// — the test suite and daemon-spawned children ride it — but hidden from
    /// `--help` and no longer part of the documented surface.
    #[arg(long, hide = true)]
    no_daemon: bool,
    /// Run the network/mutating sinks (`repo` pulls, `checkout` sweeps) on this
    /// one-shot. By default a bare `dl prog.dl` is a pure READ: a `?` query never
    /// triggers a fetch/reset. The daemon's poll loop and `--watch`/`--settle`
    /// always drain on their cadence; this flag opts a one-shot in (so
    /// `dl gh-checkout.dl --apply` actually sweeps). Same as `DL_APPLY_SINKS=1`.
    #[arg(long, help_heading = "Daemon")]
    apply: bool,
}

/// `true` for the two invocations that call `daemon::run_daemon` IN this
/// process (`dl daemon serve`, and `dl daemon start` with `--foreground` or
/// `--tray`, per `src/cli/daemon_cmd.rs`'s own foreground check) — these install
/// their own combined stderr+file tracing subscriber
/// (`daemon::init_daemon_tracing`) and must win the global `try_init` race,
/// so `crate::trace::init` must not claim the slot first for them. Every
/// other invocation, including a bare `dl daemon start` (which detaches a
/// child and returns), gets `trace::init`'s file-only subscriber.
fn is_daemon_foreground(raw: &[String]) -> bool {
    if raw.first().map(String::as_str) != Some("daemon") {
        return false;
    }
    match raw.get(1).map(String::as_str) {
        Some("serve") => true,
        Some("start") => raw[2..]
            .iter()
            .any(|a| a == "--foreground" || a == "--tray"),
        _ => false,
    }
}

/// The `dl` entry point. `main` is a one-liner over this.
// ARCH {"url":"10-cli","role":"dispatch"}
pub fn run() -> Result<()> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    crate::trace::init(is_daemon_foreground(&raw));
    crate::engine::init_thread_pool();
    // Global invocation log (the access-log twin of `why.jsonl`): every
    // invocation logs, including `--no-daemon`, `daemon why`, hooks, clients.
    // `argv` is the FULL process args (`std::env::args()`, unlike `raw` which
    // already skipped argv[0]) so the recorded command line is copy-pasteable.
    let full_argv: Vec<String> = std::env::args().collect();
    let inv_id = crate::invlog::record_start(&full_argv);
    // Process start seam: fires on EVERY invocation regardless of whether it
    // ever ticks an engine (e.g. `dl daemon why`), so `dl.log` always carries
    // at least this one line per process — the guarantee the log-files test
    // relies on.
    tracing::info!(pid = std::process::id(), argv = %full_argv.join(" "), "[process] start");
    // Standing law: nothing seizes the machine. Every one-shot CLI mode runs
    // under the same CPU/IO budget the daemon uses. The daemon-serve verb
    // (`dl daemon ...` -> run_daemon) applies its own budget with a thread cap,
    // so it is skipped here; a double-apply would be harmless regardless (the
    // syscalls are idempotent), but skipping keeps the debug line honest about
    // which path set the budget.
    if raw.first().map(String::as_str) != Some("daemon") {
        apply_cli_budget();
    }
    if let Some(code) = dispatch_subcommand(&raw)? {
        // A SIGKILL between here and `record_end` leaves the row open
        // (ts_end_ms NULL) — that IS the kill evidence `invlog::report_recent`
        // flags. `std::process::exit` does not run `Drop`, so this call must
        // be explicit, not an RAII guard.
        crate::invlog::record_end(inv_id, code);
        tracing::info!(pid = std::process::id(), code, "[process] end");
        // Same "process::exit skips Drop" reasoning as invlog::record_end just
        // above: close the chrome trace's `]` explicitly. No-op if
        // DL_TRACE_CHROME was never set, or a nested `daemon serve` already
        // closed it in `shutdown_cleanup`.
        crate::trace::finish_chrome_trace();
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
    let result = dispatch_mode(cli);
    let exit_code = match &result {
        Ok(()) => 0,
        Err(_) => 1,
    };
    crate::invlog::record_end(inv_id, exit_code);
    tracing::info!(pid = std::process::id(), code = exit_code, "[process] end");
    // The normal (non-`process::exit`) return path: Rust's own runtime exit
    // after `main` returns WOULD run `Drop`, but the chrome guard is only
    // reachable process-globally (see `trace::finish_chrome_trace`'s doc), so
    // this call is still explicit rather than relying on that.
    crate::trace::finish_chrome_trace();
    result
}

/// Apply the process CPU/disk-I/O scheduling budget for a one-shot CLI run
/// (standing law: nothing seizes the machine). `DL_NO_BUDGET=1` skips it for
/// benchmarking and prints nothing; `DL_BUDGET_DEBUG=1` prints one verifiable
/// stderr line so an external observer can confirm the budget was applied.
fn apply_cli_budget() {
    if std::env::var("DL_NO_BUDGET").ok().as_deref() == Some("1") {
        return;
    }
    crate::daemon::apply_process_budget();
    // One-shot runs stay un-governed by default (interactive latency);
    // DL_MAX_CPU_PCT opts a run into the same hard ceiling the daemon gets.
    crate::budget::start_governor(crate::budget::ceiling_from_env(0));
    if std::env::var("DL_BUDGET_DEBUG").ok().as_deref() == Some("1") {
        let (qos, iopol) = if cfg!(target_os = "macos") {
            ("utility", "throttle")
        } else {
            ("none", "none")
        };
        let nice = if cfg!(unix) { 10 } else { 0 };
        let threads = rayon::current_num_threads();
        // @eprintln-ok: DL_BUDGET_DEBUG env-gated debug line is the command's stderr output contract
        eprintln!("[budget] qos={qos} nice={nice} iopol={iopol} threads={threads}");
    }
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
        "uninstall" => crate::setup::uninstall()?,
        "examples" => crate::corpus::run(rest)?,
        "index" => crate::scip_setup::run_index(rest)?,
        "doctor" => crate::scip_setup::run_doctor(rest)?,
        "docs" => crate::docs_cmd::run(rest)?,
        "update" => crate::update::run(rest)?,
        "daemon" => daemon_cmd::run_cmd(rest)?,
        "watch" => daemon_cmd::run_watch(rest)?,
        "what" => query::run_what(rest)?,
        "summary" => query::run_summary(rest)?,
        "q" => query::run_q(rest)?,
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
        let code = crate::run_parse_only(&programs, root)?;
        crate::trace::finish_chrome_trace();
        std::process::exit(code);
    }
    // Attaching to the REAL served-root db (`<state-home>/roots/<key>/db.sqlite`,
    // storage-endgame L2, one db per corpus) is OPT-IN: only the no-positional
    // discovery mode, or an explicit `--attach`, may default onto it.
    //
    // Failure-modes class 29: a file-scoped read-shaped run
    // (`--check`/`--diag-json`/`--lsp` WITH a positional) must NEVER default
    // onto the real db — the old `want_default` heuristic conflated "user asked
    // for a check" with "attach to the served root" and let a worktree's partial
    // scan narrow an ~860MB analysis cache to ~0.6MB. Such a run now defaults to
    // an ephemeral in-memory db. Crucially the default is a CONCRETE `:memory:`,
    // not `None`: `run_file`/`run_check` daemon-eligibility keys on
    // `db_path.is_none() || db_defaulted`, so a concrete `:memory:` also keeps
    // the run off the daemon that would otherwise tick+write the real served db.
    //
    // Discovery mode (`programs.is_empty()`) still defaults to the real root db
    // and stays daemon-eligible (`db_defaulted`): a warm daemon is preferred,
    // and when both worlds touch the file (daemon up, attach failed) WAL +
    // busy_timeout in db.rs is the shared-access discipline. `--db` outranks all.
    let mut db = cli.db;
    let mut db_defaulted = false;
    if db.is_none() {
        let dir = root.join(".dl");
        let attach_served = programs.is_empty() || cli.attach;
        if attach_served && dir.is_dir() {
            let root_db = crate::daemon::root_db_path(&root);
            if let Some(parent) = root_db.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            db = Some(root_db.to_string_lossy().into_owned());
            db_defaulted = true;
        } else if cli.check || cli.diag_json || cli.lsp {
            // File-scoped read-shaped run, not attaching: ephemeral + hermetic.
            db = Some(":memory:".to_string());
        }
    }
    // Every one-shot mode takes the full (expanded) positional set: multiple
    // files merge into one program in the given order (a rail file beside the
    // program it watches). Empty = `.dl/*.dl` discovery inside resolve_programs.
    let programs = &programs;
    // `--format json` wins over `--query-json` when both are given; either
    // alone selects its shape; neither leaves the default text block. See
    // `QueryOutputFormat`'s doc for what each variant prints.
    let query_format = if cli.format.as_deref() == Some("json") {
        crate::engine::QueryOutputFormat::JsonRows
    } else if cli.query_json {
        crate::engine::QueryOutputFormat::Ndjson
    } else {
        crate::engine::QueryOutputFormat::Text
    };
    if cli.lsp {
        crate::run_lsp(programs, db.as_deref(), db_defaulted, root, cli.diag_db)
    } else if cli.hook {
        // Harness-hook: stdin event -> tick -> stdout hook JSON. Exit 0 normally
        // (block rides the JSON), 1 if the program is broken (user-facing only).
        let dialect = cli
            .dialect
            .as_deref()
            .map(crate::hook::HookDialect::from_flag)
            .transpose()?;
        let code = crate::hook::run_hook(programs, db.as_deref(), root, dialect)?;
        // This arm exits without returning to `run`, which would skip its
        // `record_end` — close the invocation row here so a hook run (deadline
        // give-up included) is invlog-visible as a finished exit, not an open
        // row that reads as a kill. Also required to shed an abandoned deadline
        // worker: `std::process::exit` is what tears its thread down.
        crate::invlog::record_end_current(code);
        tracing::info!(pid = std::process::id(), code, "[process] end");
        crate::trace::finish_chrome_trace();
        std::process::exit(code);
    } else if cli.mcp {
        crate::mcp::run_mcp(programs, db.as_deref(), root)
    } else if cli.check || cli.diag_json {
        // Class-14 rail: pre-commit `--check` in a linked worktree with no warm
        // db refuses the cold build (each agent-worktree commit was minting a
        // ~600MB orphan under roots/). Green-by-skip with a loud note; the
        // explicit `--db` form is exempt (the caller named a target on purpose).
        // `cli.db` moved into `db` above; "no explicit --db" is exactly
        // `db.is_none() || db_defaulted` here.
        if db.is_none() || db_defaulted {
            if let Some(reason) = crate::hook::refuse_worktree_cold_check(&root) {
                eprintln!("[check] {reason}"); // @eprintln-ok: final user-facing skip note at CLI top level
                tracing::warn!(root = %root.display(), "class-14 worktree cold-build refused");
                crate::trace::finish_chrome_trace();
                std::process::exit(0);
            }
        }
        // Class-13 stale-binary rail: `--check` is the third named surface
        // (docs/failure-modes.md:300-327) — the pre-commit/agent-turn hook
        // path that answered from a stale install during freeze #1.
        crate::stale_binary::warn_if_stale(&root);
        // Exit contract: 0 clean, 2 rail violations (Claude Code's blocking-hook
        // code; stderr feeds the agent), 1 broken program (user-facing).
        // R7: resolve the routing stage (default the pre-commit surface); reject
        // an unknown name loudly rather than silently surfacing nothing.
        let stage = cli
            .stage
            .as_deref()
            .unwrap_or(crate::stage::DEFAULT_STAGE)
            .to_string();
        if !crate::stage::is_stage(&stage) {
            anyhow::bail!(
                "unknown --stage `{stage}` (expected live, commit, agent-turn, or agent-session)"
            );
        }
        // `--max-wall` marks the hook surface: its no-daemon fallback reads the
        // shared root db read-only (storage-endgame L2) — a hook never
        // cold-builds and never contends for the daemon's write lock.
        let errors = if let Some(max_wall) = cli.max_wall {
            let Some(errors) = check_deadline::run(max_wall, {
                let programs = programs.to_vec();
                let db = db.clone();
                let root = root.clone();
                let json = cli.diag_json;
                let stage = stage.clone();
                move || {
                    crate::run_check(
                        &programs,
                        db.as_deref(),
                        db_defaulted,
                        root,
                        json,
                        &stage,
                        true,
                    )
                }
            })?
            else {
                return Ok(());
            };
            errors
        } else {
            crate::run_check(
                programs,
                db.as_deref(),
                db_defaulted,
                root,
                cli.diag_json,
                &stage,
                false,
            )?
        };
        if errors > 0 {
            eprintln!("{errors} error-severity diagnostic(s) found"); // @eprintln-ok: final user-facing error count before exit 2
            crate::trace::finish_chrome_trace();
            std::process::exit(2);
        }
        Ok(())
    } else if let Some(cmd) = cli.verify.as_deref() {
        // Transactional codemod: apply gen edits, run the checker, keep-if-pass.
        let kept = crate::run_verify(programs, db.as_deref(), root, cmd)?;
        if !kept {
            crate::trace::finish_chrome_trace();
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
            query_format,
        )
    } else if !cli.changed.is_empty() {
        crate::run_changed(programs, db.as_deref(), root, cli.changed)
    } else if cli.watch {
        crate::run_watch(programs, db.as_deref(), root)
    } else {
        crate::run_file(programs, db.as_deref(), db_defaulted, root, query_format)
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
            let cli =
                Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{argv:?} must parse: {e}"));
            assert!(cli.lsp, "{argv:?} must set lsp");
        }
    }
}
