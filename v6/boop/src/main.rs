//! `boop`: the cross-harness agent-event reader, 1-1 with `bus` plus the four
//! verbs `bus` cannot do (read what an agent did, and measure what its
//! processes cost). The CLI routes to layers 0-3; it contains no `match` on
//! harness id and no direct `Command::new("tmux")` beyond the layer-1 helpers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use boop::bus::Route;
use boop::event::AgentEvent;
use boop::proc::ProcReader;
use boop::registry::Registry;
use boop::{bus, ident, identity, proc, query, tmux, usage};

const DOCTRINE: &str = "\
DOCTRINE (this help is the usage contract; agents read it with `boop --help`):

SPAWN: every lane spawn goes through lane create; bare tmux spawns leave no
edge and stay invisible to tracking:
    boop beep lane create --lane <id> --cwd <repo> --brief <abs-path> \\
      [--parent <coordinator>] [--branch <b> --base-sha <sha>] \\
      [--model <m>] [--tmux <name>] [--mail-dir <d>] [--dry-run]
  One shot: worktree at base sha + spawn + route registration.
  Always --dry-run first; the printed `cmd:` line is the literal spawn.

COMPLETION: --parent appends an on-exit hail `lane <id> done rc=$rc` into the
  parent's mailbox. A lane spawned with --parent reports completion; do not poll.

LIVENESS: a lane can die silently, producing nothing. Liveness is TWO checks:
    1. process alive:    boop beep ps <lane>
    2. worktree changed: git -C <worktree> status --short
  A REPORT.md at the root alone proves nothing; check its mtime and first line
  against the lane you dispatched.

HAIL: boop beep hail <lane> --body \"text\" [--from <me>] [--kind <k>]
  Injects keystrokes AND reports whether they landed. `opencode run` lanes take
  their prompt from ARGV, so a mid-flight hail reaches nothing: let the lane
  finish and re-dispatch with its session id, or kill it. Only interactive TUIs
  receive mid-flight hails.

ACK: boop beep message ack is age-based bulk-mark, NOT proof-of-read. An ack
  proves read at best, never compliance; compliance = the work's own artifacts.

ROUTE: session id for a lane: boop beep lane route <lane> (route cwd = the
  lane's worktree). Mailbox: ~/.agent/mail/ (bus.ndjson + registry.json),
  override with --mail-dir.

STORE SCHEMA: this build writes version 5. A store written by an older build is
refused, and `boop db sync create --rebuild` drops every stored row and
re-projects every transcript from byte 0 (about 18 s over 1.5 GB here). Nothing
is wiped without that flag.

SQL: the store is SQLite at ~/.agent/boop.db; `boop db \"<sql>\"` queries it
  read-only. sqlite3 dot-commands (.schema, .tables) are NOT supported; the
  passthrough takes plain SQL only.

The pre-split verbs (harnesses, sessions, events, chat, tail, list, measure,
dispatch, lane, resolve, adopt, sweep, prune, hail, sync, follow) still run as
hidden aliases for one release. Use `beep` and `db`.";

#[derive(Parser)]
#[command(
    name = "boop",
    version,
    about = "Cross-harness agent transcript reader: drive agents with `beep`, read what they did with `db`",
    after_help = DOCTRINE
)]
struct Cli {
    #[command(subcommand)]
    command: SubCmd,
}

#[derive(Subcommand)]
enum SubCmd {
    /// Drive agents: harnesses, lanes, mail, processes.
    Beep {
        #[command(subcommand)]
        cmd: BeepCmd,
    },
    /// Run raw SQL read-only against the store (the default `db` form), or
    /// read/count what agents did through a `db` subcommand.
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Db {
        /// The SQL to run against ~/.agent/boop.db.
        #[arg(value_name = "SQL")]
        sql: Option<String>,
        /// Output format for the SQL passthrough.
        #[arg(long, value_enum)]
        format: Option<QueryFormat>,
        #[command(subcommand)]
        cmd: Option<DbCmd>,
    },
    /// Report the caller's own identity and the rung that resolved it.
    Whoami {
        #[arg(long)]
        json: bool,
    },
    /// List registered harness adapters, one per line. (pass 1)
    #[command(hide = true)]
    Harnesses,
    /// List on-disk sessions, newest last. (pass 1)
    #[command(hide = true)]
    Sessions {
        /// Only sessions from this harness (its stable id).
        #[arg(long)]
        harness: Option<String>,
    },
    /// Tail one session's events from a byte offset. (pass 1)
    #[command(hide = true)]
    Tail {
        /// The session id to read.
        session_id: String,
        /// Byte offset to start from. Defaults to 0.
        #[arg(long)]
        from: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Stream turns across sessions, filtered from the db. (pass 4)
    #[command(hide = true)]
    Events {
        #[command(flatten)]
        query: QueryArgs,
    },
    /// List lanes and messages like `bus list`.
    #[command(hide = true)]
    List {
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Measure per-lane pid, rss, cpu, uptime, child count.
    #[command(hide = true)]
    Measure {
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Spawn a lane: tmux new-session + mailbox + registry route.
    #[command(hide = true)]
    Dispatch {
        #[arg(long)]
        to: String,
        #[arg(long)]
        cwd: String,
        #[arg(long)]
        cmd: String,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        tmux: Option<String>,
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        r#ref: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        #[arg(long, default_value_t = 3)]
        resolve_wait: u64,
        /// Spawn in the main tree instead of creating a worktree.
        #[arg(long)]
        main_tree: bool,
        #[arg(long)]
        base_sha: Option<String>,
    },
    /// Resolve a lane's harness session id into its registry route.
    #[command(hide = true)]
    Resolve {
        #[arg(long)]
        to: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Queue a message and inject it into a live pane.
    #[command(hide = true)]
    Hail {
        #[arg(long)]
        to: String,
        #[arg(long)]
        body: String,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        box_: Option<String>,
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Acknowledge unacked lanes via cass and stamp token usage.
    #[command(hide = true)]
    Sweep {
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        box_: Option<String>,
        #[arg(long)]
        close_routeless: bool,
        #[arg(long, default_value_t = 7)]
        max_age_days: u64,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Register and spawn a lane (the first-contact verb).
    #[command(hide = true)]
    Lane {
        #[arg(long)]
        name: String,
        #[arg(long)]
        cwd: String,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        brief: Option<PathBuf>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        tmux: Option<String>,
        #[arg(long)]
        parent: Option<String>,
        /// New branch name; with `--base-sha`, spawns in a worktree instead
        /// of `--cwd` directly.
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        base_sha: Option<String>,
        /// tmux socket to spawn on; a throwaway socket for tests, `None` for
        /// the default server.
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Rewrite a registry route only; never spawns.
    #[command(hide = true)]
    Adopt {
        #[arg(long)]
        name: String,
        #[arg(long)]
        tmux: String,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        /// The lane that summoned this one.
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Drop lanes whose tmux sessions are gone. Refuses when tmux is
    /// unreachable because it cannot tell live from dead.
    #[command(hide = true)]
    Prune {
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Project sessions into NDJSON chat-repr turns (the zipf door).
    #[command(hide = true)]
    Chat {
        #[command(flatten)]
        query: QueryArgs,
        /// Project every session the registry knows.
        #[arg(long)]
        all: bool,
        /// Tail new turns from the db, one NDJSON line per new turn.
        #[arg(long)]
        follow: bool,
        #[arg(long)]
        json: bool,
    },
    /// Tail every harness forward from stored offsets into the db.
    #[command(hide = true)]
    Sync {
        /// Drop every stored row and re-project every transcript from byte 0.
        /// Required once to move a store off pre-dense turn ordinals.
        #[arg(long)]
        rebuild: bool,
    },
    /// Stream new facts into the db on a coarse poll (idle near-zero CPU).
    #[command(hide = true)]
    Follow {},
}

/// The shared read filter, used by `chat` and `events`.
#[derive(clap::Args, Clone, Default)]
struct QueryArgs {
    #[arg(long)]
    harness: Option<String>,
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    role: Option<String>,
    #[arg(long)]
    since: Option<u64>,
    #[arg(long)]
    until: Option<u64>,
    #[arg(long)]
    turn_from: Option<u64>,
    #[arg(long)]
    turn_to: Option<u64>,
    #[arg(long)]
    path: Option<String>,
    #[arg(long)]
    limit: Option<u64>,
    #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
    format: QueryFormat,
}

#[derive(Clone, Copy, ValueEnum, Default)]
enum QueryFormat {
    #[default]
    Ndjson,
    Text,
}

#[derive(Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, ValueEnum, Default)]
enum PstreeFormat {
    #[default]
    Text,
    Ndjson,
}

/// Write one line, treating a closed pipe as a normal end. Rust masks SIGPIPE,
/// so a bare `println!` panics the moment output is piped into `head`.
fn line(text: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    match writeln!(out, "{text}") {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => std::process::exit(0),
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "write failed: {error}");
            std::process::exit(1);
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let registry = Registry::discover();
    match cli.command {
        SubCmd::Harnesses => run_harnesses(&registry),
        SubCmd::Sessions { harness } => run_sessions(&registry, harness.as_deref()),
        SubCmd::Tail {
            session_id,
            from,
            format,
        } => run_tail(&registry, &session_id, from.unwrap_or(0), format),
        SubCmd::Events { query } => run_query(&query),
        SubCmd::Sync { rebuild } => run_sync_all(&registry, rebuild),
        SubCmd::Follow {} => run_follow(&registry),
        SubCmd::Chat {
            query,
            all,
            follow,
            json,
        } => run_chat_query(&query, all, follow, json),
        SubCmd::List {
            agent,
            all,
            mail_dir,
        } => run_list(mail_dir.as_deref(), agent.as_deref(), all),
        SubCmd::Measure { mail_dir } => run_measure(mail_dir.as_deref()),
        SubCmd::Dispatch {
            to,
            cwd,
            cmd,
            from,
            harness,
            session_id,
            model,
            mode,
            tmux,
            socket,
            body,
            r#ref,
            goal,
            mail_dir,
            resolve_wait,
            main_tree,
            base_sha,
        } => run_dispatch(
            &registry,
            DispatchArgs {
                to,
                cwd,
                cmd,
                from,
                harness,
                session_id,
                model,
                mode,
                tmux,
                socket,
                body,
                r#ref,
                goal,
                mail_dir,
                resolve_wait,
                main_tree,
                base_sha,
                branch: None,
                worktree_dir: None,
                parent: None,
                on_exit: None,
            },
        ),
        SubCmd::Resolve { to, mail_dir } => run_resolve(&to, mail_dir.as_deref()),
        SubCmd::Hail {
            to,
            body,
            from,
            kind,
            box_,
            socket,
            mail_dir,
        } => run_hail(
            &registry,
            &to,
            &body,
            from.as_deref(),
            kind.as_deref(),
            box_.as_deref(),
            socket.as_deref(),
            mail_dir.as_deref(),
        ),
        SubCmd::Sweep {
            agent,
            box_,
            close_routeless,
            max_age_days,
            mail_dir,
        } => run_sweep(
            mail_dir.as_deref(),
            box_.as_deref(),
            agent.as_deref(),
            close_routeless,
            max_age_days,
        ),
        SubCmd::Lane {
            name,
            cwd,
            harness,
            brief,
            model,
            tmux,
            parent,
            branch,
            base_sha,
            socket,
            goal,
            mail_dir,
            dry_run,
        } => run_lane(
            &registry,
            LaneArgs {
                name,
                cwd,
                harness,
                brief,
                model,
                tmux,
                parent,
                branch,
                base_sha,
                socket,
                goal,
                mail_dir,
                dry_run,
            },
        ),
        SubCmd::Adopt {
            name,
            tmux,
            harness,
            session_id,
            cwd,
            model,
            mode,
            parent,
            goal,
            mail_dir,
        } => run_adopt(
            &name,
            &tmux,
            harness.as_deref(),
            session_id.as_deref(),
            cwd.as_deref(),
            model.as_deref(),
            mode.as_deref(),
            parent.as_deref(),
            goal.as_deref(),
            mail_dir.as_deref(),
        ),
        SubCmd::Prune { mail_dir } => run_prune(mail_dir.as_deref()),
        SubCmd::Beep { cmd } => run_beep(&registry, cmd),
        SubCmd::Db { sql, format, cmd } => match cmd {
            Some(cmd) => run_db(&registry, cmd),
            None => match sql {
                Some(sql) => run_passthrough(&sql, format.unwrap_or_default()),
                None => anyhow::bail!(
                    "boop db needs a SQL string or a subcommand; see `boop db --help`"
                ),
            },
        },
        SubCmd::Whoami { json } => run_whoami(json),
    }
}

// ---------------------------------------------------------------------------
// Pass 1 verbs: layer 2 (transcript)
// ---------------------------------------------------------------------------

fn run_harnesses(registry: &Registry) -> Result<()> {
    for harness in registry.all() {
        line(harness.id());
    }
    Ok(())
}

fn run_sessions(registry: &Registry, harness_id: Option<&str>) -> Result<()> {
    let harnesses: Vec<&dyn boop::harness::Harness> = match harness_id {
        Some(id) => vec![resolve_harness(registry, id)?],
        None => registry.all().iter().map(|boxed| boxed.as_ref()).collect(),
    };
    for adapter in harnesses {
        for session in adapter.sessions()? {
            line(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                session.session_id,
                session.harness,
                session.cwd.as_deref().unwrap_or("-"),
                session.git_branch.as_deref().unwrap_or("-"),
                session.modified_ms,
                session.size,
            ));
        }
    }
    Ok(())
}

fn run_tail(
    registry: &Registry,
    session_id: &str,
    offset: u64,
    format: OutputFormat,
) -> Result<()> {
    for adapter in registry.all() {
        for session in adapter.sessions()? {
            if session.session_id == session_id {
                let chunk = adapter.read_from(&session, offset)?;
                emit_notes(chunk.reset, chunk.skipped);
                for event in &chunk.events {
                    emit_event(event, format);
                }
                if matches!(format, OutputFormat::Text) {
                    eprintln!("resume offset: {}", chunk.next_offset);
                }
                return Ok(());
            }
        }
    }
    anyhow::bail!("no session found with id `{session_id}`")
}

/// Resolve the shared filter set, with the session filter pinned externally
/// so `--all` can clear it.
fn query_from(q: &QueryArgs, session: Option<String>) -> ident::TurnQuery {
    ident::TurnQuery {
        harness: q.harness.clone(),
        session,
        role: q.role.clone(),
        since: q.since,
        until: q.until,
        turn_from: q.turn_from,
        turn_to: q.turn_to,
        path: q.path.clone(),
        limit: q.limit,
    }
}

/// Query the db with the shared filter set; emit raw rows, no interpretation.
/// Turns first, then any spawn edges touching the filtered session.
fn run_query(query: &QueryArgs) -> Result<()> {
    let store = ident::Store::open(ident::Store::default_path()?)?;
    let rows = store.query_turns(&query_from(query, query.session.clone()))?;
    emit_rows(&rows, query.format);
    emit_edges(&store, query.session.as_deref(), query.limit)?;
    Ok(())
}

/// `boop chat`: like `query` but emits the chat-repr turn shape. `--all`
/// clears the session filter; `--follow` re-queries in a loop.
fn run_chat_query(query: &QueryArgs, all: bool, follow: bool, _json: bool) -> Result<()> {
    let store = ident::Store::open(ident::Store::default_path()?)?;
    let session = if all { None } else { query.session.clone() };
    if follow {
        loop {
            let rows = store.query_turns(&query_from(query, session.clone()))?;
            emit_rows(&rows, QueryFormat::Ndjson);
            std::io::Write::flush(&mut std::io::stdout())?;
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    let rows = store.query_turns(&query_from(query, session.clone()))?;
    emit_rows(&rows, query.format);
    Ok(())
}

fn emit_edges(store: &ident::Store, session: Option<&str>, limit: Option<u64>) -> Result<()> {
    let edges = store.query_edges(session)?;
    for edge in edges.into_iter().take(limit.unwrap_or(u64::MAX) as usize) {
        line(&serde_json::to_string(&edge)?.to_string());
    }
    Ok(())
}

fn emit_rows(rows: &[ident::Row], format: QueryFormat) {
    for row in rows {
        match format {
            QueryFormat::Ndjson => {
                if let Ok(encoded) = serde_json::to_string(row) {
                    line(&encoded);
                }
            }
            QueryFormat::Text => {
                line(&format!(
                    "{} {} {} {} {}",
                    row["session"].as_str().unwrap_or(""),
                    row["turn"].as_i64().unwrap_or(0),
                    row["role"].as_str().unwrap_or(""),
                    row["ts"].as_i64().unwrap_or(0),
                    row["said"].as_str().unwrap_or(""),
                ));
            }
        }
    }
}

/// `boop sync`: tail every harness forward from stored offsets into the db.
fn run_sync_all(registry: &Registry, rebuild: bool) -> Result<()> {
    let started = std::time::Instant::now();
    let store = ident::Store::open(ident::Store::default_path()?)?;
    if rebuild {
        store.rebuild()?;
    } else {
        refuse_stale(&store)?;
    }
    store.begin()?;
    let result = (|| {
        let mut stat = ident::SyncStat::default();
        for adapter in registry.all() {
            for session in adapter.sessions()? {
                stat.add(ident::sync_session(&store, adapter.as_ref(), &session)?);
            }
        }
        Ok::<ident::SyncStat, anyhow::Error>(stat)
    })();
    match result {
        Ok(stat) => {
            store.commit()?;
            let elapsed_ms = started.elapsed().as_millis();
            let counts = store.counts()?;
            let db_bytes = store.db_bytes()?;
            let sparse = store.sparse_sessions()?.len();
            let rate = (stat.written as u128)
                .saturating_mul(1000)
                .checked_div(elapsed_ms.max(1))
                .unwrap_or(0) as u64;
            println!(
                "events={} dropped={} usage_new={} usage_updated={} sparse_sessions={sparse} elapsed_ms={elapsed_ms} rate={rate}/s db_bytes={db_bytes} counts={}",
                stat.written,
                stat.dropped,
                stat.usage_written,
                stat.usage_updated,
                serde_json::to_string(&counts)?
            );
        }
        Err(error) => {
            let _ = store.rollback();
            return Err(error);
        }
    }
    Ok(())
}

/// A store written before dense ordinals is readable but not appendable, so
/// the refusal names the one command that fixes it.
fn refuse_stale(store: &ident::Store) -> Result<()> {
    if !store.is_stale()? {
        return Ok(());
    }
    anyhow::bail!(
        "store is schema version {}, this boop writes version {}: rows stored under \
         an older schema cannot be appended to. Run `boop db sync create --rebuild` \
         to drop every stored row and re-project every transcript from byte 0.",
        store.schema_version()?,
        ident::SCHEMA_VERSION
    )
}

/// `boop follow`: the same projection on a coarse poll. Sessions and their
/// mtimes are discovered once, and a file is only re-read when its mtime
/// changed, so steady-state idle is a stat per file plus a sleep.
fn run_follow(registry: &Registry) -> Result<()> {
    refuse_stale(&ident::Store::open(ident::Store::default_path()?)?)?;
    let mut sessions = Vec::new();
    for adapter in registry.all() {
        for session in adapter.sessions()? {
            sessions.push((adapter.id().to_owned(), session));
        }
    }
    let mut last_mtime: std::collections::HashMap<String, u64> = sessions
        .iter()
        .map(|(_, session)| {
            let mtime = file_mtime_ms(&session.path).unwrap_or(0);
            (session.session_id.clone(), mtime)
        })
        .collect();
    loop {
        let store = ident::Store::open(ident::Store::default_path()?)?;
        store.begin()?;
        for (harness_id, session) in &sessions {
            let mtime = file_mtime_ms(&session.path).unwrap_or(0);
            if last_mtime.get(&session.session_id).copied().unwrap_or(0) == mtime {
                continue;
            }
            let adapter = harness_by_id(registry, harness_id)?;
            let _ = ident::sync_session(&store, adapter, session)?;
            last_mtime.insert(session.session_id.clone(), mtime);
        }
        store.commit()?;
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn file_mtime_ms(path: &std::path::Path) -> Result<u64> {
    use std::time::UNIX_EPOCH;
    let metadata = std::fs::metadata(path)?;
    match metadata.modified() {
        Ok(time) => Ok(time
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)),
        Err(_) => Ok(0),
    }
}

fn resolve_harness<'a>(registry: &'a Registry, id: &str) -> Result<&'a dyn boop::harness::Harness> {
    registry
        .by_id(id)
        .with_context(|| format!("no harness registered with id `{id}`"))
}

fn emit_notes(reset: bool, skipped: usize) {
    if reset {
        eprintln!("note: transcript shorter than stored offset; restarted from byte 0");
    }
    if skipped > 0 {
        eprintln!("note: skipped {skipped} line(s) that failed to parse as JSON");
    }
}

// ---------------------------------------------------------------------------
// The verb output helpers
// ---------------------------------------------------------------------------

fn emit_event(event: &AgentEvent, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            if let Ok(encoded) = serde_json::to_string(event) {
                println!("{encoded}");
            }
        }
        OutputFormat::Text => {
            let paths = event
                .paths
                .iter()
                .map(|path| format!("{}({:?})", path.path, path.access))
                .collect::<Vec<_>>()
                .join(",");
            let tool = event.tool_name.as_deref().unwrap_or("-");
            if paths.is_empty() && event.urls.is_empty() {
                println!(
                    "[{}] {} {} {} {}",
                    event.harness, event.ts_ms, event.record_type, tool, event.session_id
                );
            } else {
                println!(
                    "[{}] {} {} {} {} paths=[{}] urls=[{}]",
                    event.harness,
                    event.ts_ms,
                    event.record_type,
                    tool,
                    event.session_id,
                    paths,
                    event.urls.join(",")
                );
            }
        }
    }
}

fn mail_dir(value: Option<&Path>) -> Result<PathBuf> {
    match value {
        Some(path) => Ok(path.to_path_buf()),
        None => bus::default_mail_dir(),
    }
}

/// Pad `value` to `width` with trailing spaces (Rust strings pad a mix of
/// byte and char semantics; bus uses JS padEnd which pads code units, close
/// enough for lane names here).
fn pad(value: &str, width: usize) -> String {
    let mut out = value.to_owned();
    if out.chars().count() < width {
        out.extend(std::iter::repeat_n(' ', width - out.chars().count()));
    }
    out
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

fn run_list(mail_dir_arg: Option<&Path>, agent: Option<&str>, all: bool) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    match agent {
        None => {
            let routes = bus::read_routes(&dir)?;
            let live = tmux::live_sessions(None);
            for (name, route) in &routes {
                let state = match &live {
                    None => "?",
                    Some(sessions) if sessions.has(route.tmux.as_deref().unwrap_or("")) => "live",
                    Some(_) => "dead",
                };
                let padded_name = pad(name, 16);
                let padded_harness = pad(route.harness.as_deref().unwrap_or("-"), 10);
                let padded_mode = pad(route.mode.as_deref().unwrap_or("-"), 6);
                let padded_model = pad(route.model.as_deref().unwrap_or("-"), 46);
                let padded_tmux = pad(route.tmux.as_deref().unwrap_or("-"), 16);
                line(&format!(
                    "{} {} {} {} {} {} {}",
                    pad(state, 4),
                    padded_name,
                    padded_harness,
                    padded_mode,
                    padded_model,
                    padded_tmux,
                    route.cwd.as_deref().unwrap_or("-"),
                ));
            }
            let messages = all_messages(&dir)?;
            let rows = if all {
                bus::fold(&messages)
            } else {
                bus::unacked(&messages)
            };
            for message in rows {
                line(&bus::message_line(&message).to_string());
            }
            if !all {
                line(&format!(
                    "{} open (closed history: --all)",
                    bus::unacked(&all_messages(&dir)?).len()
                ));
            }
        }
        Some(agent_id) => {
            let messages = all_messages(&dir)?;
            let rows = bus::fold(&messages);
            let inbox: Vec<_> = rows.iter().filter(|m| m.to == agent_id).cloned().collect();
            let outbox: Vec<_> = rows
                .iter()
                .filter(|m| m.from == agent_id)
                .cloned()
                .collect();
            for message in &inbox {
                line(&format!("in  {}", bus::message_line(message)));
            }
            for message in &outbox {
                line(&format!("out {}", bus::message_line(message)));
            }
            let mut combined = inbox.clone();
            combined.extend(outbox.iter().cloned());
            line(&format!(
                "{agent_id}: {} in, {} out, {} unacked",
                inbox.len(),
                outbox.len(),
                bus::unacked(&combined).len()
            ));
        }
    }
    Ok(())
}

fn all_messages(dir: &std::path::Path) -> Result<Vec<bus::Message>> {
    let mut messages = Vec::new();
    for path in bus::read_boxes(dir)? {
        messages.extend(bus::parse_box(&path));
    }
    Ok(messages)
}

// ---------------------------------------------------------------------------
// measure (layer 0)
// ---------------------------------------------------------------------------

fn run_measure(mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let snapshot = proc::SysinfoSnapshot::capture()?;
    line("lane\tpid\trss_kb\tcpu_pct\tuptime_sec\tchildren");
    for (name, route) in &routes {
        let pane_pid = route
            .tmux
            .as_deref()
            .and_then(|target| tmux::pane_pid(None, target))
            .unwrap_or(0);
        match snapshot.process(pane_pid) {
            Some(info) => {
                let uptime = info.start_time_secs;
                line(&format!(
                    "{}\t{}\t{}\t{:.1}\t{}\t{}",
                    name,
                    pane_pid,
                    info.rss_bytes / 1024,
                    info.cpu_percent,
                    uptime,
                    snapshot.descendent_count(pane_pid),
                ));
            }
            None => println!("{}\t{}\t-\t-\t-\t-", name, pane_pid),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// dispatch (layer 1 + bus)
// ---------------------------------------------------------------------------

struct DispatchArgs {
    to: String,
    cwd: String,
    cmd: String,
    from: Option<String>,
    harness: Option<String>,
    session_id: Option<String>,
    model: Option<String>,
    mode: Option<String>,
    tmux: Option<String>,
    socket: Option<String>,
    body: Option<String>,
    r#ref: Option<String>,
    mail_dir: Option<PathBuf>,
    resolve_wait: u64,
    main_tree: bool,
    base_sha: Option<String>,
    /// Overrides the branch name derived from `tmux`/`to`; `lane create`
    /// sets this from its own `--branch` flag.
    branch: Option<String>,
    /// The worktree to create; `None` spawns in `cwd` (`main_tree` decides
    /// whether that's a fast-forward check or a plain directory).
    worktree_dir: Option<PathBuf>,
    /// The lane that summoned this one; written to the route's `parent`.
    parent: Option<String>,
    /// What the lane is running toward; written to the route and dispatch mail.
    goal: Option<String>,
    /// Shell appended after the harness command; `lane create --parent`
    /// composes the completion hail here.
    on_exit: Option<String>,
}

fn run_dispatch(registry: &Registry, args: DispatchArgs) -> Result<()> {
    let adapter = resolve_dispatch_harness(registry, args.harness.as_deref())?;
    // The caller is the PARENT of the lane being born, never its identity.
    let caller = identity::resolve(&bus::read_routes(&mail_dir(args.mail_dir.as_deref())?)?)?;
    let harness_id = adapter.id().to_owned();
    let branch = args
        .branch
        .clone()
        .unwrap_or_else(|| args.tmux.clone().unwrap_or_else(|| args.to.clone()));
    let base_sha = match &args.base_sha {
        Some(sha) => sha.clone(),
        None => git_head(&args.cwd)?.unwrap_or_else(|| "HEAD".into()),
    };
    let mut body = args.body.clone().unwrap_or_else(|| args.cmd.clone());
    // A dispatch's goal rides the route's `goal` field; embed it in the mail
    // row body too so history states the goal without a registry lookup.
    if let Some(goal) = &args.goal {
        body = format!("{body}\n[goal] {goal}");
    }

    let message = bus::Message {
        id: bus::mint_id(),
        from: args.from.clone().unwrap_or_else(|| "coordinator".into()),
        to: args.to.clone(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: "dispatch".into(),
        reply_to: None,
        body,
        r#ref: args.r#ref.clone(),
    };

    let spec = boop::harness::SpawnSpec {
        harness: harness_id.clone(),
        branch,
        base_sha,
        main_tree: args.main_tree,
        setup: Vec::new(),
        prompt: args.cmd.clone(),
        resume_session: args.session_id.clone(),
        socket: args.socket.clone(),
        worktree_dir: args.worktree_dir.clone(),
        repo: std::path::PathBuf::from(&args.cwd),
        env_stamp: Some(identity::child_stamp(
            &args.to,
            &args.to,
            &harness_id,
            caller.session.as_deref(),
        )),
        model: args.model.clone(),
        on_exit: args.on_exit.clone(),
        tmux: args.tmux.clone(),
    };
    let session = adapter.spawn(&spec)?;

    let stamp = format!(
        "[bus {}] dispatched: {}",
        message.id,
        args.cmd.split('\n').next().unwrap_or("")
    );
    adapter.send(&session, &stamp)?;

    let dir = mail_dir(args.mail_dir.as_deref())?;
    // The route's cwd is where the harness actually runs (the worktree when
    // one was made): session-id resolution joins opencode.db on directory.
    let route = Route {
        harness: Some(harness_id),
        tmux: session.tmux.clone(),
        cwd: session.cwd.clone().or_else(|| Some(args.cwd.clone())),
        model: args.model.clone(),
        mode: args.mode.clone(),
        session_id: args.session_id.clone(),
        source_path: None,
        parent: args.parent.clone(),
        goal: args.goal.clone(),
    };
    write_route(&dir, &args.to, route)?;
    append_message(&dir, &message)?;
    println!(
        "dispatched {} -> {} (tmux {})",
        message.id,
        args.to,
        session.tmux.as_deref().unwrap_or("-")
    );
    std::thread::sleep(std::time::Duration::from_secs(args.resolve_wait));
    Ok(())
}

/// The registered harness adapter for a dispatched `--harness`. A named
/// harness must resolve exactly; an unnamed one takes the first registered
/// adapter. A named harness resolving to a different harness is a capability
/// lie, so an unregistered name is a hard error that lists the registered set.
fn resolve_dispatch_harness<'a>(
    registry: &'a Registry,
    id: Option<&str>,
) -> Result<&'a dyn boop::harness::Harness> {
    let Some(id) = id else {
        return registry
            .all()
            .first()
            .map(|boxed| boxed.as_ref())
            .ok_or_else(|| anyhow::anyhow!("no harness registered"));
    };
    match registry.by_id(id) {
        Some(adapter) => Ok(adapter),
        None => {
            let registered = registry
                .all()
                .iter()
                .map(|harness| harness.id())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!("unregistered harness `{id}`; registered harnesses: {registered}")
        }
    }
}

/// The registered harness adapter for a `--harness` filter, or the first
/// registered one when the id is absent.
fn harness_by_id<'a>(registry: &'a Registry, id: &str) -> Result<&'a dyn boop::harness::Harness> {
    registry
        .by_id(id)
        .or_else(|| registry.all().first().map(|b| b.as_ref()))
        .ok_or_else(|| anyhow::anyhow!("no harness registered"))
}

fn git_head(repo: &str) -> Result<Option<String>> {
    let output = std::process::Command::new("git")
        .args(["-C", repo, "rev-parse", "HEAD"])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    ))
}

// ---------------------------------------------------------------------------
// resolve
// ---------------------------------------------------------------------------

fn run_resolve(to: &str, mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let route = match routes.get(to) {
        Some(route) => route,
        None => {
            println!("unresolved {to}: no registry route");
            return Ok(());
        }
    };
    if route.session_id.is_some() {
        println!(
            "resolved {to} -> {} (self-reported)",
            route.session_id.as_deref().unwrap()
        );
        return Ok(());
    }
    let harness = route.harness.as_deref().unwrap_or("-");
    let Some(cwd) = route.cwd.as_deref() else {
        println!("unresolved {to}: no cwd in registry route");
        return Ok(());
    };
    match resolve_harness_binary(harness, cwd) {
        Some(session_id) => {
            let mut updated = route.clone();
            updated.session_id = Some(session_id.clone());
            println!("resolved {to} -> {session_id}");
            let path = dir.join("registry.json");
            bus::cas_update_json(&path, |current| {
                current.insert(to.to_owned(), route_to_json(&updated));
                Ok(())
            })?;
            Ok(())
        }
        None => {
            println!("unresolved {to}: no {harness} session for {cwd} yet");
            Ok(())
        }
    }
}

/// Resolve via the instant-harness binary when it exists (the same binary
/// `bus` shells out to); `None` when the binary is absent or finds nothing.
fn resolve_harness_binary(harness: &str, cwd: &str) -> Option<String> {
    let root = dirs::home_dir()?.join("projects/instant");
    let candidates = [
        root.join("src-tauri/target/debug/instant-harness"),
        root.join("src-tauri/target/release/instant-harness"),
    ];
    let binary = candidates.iter().find(|path| path.exists())?;
    let output = Command::new(binary)
        .args(["resolve", "--harness", harness, "--cwd", cwd])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_session_id(&String::from_utf8_lossy(&output.stdout))
}

fn parse_session_id(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

// ---------------------------------------------------------------------------
// hail
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn run_hail(
    registry: &Registry,
    to: &str,
    body: &str,
    from: Option<&str>,
    kind: Option<&str>,
    box_name: Option<&str>,
    socket: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let message = bus::Message {
        id: bus::mint_id(),
        from: from.unwrap_or("coordinator").to_owned(),
        to: to.to_owned(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: kind.unwrap_or("request").to_owned(),
        reply_to: None,
        body: body.to_owned(),
        r#ref: None,
    };
    append_message_to(&dir, box_name.unwrap_or("bus.ndjson"), &message)?;
    record_control_edge(&message)?;
    println!("queued {} -> {to}", message.id);

    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(to) else {
        println!("no registry route for {to}: message stays queued, to_timestamp null");
        return Ok(());
    };
    let Some(pane) = route.tmux.as_deref() else {
        println!("{to} has no tmux pane: message stays queued, to_timestamp null");
        return Ok(());
    };
    let line = bus::injected_line(&message);
    // Route the send through the harness control facet; tmux is a transport
    // detail inside the impl. The session carries the pane handle spawn gave it.
    let harness_id = route.harness.as_deref().unwrap_or("claude");
    let adapter = harness_by_id(registry, harness_id)?;
    let session = boop::harness::SessionRef {
        harness: adapter.id(),
        session_id: to.to_owned(),
        nickname: to.to_owned(),
        path: std::path::PathBuf::from("/tmp/hail.jsonl"),
        cwd: route.cwd.clone(),
        git_branch: None,
        modified_ms: 0,
        size: 0,
        tmux: Some(pane.to_owned()),
        tmux_socket: socket.map(str::to_owned),
        parent: None,
    };
    let outcome = adapter.send(&session, &line)?;
    match outcome {
        boop::harness::SendOutcome::Injected => println!("injected into tmux {pane}"),
        boop::harness::SendOutcome::QueuedForNextSpawn => {
            println!("queued for next spawn -> {to}");
        }
        boop::harness::SendOutcome::Unsupported => {
            println!("{to} harness has no send support: message stays queued");
        }
    }
    Ok(())
}

fn record_control_edge(message: &boop::bus::Message) -> Result<()> {
    if !matches!(
        message.kind.as_str(),
        "hail" | "result" | "retry" | "resume" | "cancel"
    ) {
        return Ok(());
    }
    let store = boop::Store::open(boop::Store::default_path()?)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    store.add_edge_at(&message.from, &message.to, &message.kind, timestamp)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// sweep
// ---------------------------------------------------------------------------

fn run_sweep(
    mail_dir_arg: Option<&Path>,
    box_name: Option<&str>,
    agent: Option<&str>,
    close_routeless: bool,
    max_age_days: u64,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let messages = all_messages(&dir)?;
    let pending = bus::unacked(&messages);
    if pending.is_empty() {
        println!("nothing unacked");
        return Ok(());
    }
    let cutoff_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
        .saturating_sub(max_age_days * 86_400_000);
    let mut acked = 0usize;
    let mut expired = 0usize;
    for message in &pending {
        if let Some(agent_id) = agent {
            if message.to != agent_id {
                continue;
            }
        }
        if parse_iso_ms(&message.from_timestamp).unwrap_or(0) < cutoff_ms {
            append_ack(&dir, box_name, message)?;
            expired += 1;
            println!("expired {}", message.id);
            continue;
        }
        let Some(route) = routes.get(&message.to) else {
            if close_routeless {
                append_ack(&dir, box_name, message)?;
                expired += 1;
                println!(
                    "expired {} -> {}: no registry route",
                    message.id, message.to
                );
            } else {
                println!(
                    "{} -> {}: no registry route, cannot scope the cass query (--close-routeless expires these)",
                    message.id,
                    message.to
                );
            }
            continue;
        };
        if cass_hit(route, &message.id).unwrap_or(false) {
            append_ack(&dir, box_name, message)?;
            acked += 1;
            println!("{} -> {}: acked", message.id, message.to);
        } else {
            println!(
                "{} -> {}: no transcript hit, still unacked",
                message.id, message.to
            );
        }
    }
    println!(
        "swept {} unacked, acked {acked}, expired {expired}",
        pending.len()
    );
    Ok(())
}

/// Ask `cass` whether the envelope id appears in the recipient's transcript.
fn cass_hit(route: &Route, message_id: &str) -> Result<bool> {
    let output = Command::new("cass")
        .args(["search", message_id, "--robot", "--limit", "20"])
        .output();
    let output = match output {
        Ok(output) if output.status.success() => output,
        _ => return Ok(false),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let hits = value
        .get("hits")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(hits.iter().any(|hit| {
        let source = hit
            .get("source_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        scoped_to_agent(route, source)
    }))
}

fn scoped_to_agent(route: &Route, source_path: &str) -> bool {
    if source_path.is_empty() {
        return false;
    }
    if let Some(expected) = route.source_path.as_deref() {
        return source_path == expected;
    }
    route
        .session_id
        .as_deref()
        .map(|session_id| source_path.contains(session_id))
        .unwrap_or(false)
}

fn parse_iso_ms(text: &str) -> Option<u64> {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::parse(text, &Rfc3339)
        .ok()
        .map(|parsed| parsed.unix_timestamp() as u64 * 1000 + parsed.millisecond() as u64)
}

// ---------------------------------------------------------------------------
// lane
// ---------------------------------------------------------------------------

/// Source of truth for the flash4 lane default; `Opencode::spawn` reads it
/// via `BOOP_LANE_MODEL`, never hardcodes it.
const DEFAULT_LANE_MODEL: &str = "openrouter/deepseek/deepseek-v4-flash-0731";

struct LaneArgs {
    name: String,
    cwd: String,
    harness: Option<String>,
    brief: Option<PathBuf>,
    model: Option<String>,
    tmux: Option<String>,
    parent: Option<String>,
    branch: Option<String>,
    base_sha: Option<String>,
    socket: Option<String>,
    goal: Option<String>,
    mail_dir: Option<PathBuf>,
    dry_run: bool,
}

/// Register and spawn a lane. No match on harness id here; the adapter's own
/// `spawn`/`preview_command` decides how `prompt` becomes a real invocation.
fn run_lane(registry: &Registry, args: LaneArgs) -> Result<()> {
    let harness_id = args.harness.clone().unwrap_or_else(|| "opencode".into());
    let adapter = harness_by_id(registry, &harness_id)?;
    let brief = args
        .brief
        .clone()
        .unwrap_or_else(|| PathBuf::from(&args.cwd).join("brief.md"));
    let model = args
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_LANE_MODEL.to_owned());
    let prompt = brief.display().to_string();
    let worktree_mode = args.branch.is_some() && args.base_sha.is_some();
    let branch = args
        .branch
        .clone()
        .unwrap_or_else(|| args.tmux.clone().unwrap_or_else(|| args.name.clone()));
    let worktree_dir = worktree_mode.then(|| {
        PathBuf::from(&args.cwd)
            .join(".boop-worktrees")
            .join(&branch)
    });
    // The epilogue runs in the lane's pane: the sender is the lane, and the
    // mailbox must be the one this dispatch registered in, never the default.
    let hail_mail_dir = mail_dir(args.mail_dir.as_deref())?;
    let on_exit = args.parent.as_ref().map(|parent| {
        format!(
            "boop hail --to {} --from {} --mail-dir {} --kind result --body \"lane {} done rc=$__rc\" ; boop beep lane delete {} --route-only --mail-dir {}",
            shell_quote(parent),
            shell_quote(&args.name),
            shell_quote(&hail_mail_dir.display().to_string()),
            args.name,
            shell_quote(&args.name),
            shell_quote(&hail_mail_dir.display().to_string()),
        )
    });
    let tmux_name = args.tmux.clone().unwrap_or_else(|| args.name.clone());

    if args.dry_run {
        let spec = boop::harness::SpawnSpec {
            harness: harness_id.clone(),
            branch: branch.clone(),
            base_sha: args.base_sha.clone().unwrap_or_else(|| "HEAD".to_owned()),
            main_tree: !worktree_mode,
            setup: Vec::new(),
            prompt: prompt.clone(),
            resume_session: None,
            socket: args.socket.clone(),
            worktree_dir: worktree_dir.clone(),
            repo: PathBuf::from(&args.cwd),
            env_stamp: None,
            model: Some(model.clone()),
            on_exit: on_exit.clone(),
            tmux: Some(tmux_name.clone()),
        };
        let command = adapter
            .preview_command(&spec)
            .unwrap_or_else(|| format!("{} {}", adapter.id(), shell_quote(&prompt)));
        println!("cmd: {command}");
        println!("to: {}", args.name);
        println!("cwd: {}", args.cwd);
        println!("harness: {harness_id}");
        println!("branch: {branch}");
        if let Some(worktree_dir) = &worktree_dir {
            println!("worktree: {}", worktree_dir.display());
        }
        println!("tmux: {}", args.tmux.as_deref().unwrap_or(&args.name));
        if let Some(parent) = &args.parent {
            println!("parent: {parent} (completion hail appended on exit)");
        }
        if let Some(goal) = &args.goal {
            println!("goal: {goal}");
        }
        return Ok(());
    }
    run_dispatch(
        registry,
        DispatchArgs {
            to: args.name,
            cwd: args.cwd,
            cmd: prompt,
            from: None,
            harness: Some(harness_id),
            session_id: None,
            model: Some(model),
            mode: Some("auto".into()),
            tmux: Some(tmux_name),
            socket: args.socket,
            body: Some(format!(
                "Read and execute the lane brief at {}",
                brief.display()
            )),
            r#ref: Some(brief.display().to_string()),
            mail_dir: args.mail_dir,
            resolve_wait: 3,
            main_tree: !worktree_mode,
            base_sha: args.base_sha,
            branch: Some(branch),
            worktree_dir,
            parent: args.parent.clone(),
            goal: args.goal.clone(),
            on_exit,
        },
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

// ---------------------------------------------------------------------------
// adopt / prune + bus store helpers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn run_adopt(
    name: &str,
    tmux_session: &str,
    harness: Option<&str>,
    session_id: Option<&str>,
    cwd: Option<&str>,
    model: Option<&str>,
    mode: Option<&str>,
    parent: Option<&str>,
    goal: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    if !tmux::has_session(None, tmux_session)? {
        println!("refusing adopt {name}: no such tmux session {tmux_session}");
        return Ok(());
    }
    let dir = mail_dir(mail_dir_arg)?;
    let route = Route {
        harness: harness.map(str::to_owned),
        tmux: Some(tmux_session.to_owned()),
        cwd: cwd.map(str::to_owned),
        model: model.map(str::to_owned),
        mode: mode.map(str::to_owned),
        session_id: session_id.map(str::to_owned),
        source_path: None,
        parent: parent.map(str::to_owned),
        goal: goal.map(str::to_owned),
    };
    write_route(&dir, name, route)?;
    println!("adopted {name} -> tmux {tmux_session}");
    Ok(())
}

fn run_prune(mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    if tmux::live_sessions(None).is_none() {
        println!("refusing prune: tmux unreachable, cannot tell live from dead");
        return Ok(());
    }
    let routes = bus::read_routes(&dir)?;
    let dead: Vec<String> = routes
        .iter()
        .filter(|(_, route)| {
            let Some(target) = route.tmux.as_deref() else {
                return true;
            };
            !tmux::target_alive(None, target)
        })
        .map(|(name, _)| name.clone())
        .collect();
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        for name in &dead {
            current.remove(name);
        }
        Ok(())
    })?;
    println!("pruned {} dead routes", dead.len());
    Ok(())
}

fn write_route(dir: &std::path::Path, lane_id: &str, route: Route) -> Result<()> {
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        current.insert(lane_id.to_owned(), route_to_json(&route));
        Ok(())
    })
}

fn route_to_json(route: &Route) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    if let Some(harness) = &route.harness {
        object.insert("harness".into(), serde_json::json!(harness));
    }
    if let Some(tmux) = &route.tmux {
        object.insert("tmux".into(), serde_json::json!(tmux));
    }
    if let Some(cwd) = &route.cwd {
        object.insert("cwd".into(), serde_json::json!(cwd));
    }
    if let Some(model) = &route.model {
        object.insert("model".into(), serde_json::json!(model));
    }
    if let Some(mode) = &route.mode {
        object.insert("mode".into(), serde_json::json!(mode));
    }
    if let Some(session_id) = &route.session_id {
        object.insert("sessionId".into(), serde_json::json!(session_id));
    }
    if let Some(parent) = &route.parent {
        object.insert("parent".into(), serde_json::json!(parent));
    }
    if let Some(goal) = &route.goal {
        object.insert("goal".into(), serde_json::json!(goal));
    }
    serde_json::Value::Object(object)
}

fn append_message(dir: &std::path::Path, message: &bus::Message) -> Result<()> {
    append_message_to(dir, "bus.ndjson", message)
}

fn append_message_to(dir: &std::path::Path, filename: &str, message: &bus::Message) -> Result<()> {
    let path = dir.join(filename);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create mail dir")?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .context("open ndjson box")?;
    writeln!(file, "{}", bus::message_line(message)).context("append ndjson box")?;
    Ok(())
}

fn append_ack(
    dir: &std::path::Path,
    _box_name: Option<&str>,
    message: &bus::Message,
) -> Result<()> {
    let mut ack = message.clone();
    ack.to_timestamp = Some(bus::now_iso());
    append_message(dir, &ack)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use super::{resolve_dispatch_harness, run_lane_delete, write_route};
    use boop::bus::{read_routes, Route};
    use boop::registry::Registry;

    /// A named harness that is not registered must be refused, never quietly
    /// swapped for the first adapter, which would be a capability lie.
    #[test]
    fn dispatch_refuses_an_unregistered_harness() {
        let registry = Registry::discover();
        let error = match resolve_dispatch_harness(&registry, Some("gemini-cli")) {
            Ok(_) => panic!("unregistered harness must be refused"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("gemini-cli"), "message: {message}");
        assert!(message.contains("claude"), "registered set: {message}");
        assert!(message.contains("opencode"), "registered set: {message}");
    }

    fn temp_mail_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "boop_mail_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// RECEIPT (Job 3b). A `--route-only` delete drops the lane's registry row
    /// without touching pane or tmux, so the on-exit epilogue cleans up in-pane.
    #[test]
    fn route_only_delete_drops_the_registry_row_without_tmux() {
        let dir = temp_mail_dir();
        write_route(
            &dir,
            "l",
            Route {
                harness: Some("claude".into()),
                tmux: Some("somesession".into()),
                cwd: None,
                model: None,
                mode: None,
                session_id: None,
                source_path: None,
                parent: None,
                goal: None,
            },
        )
        .unwrap();
        run_lane_delete(Some(&dir), "l", true).unwrap();
        let routes = read_routes(&dir).unwrap();
        assert!(
            !routes.contains_key("l"),
            "a finished lane must leave no registry row"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (job 1). A route written with --goal round-trips through the
    /// registry.
    #[test]
    fn route_goal_round_trips() {
        let dir = temp_mail_dir();
        let route = Route {
            harness: Some("opencode".into()),
            tmux: Some("lane-x".into()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: Some("ship the edge".into()),
        };
        write_route(&dir, "child", route).unwrap();
        let routes = read_routes(&dir).unwrap();
        assert_eq!(
            routes["child"].goal.as_deref(),
            Some("ship the edge"),
            "registry: {:#?}",
            routes
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn route_with(parent: Option<&str>) -> Route {
        Route {
            harness: Some("opencode".into()),
            tmux: Some("lane-x".into()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: parent.map(str::to_owned),
            goal: None,
        }
    }

    fn dispatch(from: &str, to: &str) -> boop::bus::Message {
        boop::bus::Message {
            id: format!("m-{from}-{to}"),
            from: from.into(),
            to: to.into(),
            from_timestamp: "2026-01-01T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: "dispatch".into(),
            reply_to: None,
            body: "".into(),
            r#ref: None,
        }
    }

    fn live_meta(pid: u32) -> super::LaneMeta {
        super::LaneMeta {
            pid,
            state: "live",
            descendants: vec![],
        }
    }

    /// RECEIPT (pstree). A route's explicit `--parent` wins over a mailbox
    /// dispatch edge that names a different summoner.
    #[test]
    fn explicit_parent_beats_inferred_dispatch() {
        let mut routes = BTreeMap::new();
        routes.insert("child".into(), route_with(Some("explicit")));
        let messages = vec![dispatch("mailbox", "child")];
        let edges = super::resolve_edges(&routes, &messages);
        let edge = &edges["child"];
        assert_eq!(edge.parent.as_deref(), Some("explicit"));
        assert!(!edge.inferred);
    }

    /// RECEIPT (pstree). An orphaned route infers its summoner from the FIRST
    /// dispatch row addressed to it, later rows ignored.
    #[test]
    fn orphan_infers_summoner_from_first_dispatch() {
        let mut routes = BTreeMap::new();
        routes.insert("child".into(), route_with(None));
        let messages = vec![
            dispatch("summoner1", "child"),
            dispatch("summoner2", "child"),
        ];
        let edges = super::resolve_edges(&routes, &messages);
        let edge = &edges["child"];
        assert_eq!(edge.parent.as_deref(), Some("summoner1"));
        assert!(edge.inferred);
    }

    /// RECEIPT (pstree). A summoner absent from the registry renders as a
    /// `[gone]` root with the orphan lane hung beneath it.
    #[test]
    fn orphan_root_prints_gone_summoner() {
        let mut routes = BTreeMap::new();
        routes.insert("child".into(), route_with(None));
        let messages = vec![dispatch("coordinator", "child")];
        let edges = super::resolve_edges(&routes, &messages);
        let mut meta = BTreeMap::new();
        meta.insert("child".into(), live_meta(4242));
        let mut include = BTreeSet::new();
        include.insert("child".into());
        let nodes = super::build_lane_nodes(&edges, &meta, &include);
        let text = super::render_text(&nodes);
        let joined = text.join("\n");
        assert!(joined.contains("coordinator [gone]"), "text:\n{joined}");
        assert!(
            joined.contains("child (4242) [live] [inferred]"),
            "text:\n{joined}"
        );
        let ndjson = super::render_ndjson(&nodes);
        let gone = ndjson
            .iter()
            .find(|row| row.contains("\"lane\":\"coordinator\""))
            .unwrap();
        assert!(gone.contains("\"state\":\"gone\""), "row: {gone}");
        assert!(gone.contains("\"pid\":null"), "row: {gone}");
    }

    /// RECEIPT (pstree). A true root with no parent edge stays a root and is
    /// never inferred from a non-dispatch message.
    #[test]
    fn a_lane_with_no_dispatch_shadow_is_a_root() {
        let mut routes = BTreeMap::new();
        routes.insert("loner".into(), route_with(None));
        let messages = vec![boop::bus::Message {
            kind: "note".into(),
            ..dispatch("whoever", "loner")
        }];
        let edges = super::resolve_edges(&routes, &messages);
        let edge = &edges["loner"];
        assert_eq!(edge.parent, None);
        assert!(!edge.inferred);
    }
}

// ---------------------------------------------------------------------------
// The two trees. `beep` controls agents, `db` reads what they did; the mapping
// to REST is 1:1 per plans/2026-08-09-boop-openapi.yaml.
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum BeepCmd {
    /// Harness adapters and what each can do.
    Harness {
        #[command(subcommand)]
        cmd: HarnessCmd,
    },
    /// Lanes: the agents boop spawns and tracks.
    Lane {
        #[command(subcommand)]
        cmd: LaneCmd,
    },
    /// Type into a running agent, and say whether the keystrokes landed.
    Hail {
        lane: String,
        #[arg(long)]
        body: String,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Mail across lanes.
    Message {
        #[command(subcommand)]
        cmd: MessageCmd,
    },
    /// pid, rss, cpu, uptime, child count per live lane.
    Ps {
        lane: Option<String>,
        /// Include dead routes (no live process behind the pane).
        #[arg(long)]
        all: bool,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Filesystem-style tree of lanes by parent edge.
    Pstree {
        /// Include dead lanes; default is live-only.
        #[arg(long)]
        all: bool,
        #[arg(long, value_enum, default_value_t = PstreeFormat::Text)]
        format: PstreeFormat,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum HarnessCmd {
    List,
    Get { harness: String },
}

#[derive(Subcommand)]
enum LaneCmd {
    /// Every lane, with live or dead.
    List {
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Make a worktree, spawn the agent, register the route.
    Create {
        #[arg(long)]
        lane: String,
        #[arg(long)]
        cwd: String,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        brief: Option<PathBuf>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        tmux: Option<String>,
        #[arg(long)]
        parent: Option<String>,
        /// New branch name; with `--base-sha`, spawns in a worktree instead
        /// of `--cwd` directly.
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        base_sha: Option<String>,
        /// tmux socket to spawn on; a throwaway socket for tests, `None` for
        /// the default server.
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    /// One lane's route and state.
    Get {
        lane: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Point a lane at a pane that already exists.
    Patch {
        lane: String,
        #[arg(long)]
        tmux: String,
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        /// The lane that summoned this one.
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Stop a lane and forget it, or bulk-delete by state.
    Delete {
        lane: Option<String>,
        /// Drop only the registry route; never kill the pane. The `--parent`
        /// on-exit epilogue uses this to clean up while still running inside it.
        #[arg(long)]
        route_only: bool,
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Which tmux pane and harness session id.
    Route {
        lane: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Show the lane's screen.
    Pane {
        lane: String,
        #[arg(long)]
        lines: Option<u32>,
        #[arg(long)]
        socket: Option<String>,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// The lane's mailbox.
    Message {
        #[command(subcommand)]
        cmd: LaneMessageCmd,
    },
}

#[derive(Subcommand)]
enum LaneMessageCmd {
    List {
        lane: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum MessageCmd {
    /// Mark mail handled, in bulk.
    Ack {
        #[arg(long)]
        lane: Option<String>,
        #[arg(long)]
        box_: Option<String>,
        #[arg(long)]
        close_routeless: bool,
        #[arg(long, default_value_t = 7)]
        max_age_days: u64,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum DbCmd {
    Session {
        #[command(subcommand)]
        cmd: SessionCmd,
    },
    Turn {
        #[command(subcommand)]
        cmd: TurnCmd,
    },
    Chat {
        #[command(subcommand)]
        cmd: ChatCmd,
    },
    Touch {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    Command {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    Fetch {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    Skill {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    Pr {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    Span {
        #[command(subcommand)]
        cmd: FactCmd,
    },
    Edge {
        #[command(subcommand)]
        cmd: EdgeCmd,
    },
    /// Tokens and cost. A totals report the passthrough powers, and a parent
    /// of the row computations blocks and burn-rate; clap needs both attributes
    /// to accept the two forms.
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Usage {
        #[command(flatten)]
        args: UsageArgs,
        /// Print this alias's SQL and exit.
        #[arg(long)]
        show_sql: bool,
        #[command(subcommand)]
        cmd: Option<UsageCmd>,
    },
    /// The rate table cost is computed from.
    Price {
        #[command(subcommand)]
        cmd: PriceCmd,
    },
    /// Ingest new transcript bytes.
    Sync {
        #[command(subcommand)]
        cmd: SyncCmd,
    },
    /// How far ingest has read each transcript.
    SyncCursor {
        #[command(subcommand)]
        cmd: CursorCmd,
    },
    /// Who is alive, who moved recently, and what it cost.
    Status {
        /// Window in minutes.
        #[arg(long, default_value_t = 10)]
        window: u64,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

#[derive(clap::Args, Clone, Default)]
struct UsageArgs {
    #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
    format: QueryFormat,
}

#[derive(clap::Args, Clone, Default)]
struct FactArgs {
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    since: Option<u64>,
    #[arg(long)]
    until: Option<u64>,
    /// Prefix match on the row's leading dictionary column.
    #[arg(long)]
    like: Option<String>,
    #[arg(long)]
    limit: Option<u64>,
    #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
    format: QueryFormat,
}

#[derive(Subcommand)]
enum UsageCmd {
    /// Gap-aware billing windows.
    Blocks {
        #[arg(long, default_value_t = 5)]
        window_hours: u64,
        /// Only the window that is still open.
        #[arg(long)]
        active: bool,
        #[command(flatten)]
        args: UsageArgs,
    },
    /// Tokens per minute and dollars per hour over a trailing window.
    BurnRate {
        #[arg(long, default_value_t = 60)]
        window_minutes: u64,
        #[command(flatten)]
        args: UsageArgs,
    },
}

#[derive(Subcommand)]
enum PriceCmd {
    List,
    /// Write one rate row by hand, in USD per million tokens.
    Set {
        model: String,
        #[arg(long)]
        input_per_mtok: f64,
        #[arg(long)]
        output_per_mtok: f64,
        #[arg(long)]
        cache_write_5m_per_mtok: f64,
        #[arg(long)]
        cache_write_1h_per_mtok: f64,
        #[arg(long)]
        cache_read_per_mtok: f64,
        #[arg(long, default_value = "manual")]
        source: String,
    },
}

#[derive(Subcommand)]
enum FactCmd {
    List {
        #[command(flatten)]
        args: FactArgs,
    },
}

#[derive(Subcommand)]
enum SessionCmd {
    List {
        #[arg(long)]
        limit: Option<u64>,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
    Get {
        session: String,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

#[derive(Subcommand)]
enum TurnCmd {
    List {
        #[command(flatten)]
        query: QueryArgs,
    },
    Get {
        session: String,
        turn: u64,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

#[derive(Subcommand)]
enum ChatCmd {
    List {
        #[command(flatten)]
        query: QueryArgs,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        follow: bool,
    },
}

#[derive(Subcommand)]
enum EdgeCmd {
    List {
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        limit: Option<u64>,
    },
}

#[derive(Subcommand)]
enum SyncCmd {
    Create {
        #[arg(long)]
        rebuild: bool,
        /// Keep syncing on a poll instead of returning.
        #[arg(long)]
        forever: bool,
    },
}

#[derive(Subcommand)]
enum CursorCmd {
    List {
        #[arg(long)]
        limit: Option<u64>,
        #[arg(long, value_enum, default_value_t = QueryFormat::Ndjson)]
        format: QueryFormat,
    },
}

// ---------------------------------------------------------------------------
// beep
// ---------------------------------------------------------------------------

fn run_beep(registry: &Registry, cmd: BeepCmd) -> Result<()> {
    match cmd {
        BeepCmd::Harness { cmd } => match cmd {
            HarnessCmd::List => run_harnesses(registry),
            HarnessCmd::Get { harness } => run_harness_get(registry, &harness),
        },
        BeepCmd::Lane { cmd } => run_beep_lane(registry, cmd),
        BeepCmd::Hail {
            lane,
            body,
            from,
            kind,
            socket,
            mail_dir,
        } => run_hail(
            registry,
            &lane,
            &body,
            from.as_deref(),
            kind.as_deref(),
            None,
            socket.as_deref(),
            mail_dir.as_deref(),
        ),
        BeepCmd::Message { cmd } => match cmd {
            MessageCmd::Ack {
                lane,
                box_,
                close_routeless,
                max_age_days,
                mail_dir,
            } => run_sweep(
                mail_dir.as_deref(),
                box_.as_deref(),
                lane.as_deref(),
                close_routeless,
                max_age_days,
            ),
        },
        BeepCmd::Ps {
            lane,
            all,
            mail_dir,
        } => run_ps(mail_dir.as_deref(), lane.as_deref(), all),
        BeepCmd::Pstree {
            all,
            format,
            mail_dir,
        } => run_pstree(mail_dir.as_deref(), all, format),
    }
}

fn run_beep_lane(registry: &Registry, cmd: LaneCmd) -> Result<()> {
    match cmd {
        LaneCmd::List {
            state,
            harness,
            mail_dir,
        } => run_lane_list(mail_dir.as_deref(), state.as_deref(), harness.as_deref()),
        LaneCmd::Create {
            lane,
            cwd,
            harness,
            brief,
            model,
            tmux,
            parent,
            branch,
            base_sha,
            socket,
            goal,
            mail_dir,
            dry_run,
        } => run_lane(
            registry,
            LaneArgs {
                name: lane,
                cwd,
                harness,
                brief,
                model,
                tmux,
                parent,
                branch,
                base_sha,
                socket,
                goal,
                mail_dir,
                dry_run,
            },
        ),
        LaneCmd::Get { lane, mail_dir } => run_lane_get(mail_dir.as_deref(), &lane),
        LaneCmd::Patch {
            lane,
            tmux,
            harness,
            session_id,
            cwd,
            model,
            mode,
            parent,
            goal,
            mail_dir,
        } => run_adopt(
            &lane,
            &tmux,
            harness.as_deref(),
            session_id.as_deref(),
            cwd.as_deref(),
            model.as_deref(),
            mode.as_deref(),
            parent.as_deref(),
            goal.as_deref(),
            mail_dir.as_deref(),
        ),
        LaneCmd::Delete {
            lane,
            route_only,
            state,
            mail_dir,
        } => match (lane, state) {
            (Some(lane), _) => run_lane_delete(mail_dir.as_deref(), &lane, route_only),
            (None, Some(_)) => run_prune(mail_dir.as_deref()),
            (None, None) => {
                anyhow::bail!("name a lane to delete, or pass --state dead for a bulk delete")
            }
        },
        LaneCmd::Route { lane, mail_dir } => run_resolve(&lane, mail_dir.as_deref()),
        LaneCmd::Pane {
            lane,
            lines,
            socket,
            mail_dir,
        } => run_lane_pane(mail_dir.as_deref(), &lane, lines, socket.as_deref()),
        LaneCmd::Message { cmd } => match cmd {
            LaneMessageCmd::List { lane, mail_dir } => {
                run_list(mail_dir.as_deref(), Some(&lane), true)
            }
        },
    }
}

fn run_harness_get(registry: &Registry, id: &str) -> Result<()> {
    let adapter = resolve_harness(registry, id)?;
    let caps = adapter.capabilities();
    println!(
        "{}",
        serde_json::json!({
            "harness": adapter.id(),
            "send_midflight": caps.send_midflight,
            "resume": caps.resume,
            "spawn": caps.spawn,
            "subagent_visible": caps.subagent_visible,
        })
    );
    Ok(())
}

/// Lanes only. `boop list` printed routes and mail together; the two trees
/// split that, so this half never prints a message.
fn run_lane_list(
    mail_dir_arg: Option<&Path>,
    state_filter: Option<&str>,
    harness_filter: Option<&str>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let live = tmux::live_sessions(None);
    for (name, route) in &routes {
        let state = lane_state(&live, route);
        if let Some(want) = state_filter {
            if state != want {
                continue;
            }
        }
        if let Some(want) = harness_filter {
            if route.harness.as_deref() != Some(want) {
                continue;
            }
        }
        line(&format!(
            "{} {} {} {} {} {} {}",
            pad(state, 4),
            pad(name, 16),
            pad(route.harness.as_deref().unwrap_or("-"), 10),
            pad(route.mode.as_deref().unwrap_or("-"), 6),
            pad(route.model.as_deref().unwrap_or("-"), 46),
            pad(route.tmux.as_deref().unwrap_or("-"), 16),
            route.cwd.as_deref().unwrap_or("-"),
        ));
    }
    Ok(())
}

fn lane_state(live: &Option<tmux::LiveSessions>, route: &Route) -> &'static str {
    match live {
        None => "?",
        Some(sessions) if sessions.has(route.tmux.as_deref().unwrap_or("")) => "live",
        Some(_) => "dead",
    }
}

fn run_lane_get(mail_dir_arg: Option<&Path>, lane: &str) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        anyhow::bail!("no registry route for lane `{lane}`")
    };
    let live = tmux::live_sessions(None);
    println!(
        "{}",
        serde_json::json!({
            "lane": lane,
            "state": lane_state(&live, route),
            "harness": route.harness,
            "tmux": route.tmux,
            "cwd": route.cwd,
            "model": route.model,
            "mode": route.mode,
            "session_id": route.session_id,
        })
    );
    Ok(())
}

/// Stop one lane and drop its route. Refuses when tmux is unreachable. `--route-only`
/// drops the registry row and never touches the pane, so the on-exit epilogue can run inside it.
fn run_lane_delete(mail_dir_arg: Option<&Path>, lane: &str, route_only: bool) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        anyhow::bail!("no registry route for lane `{lane}`")
    };
    if !route_only {
        if let Some(session) = route.tmux.as_deref() {
            match tmux::has_session(None, session) {
                Ok(true) => tmux::kill_session(None, session)?,
                Ok(false) => {}
                Err(error) => anyhow::bail!("tmux unreachable, refusing to delete {lane}: {error}"),
            }
        }
    }
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        current.remove(lane);
        Ok(())
    })?;
    println!("deleted {lane}");
    Ok(())
}

fn run_lane_pane(
    mail_dir_arg: Option<&Path>,
    lane: &str,
    lines: Option<u32>,
    socket: Option<&str>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        anyhow::bail!("no registry route for lane `{lane}`")
    };
    let Some(target) = route.tmux.as_deref() else {
        anyhow::bail!("lane `{lane}` has no tmux session to capture")
    };
    print!("{}", tmux::capture_pane(socket, target, lines)?);
    Ok(())
}

/// `beep ps`, optionally narrowed to one lane.
fn run_ps(mail_dir_arg: Option<&Path>, lane: Option<&str>, all: bool) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let snapshot = proc::SysinfoSnapshot::capture()?;
    line("lane\tpid\trss_kb\tcpu_pct\tuptime_sec\tchildren");
    for (name, route) in &routes {
        if let Some(want) = lane {
            if name != want {
                continue;
            }
        }
        let pane_pid = route
            .tmux
            .as_deref()
            .and_then(|target| tmux::pane_pid(None, target))
            .unwrap_or(0);
        match snapshot.process(pane_pid) {
            Some(info) => println!(
                "{}\t{}\t{}\t{:.1}\t{}\t{}",
                name,
                pane_pid,
                info.rss_bytes / 1024,
                info.cpu_percent,
                info.start_time_secs,
                snapshot.descendent_count(pane_pid),
            ),
            // A dead route prints only when asked for by name or --all.
            None if all || lane.is_some() => {
                println!("{}\t{}\t-\t-\t-\t-", name, pane_pid);
            }
            None => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// pstree
// ---------------------------------------------------------------------------

/// The resolved `from -> to` summon edge for a lane. Explicit beats inferred.
#[derive(Clone, Debug)]
struct LaneEdge {
    /// The summoning lane, `None` for a true root.
    parent: Option<String>,
    /// `true` when the edge came from the first dispatch row, not a route
    /// `--parent`.
    inferred: bool,
}

fn resolve_edges(
    routes: &BTreeMap<String, Route>,
    messages: &[bus::Message],
) -> BTreeMap<String, LaneEdge> {
    routes
        .iter()
        .map(|(name, route)| {
            let edge = match &route.parent {
                Some(parent) => LaneEdge {
                    parent: Some(parent.clone()),
                    inferred: false,
                },
                None => {
                    let summoner = messages
                        .iter()
                        .find(|message| message.kind == "dispatch" && message.to == *name)
                        .and_then(|message| {
                            (!message.from.is_empty()).then(|| message.from.clone())
                        });
                    match summoner {
                        Some(parent) => LaneEdge {
                            parent: Some(parent),
                            inferred: true,
                        },
                        None => LaneEdge {
                            parent: None,
                            inferred: false,
                        },
                    }
                }
            };
            (name.clone(), edge)
        })
        .collect()
}

struct LaneMeta {
    pid: u32,
    state: &'static str,
    descendants: Vec<ProcessDesc>,
}

#[derive(Clone)]
struct ProcessDesc {
    pid: u32,
    comm: String,
}

/// One renderable node: a real lane or a `[gone]` phantom for a summoner that
/// is not itself a known lane.
struct LaneNode {
    name: String,
    parent: Option<String>,
    inferred: bool,
    pid: u32,
    state: &'static str,
    descendants: Vec<ProcessDesc>,
    gone: bool,
    children: Vec<usize>,
}

fn build_lane_nodes(
    edges: &BTreeMap<String, LaneEdge>,
    meta: &BTreeMap<String, LaneMeta>,
    include: &BTreeSet<String>,
) -> Vec<LaneNode> {
    let mut nodes: Vec<LaneNode> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for name in include {
        let lane = meta.get(name).expect("included lane has meta");
        let edge = edges.get(name).expect("included lane has edge");
        let idx = nodes.len();
        nodes.push(LaneNode {
            name: name.clone(),
            parent: edge.parent.clone(),
            inferred: edge.inferred,
            pid: lane.pid,
            state: lane.state,
            descendants: lane.descendants.clone(),
            gone: false,
            children: Vec::new(),
        });
        index.insert(name.clone(), idx);
    }
    let mut phantom: BTreeSet<String> = BTreeSet::new();
    for name in include {
        if let Some(parent) = edges.get(name).and_then(|edge| edge.parent.as_deref()) {
            if !include.contains(parent) {
                phantom.insert(parent.to_owned());
            }
        }
    }
    for name in phantom {
        let idx = nodes.len();
        nodes.push(LaneNode {
            name: name.clone(),
            parent: None,
            inferred: false,
            pid: 0,
            state: "gone",
            descendants: Vec::new(),
            gone: true,
            children: Vec::new(),
        });
        index.insert(name, idx);
    }
    for idx in 0..nodes.len() {
        let parent = nodes[idx].parent.clone();
        if let Some(parent) = parent {
            if let Some(&parent_idx) = index.get(&parent) {
                nodes[parent_idx].children.push(idx);
            }
        }
    }
    let names: Vec<String> = nodes.iter().map(|node| node.name.clone()).collect();
    for node in &mut nodes {
        node.children.sort_by_key(|&child| names[child].clone());
    }
    nodes
}

fn run_pstree(mail_dir_arg: Option<&Path>, all: bool, format: PstreeFormat) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let messages = all_messages(&dir)?;
    let edges = resolve_edges(&routes, &messages);
    let snapshot = proc::SysinfoSnapshot::capture()?;
    let mut meta: BTreeMap<String, LaneMeta> = BTreeMap::new();
    let mut include: BTreeSet<String> = BTreeSet::new();
    for (name, route) in &routes {
        let pane_pid = route
            .tmux
            .as_deref()
            .and_then(|target| tmux::pane_pid(None, target))
            .unwrap_or(0);
        let live = snapshot.process(pane_pid).is_some();
        if !all && !live {
            continue;
        }
        include.insert(name.clone());
        let descendants = snapshot
            .descendants(pane_pid)
            .into_iter()
            .filter_map(|pid| {
                snapshot.process(pid).map(|info| ProcessDesc {
                    pid,
                    comm: info.name,
                })
            })
            .collect();
        meta.insert(
            name.clone(),
            LaneMeta {
                pid: pane_pid,
                state: if live { "live" } else { "dead" },
                descendants,
            },
        );
    }
    let nodes = build_lane_nodes(&edges, &meta, &include);
    match format {
        PstreeFormat::Text => {
            for output in render_text(&nodes) {
                line(&output);
            }
        }
        PstreeFormat::Ndjson => {
            for output in render_ndjson(&nodes) {
                line(&output);
            }
        }
    }
    Ok(())
}

fn render_text(nodes: &[LaneNode]) -> Vec<String> {
    fn emit(out: &mut Vec<String>, nodes: &[LaneNode], idx: usize, depth: usize) {
        let node = &nodes[idx];
        out.push(format!(
            "{}{}",
            "  ".repeat(depth),
            match node.gone {
                true => format!("{} [gone]", node.name),
                false => {
                    let pid = if node.pid == 0 {
                        "-".to_owned()
                    } else {
                        node.pid.to_string()
                    };
                    format!(
                        "{} ({pid}) [{}]{}",
                        node.name,
                        node.state,
                        if node.inferred { " [inferred]" } else { "" }
                    )
                }
            }
        ));
        if !node.gone {
            for desc in &node.descendants {
                out.push(format!(
                    "{}  {} ({})",
                    "  ".repeat(depth + 1),
                    desc.comm,
                    desc.pid
                ));
            }
        }
        for child in &node.children {
            emit(out, nodes, *child, depth + 1);
        }
    }
    let roots: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.parent.is_none())
        .map(|(idx, _)| idx)
        .collect();
    let mut out = Vec::new();
    for root in roots {
        emit(&mut out, nodes, root, 0);
    }
    out
}

fn render_ndjson(nodes: &[LaneNode]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "lane": node.name,
                "parent": node.parent,
                "inferred": node.inferred,
                "pid": if node.gone { None } else { Some(node.pid) },
                "state": node.state,
                "children": node.descendants.iter().map(|desc| desc.pid).collect::<Vec<_>>(),
            })
            .to_string()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// db
// ---------------------------------------------------------------------------

fn run_db(registry: &Registry, cmd: DbCmd) -> Result<()> {
    match cmd {
        DbCmd::Session { cmd } => match cmd {
            SessionCmd::List { limit, format } => {
                let store = open_store()?;
                emit_json_rows(&store.query_sessions(None, limit)?, format);
                Ok(())
            }
            SessionCmd::Get { session, format } => {
                let store = open_store()?;
                emit_json_rows(&store.query_sessions(Some(&session), None)?, format);
                Ok(())
            }
        },
        DbCmd::Turn { cmd } => match cmd {
            TurnCmd::List { query } => run_query(&query),
            TurnCmd::Get {
                session,
                turn,
                format,
            } => {
                let store = open_store()?;
                let filter = ident::TurnQuery {
                    session: Some(session),
                    turn_from: Some(turn),
                    turn_to: Some(turn),
                    ..Default::default()
                };
                emit_json_rows(&store.query_turns(&filter)?, format);
                Ok(())
            }
        },
        DbCmd::Chat { cmd } => match cmd {
            ChatCmd::List { query, all, follow } => run_chat_query(&query, all, follow, false),
        },
        DbCmd::Touch { cmd } => run_fact(query::FactKind::Touch, cmd),
        DbCmd::Command { cmd } => run_fact(query::FactKind::Command, cmd),
        DbCmd::Fetch { cmd } => run_fact(query::FactKind::Fetch, cmd),
        DbCmd::Skill { cmd } => run_fact(query::FactKind::Skill, cmd),
        DbCmd::Pr { cmd } => run_fact(query::FactKind::Pr, cmd),
        DbCmd::Span { cmd } => run_fact(query::FactKind::Span, cmd),
        DbCmd::Edge { cmd } => match cmd {
            EdgeCmd::List { session, limit } => {
                let store = open_store()?;
                emit_edges(&store, session.as_deref(), limit)
            }
        },
        DbCmd::Usage {
            args,
            show_sql,
            cmd,
        } => match cmd {
            None => run_usage(&args, show_sql),
            Some(UsageCmd::Blocks {
                window_hours,
                active,
                args,
            }) => run_usage_blocks(&args, window_hours, active),
            Some(UsageCmd::BurnRate {
                window_minutes,
                args,
            }) => run_usage_burn_rate(&args, window_minutes),
        },
        DbCmd::Price { cmd } => run_price(cmd),
        DbCmd::Sync { cmd } => match cmd {
            SyncCmd::Create { rebuild, forever } => {
                if forever {
                    run_follow(registry)
                } else {
                    run_sync_all(registry, rebuild)
                }
            }
        },
        DbCmd::SyncCursor { cmd } => match cmd {
            CursorCmd::List { limit, format } => {
                let store = open_store()?;
                emit_json_rows(&store.query_sync_cursors(limit)?, format);
                Ok(())
            }
        },
        DbCmd::Status { window, format } => run_status(window, format),
    }
}

fn open_store() -> Result<ident::Store> {
    ident::Store::open(ident::Store::default_path()?)
}

/// `boop db "<sql>": run raw SQL read-only against the store. The open is
/// SQLITE_OPEN_READONLY by flag, so a write is refused by SQLite itself.
fn run_passthrough(sql: &str, format: QueryFormat) -> Result<()> {
    run_passthrough_at(ident::Store::default_path()?, sql, format)
}

fn run_passthrough_at(path: PathBuf, sql: &str, format: QueryFormat) -> Result<()> {
    let store = ident::Store::open_readonly(path)?;
    let (names, rows) = store.passthrough(sql)?;
    match format {
        QueryFormat::Ndjson => {
            for row in &rows {
                line(&serde_json::to_string(row)?);
            }
        }
        QueryFormat::Text => {
            line(&names.join("\t"));
            for row in &rows {
                let Some(object) = row.as_object() else {
                    continue;
                };
                let cells: Vec<String> = names
                    .iter()
                    .map(|name| match object.get(name) {
                        Some(serde_json::Value::String(text)) => text.clone(),
                        Some(serde_json::Value::Null) | None => "-".to_owned(),
                        Some(other) => other.to_string(),
                    })
                    .collect();
                line(&cells.join("\t"));
            }
        }
    }
    Ok(())
}

fn run_fact(kind: query::FactKind, cmd: FactCmd) -> Result<()> {
    let FactCmd::List { args } = cmd;
    let store = open_store()?;
    let filter = query::FactQuery {
        session: args.session.clone(),
        since: args.since,
        until: args.until,
        like: args.like.clone(),
        limit: args.limit,
    };
    emit_json_rows(&store.query_facts(kind, &filter)?, args.format);
    Ok(())
}

/// Liveness is asked of tmux once and joined onto the rows; the store cannot
/// know it and a per-row tmux call would be an N+1.
fn run_status(window_minutes: u64, format: QueryFormat) -> Result<()> {
    let store = open_store()?;
    let now = now_ms();
    let mut rows = store.query_status(window_minutes * 60_000, now)?;
    let dir = mail_dir(None)?;
    let routes = bus::read_routes(&dir).unwrap_or_default();
    let live = tmux::live_sessions(None);
    for row in &mut rows {
        let session = row["session"].as_str().unwrap_or("").to_owned();
        let lane = routes.iter().find(|(_, route)| {
            route.session_id.as_deref() == Some(session.as_str())
                || route.cwd.as_deref() == row["cwd"].as_str()
        });
        let (lane_name, state) = match lane {
            Some((name, route)) => (Some(name.clone()), lane_state(&live, route)),
            None => (None, "unknown"),
        };
        if let Some(object) = row.as_object_mut() {
            object.insert("lane".into(), serde_json::json!(lane_name));
            object.insert("state".into(), serde_json::json!(state));
        }
    }
    emit_json_rows(&rows, format);
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn emit_json_rows(rows: &[ident::Row], format: QueryFormat) {
    match format {
        QueryFormat::Ndjson => {
            for row in rows {
                if let Ok(encoded) = serde_json::to_string(row) {
                    line(&encoded);
                }
            }
        }
        QueryFormat::Text => {
            for row in rows {
                let Some(object) = row.as_object() else {
                    continue;
                };
                let cells: Vec<String> = object
                    .values()
                    .map(|value| match value {
                        serde_json::Value::String(text) => text.clone(),
                        serde_json::Value::Null => "-".to_owned(),
                        other => other.to_string(),
                    })
                    .collect();
                line(&cells.join("\t"));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// whoami
// ---------------------------------------------------------------------------

fn run_whoami(json: bool) -> Result<()> {
    let dir = mail_dir(None)?;
    let routes = bus::read_routes(&dir).unwrap_or_default();
    let identity = identity::resolve(&routes)?;
    if json {
        println!("{}", identity.to_json());
        return Ok(());
    }
    let rung = identity.rung.unwrap_or(identity::Rung::None);
    println!("session  {}", identity.session.as_deref().unwrap_or("-"));
    println!("lane     {}", identity.lane.as_deref().unwrap_or("-"));
    println!("parent   {}", identity.parent.as_deref().unwrap_or("-"));
    println!("harness  {}", identity.harness.as_deref().unwrap_or("-"));
    println!("pane     {}", identity.pane.as_deref().unwrap_or("-"));
    println!("rung     {} ({})", rung.as_str(), rung.confidence());
    Ok(())
}

/// The `db usage` alias's report SQL: totals with cost over the whole store.
/// The passthrough is the engine; `--show-sql` prints this const.
const USAGE_TOTALS_SQL: &str = "
SELECT COUNT(*) AS calls,
       COALESCE(SUM(usage.input_tokens), 0) AS input_tokens,
       COALESCE(SUM(usage.output_tokens), 0) AS output_tokens,
       COALESCE(SUM(usage.cache_create_5m_tokens), 0) AS cache_create_5m_tokens,
       COALESCE(SUM(usage.cache_create_1h_tokens), 0) AS cache_create_1h_tokens,
       COALESCE(SUM(usage.cache_read_tokens), 0) AS cache_read_tokens,
       SUM(usage.input_tokens / 1e6 * price.input_per_mtok
         + usage.output_tokens / 1e6 * price.output_per_mtok
         + usage.cache_create_5m_tokens / 1e6 * price.cache_write_5m_per_mtok
         + usage.cache_create_1h_tokens / 1e6 * price.cache_write_1h_per_mtok
         + usage.cache_read_tokens / 1e6 * price.cache_read_per_mtok) AS cost_usd
FROM agent_usage AS usage
LEFT JOIN model_price AS price ON price.model_id = usage.model_id";

fn open_ro_store() -> Result<ident::Store> {
    ident::Store::open_readonly(ident::Store::default_path()?)
}

/// `db usage`: the totals report, a thin alias over USAGE_TOTALS_SQL. `--show-sql`
/// prints that const and exits; otherwise it runs through the read-only passthrough.
fn run_usage(args: &UsageArgs, show_sql: bool) -> Result<()> {
    if show_sql {
        line(USAGE_TOTALS_SQL.trim());
        return Ok(());
    }
    run_passthrough(USAGE_TOTALS_SQL, args.format)
}

fn run_usage_blocks(args: &UsageArgs, window_hours: u64, active_only: bool) -> Result<()> {
    let store = open_ro_store()?;
    let window_ms = (window_hours * 3_600_000) as i64;
    let blocks = store.usage_blocks(window_ms, &usage::UsageQuery::default())?;
    let now = now_ms() as i64;
    let rows: Vec<ident::Row> = blocks
        .iter()
        .filter(|block| !active_only || block.last_ts + window_ms > now)
        .map(|block| {
            serde_json::json!({
                "window_start": block.window_start,
                "first_ts": block.first_ts,
                "last_ts": block.last_ts,
                "calls": block.calls,
                "total_tokens": block.total_tokens,
                "is_gap": block.is_gap,
                "is_active": !block.is_gap && block.last_ts + window_ms > now,
            })
        })
        .collect();
    emit_json_rows(&rows, args.format);
    if let Some(ceiling) = usage::p90_ceiling(&blocks) {
        line(&serde_json::json!({ "p90_token_ceiling": ceiling }).to_string());
    }
    Ok(())
}

fn run_usage_burn_rate(args: &UsageArgs, window_minutes: u64) -> Result<()> {
    let store = open_ro_store()?;
    let filter = usage::UsageQuery {
        since: Some(now_ms().saturating_sub(window_minutes * 60_000)),
        ..Default::default()
    };
    emit_json_rows(&store.usage_burn_rate(&filter)?, args.format);
    Ok(())
}

fn run_price(cmd: PriceCmd) -> Result<()> {
    let store = open_store()?;
    match cmd {
        PriceCmd::List => {
            emit_json_rows(&store.price_list()?, QueryFormat::Ndjson);
            Ok(())
        }
        PriceCmd::Set {
            model,
            input_per_mtok,
            output_per_mtok,
            cache_write_5m_per_mtok,
            cache_write_1h_per_mtok,
            cache_read_per_mtok,
            source,
        } => {
            store.price_set(&usage::ModelPrice {
                model: &model,
                input_per_mtok,
                output_per_mtok,
                cache_write_5m_per_mtok,
                cache_write_1h_per_mtok,
                cache_read_per_mtok,
                source: &source,
            })?;
            line(&format!("priced {model}"));
            Ok(())
        }
    }
}
