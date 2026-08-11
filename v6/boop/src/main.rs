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
use boop::{bus, ident, identity, lane, proc, query, tmux, usage};

const DOCTRINE: &str = "\
DOCTRINE (this help is the usage contract; agents read it with `boop --help`):

SPAWN: every lane spawn goes through lane create; bare tmux spawns leave no
edge and stay invisible to tracking:
    boop beep lane create --branch feature/<name> --brief <abs-path> \\
      [--goal <text>] [--model <m>] [--wait] [--mail-dir <d>] [--dry-run]
  ONE derivation, from the whole branch name: `feature/schema-emit` gives lane
  id and tmux session `feature-schema-emit` (`/` spelled `-`, the one character
  tmux cannot hold) and worktree `.boop-worktrees/feature/schema-emit` (the same
  name as a path). No prefix is dropped and no `lane/` prefix is added.
  Kinds are feature/fix/refactor/chore, a convention the CLI prints, not a gate.
  --cwd defaults to the repo you stand in, --base-sha to origin/main's head
  (resolved at spawn and printed), --parent to you then to the one registered
  coordinator, --harness to the one the model spelling names (gpt-* codex,
  provider/model opencode, kimi-* kimi).
  Overrides: --lane <id>, --tmux <name>, --base-sha <sha>, --harness <id>.
  One shot: worktree at base sha + spawn + route registration.
  Always --dry-run first; the printed `cmd:` line is the literal spawn.

COMPLETION: --parent appends an on-exit hail `lane <id> done rc=$rc` into the
  parent's mailbox. A lane spawned with --parent reports completion; do not poll.
  `--wait` blocks on that row and exits with the lane's rc, so spawn-and-join is
  one command; `--wait-timeout <s>` (default 3600, 0 waits forever) exits 124.
  The same wait after the fact is `boop beep lane wait <lane>`.

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
                name: Some(name),
                cwd: Some(cwd),
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
                wait: false,
                wait_timeout: 0,
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
    let routes = bus::read_routes(&mail_dir(None)?).unwrap_or_default();
    store.begin()?;
    let result = (|| {
        let mut stat = ident::SyncStat::default();
        let offsets = store.all_cursor_offsets()?;
        for adapter in registry.all() {
            for session in adapter.sessions()? {
                // Freshness gate: a transcript whose length still equals its
                // consumed cursor needs no re-read (walking+stat-ing all files
                // costs ~0.02s; opening every 2.86 GB corpus does not). A
                // shorter file is the existing truncation path, left to sync.
                let key = session.path.display().to_string();
                let at_cursor = offsets
                    .get(&(session.session_id.clone(), key))
                    .copied()
                    .unwrap_or(0);
                let unchanged = std::fs::metadata(&session.path)
                    .map(|meta| meta.len() == at_cursor)
                    .unwrap_or(false);
                if unchanged {
                    continue;
                }
                let pid = session_route_pid(&routes, &session);
                stat.add(ident::sync_session_with_pid(
                    &store,
                    adapter.as_ref(),
                    &session,
                    pid,
                )?);
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
    let routes = bus::read_routes(&mail_dir(None)?).unwrap_or_default();
    loop {
        let store = ident::Store::open(ident::Store::default_path()?)?;
        store.begin()?;
        for (harness_id, session) in &sessions {
            let mtime = file_mtime_ms(&session.path).unwrap_or(0);
            if last_mtime.get(&session.session_id).copied().unwrap_or(0) == mtime {
                continue;
            }
            let adapter = harness_by_id(registry, harness_id)?;
            let pid = session_route_pid(&routes, session);
            let _ = ident::sync_session_with_pid(&store, adapter, session, pid)?;
            last_mtime.insert(session.session_id.clone(), mtime);
        }
        store.commit()?;
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// The pane pid for a session that maps to a lane route (by session id or cwd).
/// A session with no route owns no process and yields `None`, never a guess.
fn session_route_pid(
    routes: &BTreeMap<String, bus::Route>,
    session: &boop::harness::SessionRef,
) -> Option<i64> {
    let route = routes
        .iter()
        .find(|(_, route)| session_matches_route(route, session));
    route
        .and_then(|(_, route)| route.tmux.as_deref())
        .and_then(|target| tmux::pane_pid(None, target))
        .map(i64::from)
}

/// A routeless session must never borrow a pid: two `None` cwds are not a
/// match, only a shared session id or a shared concrete cwd is.
fn session_matches_route(route: &bus::Route, session: &boop::harness::SessionRef) -> bool {
    route.session_id.as_deref() == Some(session.session_id.as_str())
        || (route.cwd.is_some() && route.cwd.as_deref() == session.cwd.as_deref())
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
        match snapshot.tree_sum(pane_pid) {
            Some(sum) => {
                let now = now_unix_secs();
                let uptime = proc::uptime_secs(sum.start_time_secs, now);
                line(&format!(
                    "{}\t{}\t{}\t{:.1}\t{}\t{}",
                    name,
                    pane_pid,
                    sum.rss_bytes / 1024,
                    sum.cpu_percent,
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
        env_stamp: Some(spawn_env_stamp(
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
        registered_at: Some(bus::now_iso()),
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

/// The environment a spawn's command carries: a UTF-8 locale, then the child's
/// own identity. The pane's inherited locale is the tmux server's, not a shell's.
fn spawn_env_stamp(lane_id: &str, harness_id: &str, caller_session: Option<&str>) -> String {
    format!(
        "{} {}",
        lane::locale_stamp(),
        identity::child_stamp(lane_id, lane_id, harness_id, caller_session)
    )
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
    name: Option<String>,
    cwd: Option<String>,
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
    wait: bool,
    wait_timeout: u64,
}

/// Register and spawn a lane. No match on harness id here; the adapter's own
/// `spawn`/`preview_command` decides how `prompt` becomes a real invocation.
fn run_lane(registry: &Registry, args: LaneArgs) -> Result<()> {
    let harness_id = lane::harness_for_spawn(args.harness.as_deref(), args.model.as_deref())?;
    let adapter = harness_by_id(registry, &harness_id)?;
    let repo = match &args.cwd {
        Some(cwd) => PathBuf::from(cwd),
        None => lane::repo_root(&std::env::current_dir().context("read the current directory")?)?,
    };
    let identity = lane::derive(
        &repo,
        args.branch.as_deref(),
        args.name.as_deref(),
        args.tmux.as_deref(),
    )?;
    let worktree_mode = identity.worktree_dir.is_some();
    let brief = args.brief.clone().unwrap_or_else(|| repo.join("brief.md"));
    // The flash4 default belongs to opencode's spelling; another harness gets
    // no model rather than one it cannot resolve.
    let model = match (args.model.clone(), harness_id.as_str()) {
        (Some(model), _) => Some(model),
        (None, "opencode") => Some(DEFAULT_LANE_MODEL.to_owned()),
        (None, _) => None,
    };
    let prompt = brief.display().to_string();
    // A worktree branches from origin/main unless pinned; the repo-tree shape
    // keeps its own HEAD, where a base of origin/main would be a merge.
    let base = match (&args.base_sha, worktree_mode) {
        (Some(sha), _) => lane::BaseSha {
            sha: sha.clone(),
            rev: "--base-sha".to_owned(),
        },
        (None, true) => lane::default_base_sha(&repo)?,
        (None, false) => lane::BaseSha {
            sha: git_head(&repo.display().to_string())?.unwrap_or_else(|| "HEAD".into()),
            rev: "HEAD".to_owned(),
        },
    };
    let hail_mail_dir = mail_dir(args.mail_dir.as_deref())?;
    let routes = bus::read_routes(&hail_mail_dir)?;
    let caller = identity::resolve(&routes)?;
    let caller_lane = caller.lane.clone().filter(|lane| *lane != identity.lane);
    let parent = lane::resolve_parent(args.parent.as_deref(), caller_lane.as_deref(), &routes);
    // The epilogue runs in the lane's pane: the sender is the lane, and the
    // mailbox must be the one this dispatch registered in, never the default.
    let on_exit = parent.parent.as_ref().map(|parent| {
        format!(
            "boop hail --to {} --from {} --mail-dir {} --kind result --body \"lane {} done rc=$__rc\" ; boop beep lane delete {} --route-only --mail-dir {}",
            shell_quote(parent),
            shell_quote(&identity.lane),
            shell_quote(&hail_mail_dir.display().to_string()),
            identity.lane,
            shell_quote(&identity.lane),
            shell_quote(&hail_mail_dir.display().to_string()),
        )
    });

    if args.dry_run {
        let spec = boop::harness::SpawnSpec {
            harness: harness_id.clone(),
            branch: identity.branch.clone(),
            base_sha: base.sha.clone(),
            main_tree: !worktree_mode,
            setup: Vec::new(),
            prompt: prompt.clone(),
            resume_session: None,
            socket: args.socket.clone(),
            worktree_dir: identity.worktree_dir.clone(),
            repo: repo.clone(),
            env_stamp: Some(spawn_env_stamp(
                &identity.lane,
                &harness_id,
                caller.session.as_deref(),
            )),
            model: model.clone(),
            on_exit: on_exit.clone(),
            tmux: Some(identity.tmux.clone()),
        };
        let command = adapter
            .preview_command(&spec)
            .unwrap_or_else(|| format!("{} {}", adapter.id(), shell_quote(&prompt)));
        println!("cmd: {command}");
        println!("to: {}", identity.lane);
        println!("cwd: {}", repo.display());
        println!("harness: {harness_id}");
        match lane::kind_of(&identity.branch) {
            Some(kind) => println!("branch: {} (kind {kind})", identity.branch),
            None => println!("branch: {}", identity.branch),
        }
        if let Some(worktree_dir) = &identity.worktree_dir {
            println!("worktree: {}", worktree_dir.display());
        }
        println!("base-sha: {} (from {})", base.sha, base.rev);
        println!("tmux: {}", identity.tmux);
        match &parent.parent {
            Some(name) => println!(
                "parent: {name} (from {}; completion hail appended on exit)",
                parent.source
            ),
            None => println!("parent: - (no completion hail; pass --parent <lane>)"),
        }
        if let Some(goal) = &args.goal {
            println!("goal: {goal}");
        }
        if args.wait {
            println!(
                "wait: for {} result, timeout {}s",
                identity.lane, args.wait_timeout
            );
        }
        return Ok(());
    }
    if args.wait && parent.parent.is_none() {
        anyhow::bail!(
            "--wait needs a parent: the result row it blocks on is written by the on-exit hail, which only exists with --parent <lane>"
        );
    }
    let lane_id = identity.lane.clone();
    run_dispatch(
        registry,
        DispatchArgs {
            to: identity.lane,
            cwd: repo.display().to_string(),
            cmd: prompt,
            from: None,
            harness: Some(harness_id),
            session_id: None,
            model,
            mode: Some("auto".into()),
            tmux: Some(identity.tmux),
            socket: args.socket,
            body: Some(format!(
                "Read and execute the lane brief at {}",
                brief.display()
            )),
            r#ref: Some(brief.display().to_string()),
            mail_dir: args.mail_dir,
            resolve_wait: 3,
            main_tree: !worktree_mode,
            base_sha: Some(base.sha),
            branch: Some(identity.branch),
            worktree_dir: identity.worktree_dir,
            parent: parent.parent,
            goal: args.goal.clone(),
            on_exit,
        },
    )?;
    if args.wait {
        // Same code path as `beep lane wait`, which exits with the lane's rc.
        return run_lane_wait(Some(&hail_mail_dir), &lane_id, args.wait_timeout);
    }
    Ok(())
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
        registered_at: Some(bus::now_iso()),
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
    if let Some(registered_at) = &route.registered_at {
        object.insert("registeredAt".into(), serde_json::json!(registered_at));
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
    use std::process::Command;

    use super::{
        append_message, dead_reason, resolve_dispatch_harness, run_lane_delete, run_lane_prune,
        session_matches_route, write_route,
    };
    use boop::bus::{read_routes, Route};
    use boop::proc::SysinfoSnapshot;
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
                registered_at: None,
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

    fn unique_name(prefix: &str) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn tmux_route(tmux_name: &str) -> Route {
        Route {
            harness: Some("claude".into()),
            tmux: Some(tmux_name.to_owned()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
        }
    }

    /// A real session on the default tmux server, killed on drop; `lane
    /// prune` hardcodes the default server, so a "live" fixture needs one.
    struct LiveTmuxSession {
        name: String,
    }

    impl LiveTmuxSession {
        fn new(name: &str) -> Self {
            let status = Command::new("tmux")
                .args(["new-session", "-d", "-s", name])
                .status()
                .expect("tmux installed and reachable");
            assert!(status.success(), "failed to create live session {name}");
            LiveTmuxSession {
                name: name.to_owned(),
            }
        }
    }

    impl Drop for LiveTmuxSession {
        fn drop(&mut self) {
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", &self.name])
                .status();
        }
    }

    /// FAIL-FIRST. Before `run_lane_prune` existed this had no callee to
    /// assert against; now: a dead row is gone, a live row survives.
    #[test]
    fn prune_removes_a_dead_row_and_keeps_a_live_one() {
        let dir = temp_mail_dir();
        let live_name = unique_name("boop-prune-live");
        let _session = LiveTmuxSession::new(&live_name);
        write_route(&dir, "dead-lane", tmux_route(&unique_name("boop-prune-dead"))).unwrap();
        write_route(&dir, "live-lane", tmux_route(&live_name)).unwrap();

        run_lane_prune(Some(&dir), false).unwrap();

        let routes = read_routes(&dir).unwrap();
        assert!(!routes.contains_key("dead-lane"), "a dead row must be pruned");
        assert!(routes.contains_key("live-lane"), "a live row must survive prune");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. `--dry-run` reports the same rows a real run would prune but
    /// removes nothing.
    #[test]
    fn prune_dry_run_removes_nothing() {
        let dir = temp_mail_dir();
        write_route(&dir, "dead-lane", tmux_route(&unique_name("boop-prune-dead"))).unwrap();

        run_lane_prune(Some(&dir), true).unwrap();

        let routes = read_routes(&dir).unwrap();
        assert!(routes.contains_key("dead-lane"), "--dry-run must remove nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dead_reason_names_a_gone_session_with_no_pid() {
        let snapshot = SysinfoSnapshot::capture().unwrap();
        let route = tmux_route(&unique_name("boop-prune-nonexistent"));
        assert_eq!(
            dead_reason(&route, &snapshot).as_deref(),
            Some("tmux session gone, no pid recorded")
        );
    }

    #[test]
    fn dead_reason_is_none_for_a_live_session() {
        let name = unique_name("boop-prune-alive");
        let _session = LiveTmuxSession::new(&name);
        let snapshot = SysinfoSnapshot::capture().unwrap();
        assert_eq!(dead_reason(&tmux_route(&name), &snapshot), None);
    }

    #[test]
    fn dead_reason_names_no_recorded_session_when_tmux_is_absent() {
        let snapshot = SysinfoSnapshot::capture().unwrap();
        let route = Route {
            harness: Some("claude".into()),
            tmux: None,
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
        };
        assert_eq!(
            dead_reason(&route, &snapshot).as_deref(),
            Some("no tmux session recorded")
        );
    }

    fn result_message(id: &str, lane: &str, rc: i32) -> boop::bus::Message {
        boop::bus::Message {
            id: id.into(),
            from: lane.into(),
            to: lane.into(),
            from_timestamp: "2026-08-01T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: "result".into(),
            reply_to: None,
            body: format!("lane {lane} done rc={rc}"),
            r#ref: None,
        }
    }

    fn registered_route(ts: &str) -> Route {
        Route {
            harness: Some("claude".into()),
            tmux: Some("l".into()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: Some(ts.into()),
        }
    }

    /// RECEIPT (was wait_returns_rc_from_a_preexisting_result_row; pre-fix
    /// rc=0). Older than the spawn's registration: skipped, times out (124).
    #[test]
    fn wait_skips_a_result_row_older_than_the_current_spawn() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        append_message(&dir, &result_message("m-1", "l", 5)).unwrap();
        write_route(&dir, "l", registered_route("2026-08-02T00:00:00.000Z")).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "l",
                Some(std::time::Duration::from_millis(60)),
                std::time::Duration::from_millis(10),
            ),
            None,
            "an older row is skipped, so the wait times out (exits 124)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A result row at or after the current spawn's registration satisfies the
    /// wait immediately with the rc its body names.
    #[test]
    fn wait_accepts_a_result_row_after_the_current_spawn() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        write_route(&dir, "l", registered_route("2026-08-01T00:00:00.000Z")).unwrap();
        let mut message = result_message("m-2", "l", 7);
        message.from_timestamp = "2026-08-02T00:00:00.000Z".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "l",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(10),
            ),
            Some(7)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (contract 3). No route row survives, so any result row
    /// satisfies: the after-the-fact read.
    #[test]
    fn wait_accepts_a_result_row_with_no_route_registered() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-3", "l", 4);
        message.from_timestamp = "2026-07-01T00:00:00.000Z".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "l",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(10),
            ),
            Some(4)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (Job 3). An empty mailbox times out to `None`, which the verb
    /// maps to exit code 124.
    #[test]
    fn wait_times_out_when_no_result_row_arrives() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let outcome = super::wait_for_result(
            &dir,
            "l",
            Some(std::time::Duration::from_millis(60)),
            std::time::Duration::from_millis(10),
        );
        assert_eq!(
            outcome, None,
            "timeout returns the None the verb exits 124 on"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A non-result row for the lane never satisfies the wait.
    #[test]
    fn a_non_result_row_does_not_satisfy_the_wait() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-2", "l", 3);
        message.kind = "note".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "l"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. The on-exit epilogue hails `--to <parent> --from <lane>`, so a
    /// wait keyed on the recipient never saw the row it exists to wait for.
    #[test]
    fn wait_matches_the_row_the_on_exit_epilogue_actually_writes() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-3", "feature-schema-emit", 0);
        message.to = "sprefa-coordinator".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "feature-schema-emit"), Some(0));
        assert_eq!(
            super::wait_for_result(
                &dir,
                "feature-schema-emit",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(10),
            ),
            Some(0)
        );
        assert_eq!(
            super::lane_result_rc(&dir, "some-other-lane"),
            None,
            "another lane's completion never satisfies this wait"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A lane that fails hands its rc back through the same row, and
    /// an absent row is the 124 timeout `--wait-timeout` exits on.
    #[test]
    fn wait_propagates_a_failing_rc_and_times_out_otherwise() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "feature-schema-emit",
                Some(std::time::Duration::from_millis(40)),
                std::time::Duration::from_millis(10),
            ),
            None,
            "no result row yet: the verb exits 124 on this None"
        );
        let mut message = result_message("m-4", "feature-schema-emit", 17);
        message.to = "sprefa-coordinator".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "feature-schema-emit"), Some(17));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A registry row written before the branch-derived names (lane id
    /// with no kind, `lane/*` worktree cwd) still reads, resolves and deletes.
    #[test]
    fn a_pre_branch_registry_row_still_reads_and_deletes() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("registry.json"),
            r#"{
  "boop-sql": {
    "harness": "opencode",
    "tmux": "boop-sql",
    "cwd": "/Users/x/projects/sprefa/.boop-worktrees/lane/boop-sql",
    "model": "openrouter/deepseek/deepseek-v4-flash-0731",
    "mode": "auto",
    "sessionId": "ses_0167"
  },
  "sprefa-coordinator": { "harness": "claude", "tmux": "shell:0.0" }
}"#,
        )
        .unwrap();
        let routes = read_routes(&dir).unwrap();
        let old = &routes["boop-sql"];
        assert_eq!(old.session_id.as_deref(), Some("ses_0167"));
        assert_eq!(old.tmux.as_deref(), Some("boop-sql"));
        assert!(old
            .cwd
            .as_deref()
            .unwrap()
            .contains(".boop-worktrees/lane/"));
        assert_eq!(
            boop::lane::resolve_parent(None, None, &routes)
                .parent
                .as_deref(),
            Some("sprefa-coordinator"),
            "an old row is still a usable parent default"
        );
        run_lane_delete(Some(&dir), "boop-sql", true).unwrap();
        let after = read_routes(&dir).unwrap();
        assert!(!after.contains_key("boop-sql"));
        assert!(
            after.contains_key("sprefa-coordinator"),
            "deleting one old row leaves the others"
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
            registered_at: None,
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

    fn session_with_cwd(cwd: Option<&str>) -> boop::harness::SessionRef {
        boop::harness::SessionRef {
            harness: "opencode",
            session_id: "ses-1".into(),
            nickname: "ses-1".into(),
            path: std::path::PathBuf::from("/tmp/x.jsonl"),
            cwd: cwd.map(str::to_owned),
            git_branch: None,
            modified_ms: 0,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        }
    }

    #[test]
    fn none_cwd_on_both_sides_is_not_a_route_match() {
        let mut route = route_with(None);
        route.cwd = None;
        assert!(!session_matches_route(&route, &session_with_cwd(None)));
    }

    #[test]
    fn shared_concrete_cwd_matches_and_none_session_cwd_does_not() {
        let mut route = route_with(None);
        route.cwd = Some("/repo/wt".into());
        assert!(session_matches_route(
            &route,
            &session_with_cwd(Some("/repo/wt"))
        ));
        assert!(!session_matches_route(&route, &session_with_cwd(None)));
    }

    #[test]
    fn session_id_match_needs_no_cwd() {
        let mut route = route_with(None);
        route.session_id = Some("ses-1".into());
        route.cwd = None;
        assert!(session_matches_route(&route, &session_with_cwd(None)));
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
            registered_at: None,
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
        let nodes = super::build_lane_nodes(&routes, &edges, &meta, &include);
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

    /// RECEIPT (job 2). A route's goal rides the lane line as a ` -- <goal>`
    /// suffix and the ndjson row as a `goal` string.
    #[test]
    fn pstree_carries_the_goal() {
        let mut routes = BTreeMap::new();
        routes.insert(
            "child".into(),
            Route {
                harness: Some("opencode".into()),
                tmux: Some("lane-x".into()),
                cwd: None,
                model: None,
                mode: None,
                session_id: None,
                source_path: None,
                parent: None,
                goal: Some("ship the edge".into()),
                registered_at: None,
            },
        );
        let messages = vec![dispatch("coordinator", "child")];
        let edges = super::resolve_edges(&routes, &messages);
        let mut meta = BTreeMap::new();
        meta.insert("child".into(), live_meta(4242));
        let mut include = BTreeSet::new();
        include.insert("child".into());
        let nodes = super::build_lane_nodes(&routes, &edges, &meta, &include);
        let text = super::render_text(&nodes).join("\n");
        assert!(
            text.contains("child (4242) [live] [inferred] -- ship the edge"),
            "text:\n{text}"
        );
        let ndjson = super::render_ndjson(&nodes);
        let row = &ndjson[0];
        assert!(row.contains("\"goal\":\"ship the edge\""), "row: {row}");
    }

    /// RECEIPT (job 2). A lane without a goal renders no text suffix and a
    /// null ndjson goal.
    #[test]
    fn pstree_goal_null_when_absent() {
        let mut routes = BTreeMap::new();
        routes.insert("loner".into(), route_with(None));
        let edges = super::resolve_edges(&routes, &[]);
        let mut meta = BTreeMap::new();
        meta.insert("loner".into(), live_meta(7));
        let mut include = BTreeSet::new();
        include.insert("loner".into());
        let nodes = super::build_lane_nodes(&routes, &edges, &meta, &include);
        let text = super::render_text(&nodes).join("\n");
        assert!(!text.contains(" -- "), "text:\n{text}");
        let row = &super::render_ndjson(&nodes)[0];
        assert!(row.contains("\"goal\":null"), "row: {row}");
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
        /// The lane's whole identity: `feature/<name>`, also fix/, refactor/,
        /// chore/. Lane id and tmux session are the branch with `/` as `-`.
        #[arg(long)]
        branch: Option<String>,
        /// Absolute path to the brief the lane reads and executes.
        #[arg(long)]
        brief: Option<PathBuf>,
        /// What the lane is running toward.
        #[arg(long)]
        goal: Option<String>,
        /// Repo to branch from; defaults to the repo the caller stands in.
        #[arg(long)]
        cwd: Option<String>,
        /// Defaults to origin/main's head, resolved and printed at spawn.
        #[arg(long)]
        base_sha: Option<String>,
        /// Defaults to the caller, then to the one registered coordinator.
        #[arg(long)]
        parent: Option<String>,
        /// Defaults to the harness the model spelling names.
        #[arg(long)]
        harness: Option<String>,
        #[arg(long)]
        model: Option<String>,
        /// Block until the lane's on-exit result row lands, then exit with its
        /// rc. Needs a parent, since that hail is what writes the row.
        #[arg(long)]
        wait: bool,
        /// Seconds `--wait` blocks before exiting 124; 0 waits forever.
        #[arg(long, default_value_t = 3600)]
        wait_timeout: u64,
        /// Overrides the lane id derived from `--branch`.
        #[arg(long)]
        lane: Option<String>,
        /// Overrides the tmux session name derived from `--branch`.
        #[arg(long)]
        tmux: Option<String>,
        /// tmux socket to spawn on; a throwaway socket for tests, `None` for
        /// the default server.
        #[arg(long)]
        socket: Option<String>,
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
    /// Drop routes whose tmux session is gone AND whose recorded pid, if any,
    /// is not alive. Refuses when tmux is unreachable.
    Prune {
        /// Print what would be pruned; remove nothing.
        #[arg(long)]
        dry_run: bool,
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
    /// Wait for the lane's result row, then exit with the rc it names. `--timeout`
    /// seconds exits 124; a row that already exists returns its rc immediately.
    Wait {
        lane: String,
        /// Seconds to wait before exiting 124; 0 waits forever.
        #[arg(long, default_value_t = 0)]
        timeout: u64,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
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
            wait,
            wait_timeout,
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
                wait,
                wait_timeout,
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
        LaneCmd::Prune { dry_run, mail_dir } => run_lane_prune(mail_dir.as_deref(), dry_run),
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
        LaneCmd::Wait {
            lane,
            timeout,
            mail_dir,
        } => run_lane_wait(mail_dir.as_deref(), &lane, timeout),
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

/// Bulk-drop dead rows. Registry-only: bus.ndjson is never touched.
fn run_lane_prune(mail_dir_arg: Option<&Path>, dry_run: bool) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    if tmux::live_sessions(None).is_none() {
        anyhow::bail!("tmux unreachable, cannot tell live from dead");
    }
    let routes = bus::read_routes(&dir)?;
    let snapshot = proc::SysinfoSnapshot::capture()?;
    let dead: Vec<(String, String, String)> = routes
        .iter()
        .filter_map(|(name, route)| {
            let why = dead_reason(route, &snapshot)?;
            Some((name.clone(), route.tmux.clone().unwrap_or_else(|| "-".into()), why))
        })
        .collect();
    for (name, tmux_name, why) in &dead {
        line(&format!("{name} {tmux_name} {why}"));
    }
    if dry_run {
        line(&format!("{} lane(s) would be pruned (dry run)", dead.len()));
        return Ok(());
    }
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        for (name, _, _) in &dead {
            current.remove(name);
        }
        Ok(())
    })?;
    line(&format!("{} lane(s) pruned", dead.len()));
    Ok(())
}

/// `None` when the route's tmux target is live; `Some(reason)` when the tmux
/// target is gone and its resolvable pid, if any, is also not alive.
fn dead_reason(route: &Route, snapshot: &proc::SysinfoSnapshot) -> Option<String> {
    let Some(target) = route.tmux.as_deref() else {
        return Some("no tmux session recorded".to_owned());
    };
    if tmux::target_alive(None, target) {
        return None;
    }
    match tmux::pane_pid(None, target) {
        Some(pid) if snapshot.is_alive(pid) => None,
        Some(pid) => Some(format!("tmux session gone, pid {pid} not alive")),
        None => Some("tmux session gone, no pid recorded".to_owned()),
    }
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

/// `beep lane wait`: poll for a `kind=result` row from `lane`, exit with its
/// rc; `--timeout` seconds exits 124, a pre-existing row returns immediately.
fn run_lane_wait(mail_dir_arg: Option<&Path>, lane: &str, timeout_secs: u64) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let deadline = if timeout_secs == 0 {
        None
    } else {
        Some(std::time::Duration::from_secs(timeout_secs))
    };
    match wait_for_result(&dir, lane, deadline, std::time::Duration::from_secs(1)) {
        Some(rc) => std::process::exit(rc),
        None => std::process::exit(124),
    }
}

/// The rc from the lane's most recent `kind=result` mailbox row (`lane <id>
/// done rc=N`), `None` when no result row for that lane exists yet. Without a
/// route row the wait is after-the-fact: any result row satisfies.
#[cfg(test)]
fn lane_result_rc(dir: &std::path::Path, lane: &str) -> Option<i32> {
    lane_result_rc_since(dir, lane, None)
}

/// As `lane_result_rc`, but only a result row at or after `since` (ms since
/// epoch) satisfies; older rows belong to a previous spawn and are skipped.
fn lane_result_rc_since(dir: &std::path::Path, lane: &str, since: Option<u64>) -> Option<i32> {
    let mut messages = Vec::new();
    for box_path in bus::read_boxes(dir).unwrap_or_default() {
        messages.extend(bus::parse_box(&box_path));
    }
    let folded = bus::fold(&messages);
    folded
        .iter()
        .rev()
        // The on-exit epilogue hails `--to <parent> --from <lane>`, so the
        // lane that finished is the sender; `to` matches a hand-addressed row.
        .find(|message| {
            if message.kind != "result" || (message.from != lane && message.to != lane) {
                return false;
            }
            match since {
                Some(boundary) => parse_iso_ms(&message.from_timestamp).unwrap_or(0) >= boundary,
                None => true,
            }
        })
        .and_then(|message| parse_result_rc(&message.body))
}

/// The lane's registration timestamp (ms since epoch) for the spawn that
/// wrote the current route row; `None` when no route row exists.
fn route_registered_at(dir: &std::path::Path, lane: &str) -> Option<u64> {
    bus::read_routes(dir)
        .ok()?
        .get(lane)
        .and_then(|route| route.registered_at.as_deref())
        .and_then(parse_iso_ms)
}

fn parse_result_rc(body: &str) -> Option<i32> {
    body.split_whitespace().find_map(|token| {
        token
            .strip_prefix("rc=")
            .and_then(|value| value.parse().ok())
    })
}

/// Poll `lane_result_rc` every `interval` until a result appears or `deadline`
/// passes. `None` on deadline is a timeout; `since` bounds satisfying rows.
fn wait_for_result(
    dir: &std::path::Path,
    lane: &str,
    deadline: Option<std::time::Duration>,
    interval: std::time::Duration,
) -> Option<i32> {
    let since = route_registered_at(dir, lane);
    let start = std::time::Instant::now();
    loop {
        if let Some(rc) = lane_result_rc_since(dir, lane, since) {
            return Some(rc);
        }
        if deadline.is_some_and(|limit| start.elapsed() >= limit) {
            return None;
        }
        std::thread::sleep(interval);
    }
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
        match snapshot.tree_sum(pane_pid) {
            Some(sum) => {
                let now = now_unix_secs();
                let uptime = proc::uptime_secs(sum.start_time_secs, now);
                println!(
                    "{}\t{}\t{}\t{:.1}\t{}\t{}",
                    name,
                    pane_pid,
                    sum.rss_bytes / 1024,
                    sum.cpu_percent,
                    uptime,
                    snapshot.descendent_count(pane_pid),
                );
            }
            // A dead route prints only when asked for by name or --all.
            None if all || lane.is_some() => {
                println!("{}\t{}\t-\t-\t-\t-", name, pane_pid);
            }
            None => {}
        }
    }
    Ok(())
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
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
    goal: Option<String>,
    gone: bool,
    children: Vec<usize>,
}

fn build_lane_nodes(
    routes: &BTreeMap<String, Route>,
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
            goal: routes.get(name).and_then(|route| route.goal.clone()),
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
            goal: None,
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
    let nodes = build_lane_nodes(&routes, &edges, &meta, &include);
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
                        "{} ({pid}) [{}]{}{}",
                        node.name,
                        node.state,
                        if node.inferred { " [inferred]" } else { "" },
                        match &node.goal {
                            Some(goal) => format!(" -- {goal}"),
                            None => String::new(),
                        }
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
                "goal": node.goal,
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
