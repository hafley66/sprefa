//! `boop`: the cross-harness agent-event reader, 1-1 with `bus` plus the four
//! verbs `bus` cannot do (read what an agent did, and measure what its
//! processes cost). The CLI routes to layers 0-3; it contains no `match` on
//! harness id and no direct `Command::new("tmux")` beyond the layer-1 helpers.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use boop::bus::Route;
use boop::event::AgentEvent;
use boop::proc::ProcReader;
use boop::registry::Registry;
use boop::{bus, ident, identity, proc, query, tmux, usage};







#[derive(Parser)]
#[command(
    name = "boop",
    version,
    about = "Cross-harness agent transcript reader: drive agents with `beep`, read what they did with `db`",
    after_help = "STORE SCHEMA: this build writes version 5. A store written by an older \
build is refused, and `boop db sync create --rebuild` drops every stored row and \
re-projects every transcript from byte 0 (about 18 s over 1.5 GB here). Nothing is \
wiped without that flag.\n\nThe pre-split verbs (harnesses, sessions, events, chat, \
tail, list, measure, dispatch, lane, resolve, adopt, sweep, prune, hail, sync, follow) \
still run as hidden aliases for one release. Use `beep` and `db`."
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
    /// Read and count what agents did.
    Db {
        #[command(subcommand)]
        cmd: DbCmd,
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
                mail_dir,
                resolve_wait,
                main_tree,
                base_sha,
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
            mail_dir,
        } => run_adopt(
            &name,
            &tmux,
            harness.as_deref(),
            session_id.as_deref(),
            cwd.as_deref(),
            model.as_deref(),
            mode.as_deref(),
            mail_dir.as_deref(),
        ),
        SubCmd::Prune { mail_dir } => run_prune(mail_dir.as_deref()),
        SubCmd::Beep { cmd } => run_beep(&registry, cmd),
        SubCmd::Db { cmd } => run_db(&registry, cmd),
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
            line(&format!("{}\t{}\t{}\t{}\t{}\t{}",
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
                line(&format!("{} {} {} {} {}",
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

fn resolve_harness<'a>(
    registry: &'a Registry,
    id: &str,
) -> Result<&'a dyn boop::harness::Harness> {
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
                line(&format!("{} {} {} {} {} {} {}",
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
                line(&format!("{} open (closed history: --all)",
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
            line(&format!("{agent_id}: {} in, {} out, {} unacked",
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
        let pane_pid = pane_pid(route.tmux.as_deref()).unwrap_or(0);
        match snapshot.process(pane_pid) {
            Some(info) => {
                let uptime = info.start_time_secs;
                line(&format!("{}\t{}\t{}\t{:.1}\t{}\t{}",
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

/// The pid of the shell in the first pane of `session`.
fn pane_pid(session: Option<&str>) -> Option<u32> {
    let session = session?;
    let output = Command::new("tmux")
        .args(["list-panes", "-t", session, "-F", "#{pane_pid}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .and_then(|line| line.trim().parse().ok())
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
}

fn run_dispatch(registry: &Registry, args: DispatchArgs) -> Result<()> {
    let adapter = resolve_dispatch_harness(registry, args.harness.as_deref())?;
    // The caller is the PARENT of the lane being born, never its identity.
    let caller = identity::resolve(&bus::read_routes(&mail_dir(args.mail_dir.as_deref())?)?)?;
    let harness_id = adapter.id().to_owned();
    let branch = args.tmux.clone().unwrap_or_else(|| args.to.clone());
    let base_sha = match &args.base_sha {
        Some(sha) => sha.clone(),
        None => git_head(&args.cwd)?.unwrap_or_else(|| "HEAD".into()),
    };
    let body = args.body.clone().unwrap_or_else(|| args.cmd.clone());

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
        worktree_dir: None,
        repo: std::path::PathBuf::from(&args.cwd),
        env_stamp: Some(identity::child_stamp(
            &args.to,
            &args.to,
            &harness_id,
            caller.session.as_deref(),
        )),
    };
    let session = adapter.spawn(&spec)?;

    let stamp = format!(
        "[bus {}] dispatched: {}",
        message.id,
        args.cmd.split('\n').next().unwrap_or("")
    );
    adapter.send(&session, &stamp)?;

    let dir = mail_dir(args.mail_dir.as_deref())?;
    let route = Route {
        harness: Some(harness_id),
        tmux: session.tmux.clone(),
        cwd: Some(args.cwd.clone()),
        model: args.model.clone(),
        mode: args.mode.clone(),
        session_id: args.session_id.clone(),
        source_path: None,
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

struct LaneArgs {
    name: String,
    cwd: String,
    harness: Option<String>,
    brief: Option<PathBuf>,
    model: Option<String>,
    tmux: Option<String>,
    parent: Option<String>,
    mail_dir: Option<PathBuf>,
    dry_run: bool,
}

fn run_lane(registry: &Registry, args: LaneArgs) -> Result<()> {
    let harness = args.harness.unwrap_or_else(|| "opencode".into());
    let brief = args
        .brief
        .unwrap_or_else(|| PathBuf::from(&args.cwd).join("brief.md"));
    let command = lane_command(&harness, &brief, args.model.as_deref());
    if args.dry_run {
        println!("cmd: {command}");
        println!("to: {}", args.name);
        println!("cwd: {}", args.cwd);
        println!("harness: {harness}");
        println!("tmux: {}", args.tmux.as_deref().unwrap_or(&args.name));
        return Ok(());
    }
    run_dispatch(
        registry,
        DispatchArgs {
            to: args.name,
            cwd: args.cwd,
            cmd: build_lane_command(&command, args.parent.as_deref()),
            from: None,
            harness: Some(harness),
            session_id: None,
            model: args.model,
            mode: Some("auto".into()),
            tmux: args.tmux,
            socket: None,
            body: Some(format!(
                "Read and execute the lane brief at {}",
                brief.display()
            )),
            r#ref: Some(brief.display().to_string()),
            mail_dir: args.mail_dir,
            resolve_wait: 3,
            main_tree: false,
            base_sha: None,
        },
    )
}

/// When a parent lane is given, append a completion hail so the original cmd
/// still runs and the lane re-raises its own exit code.
fn build_lane_command(command: &str, parent: Option<&str>) -> String {
    match parent {
        Some(parent) => {
            format!("{command}; __rc=$?; boop hail --to {parent} --kind result --body \"lane done rc=$__rc\"; exit $__rc")
        }
        None => command.to_owned(),
    }
}

fn lane_command(harness: &str, brief: &std::path::Path, model: Option<&str>) -> String {
    let brief = shell_quote_double(&brief.display().to_string());
    match harness {
        "opencode" => {
            let model = model.map(shell_quote_double).unwrap_or_else(|| {
                shell_quote_double("openrouter/deepseek/deepseek-v4-flash-0731")
            });
            format!("opencode run -m {model} --auto \"$(cat {brief})\"")
        }
        _ => format!("claude --auto \"$(cat {brief})\""),
    }
}

fn shell_quote_double(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
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
    };
    write_route(&dir, name, route)?;
    println!("adopted {name} -> tmux {tmux_session}");
    Ok(())
}

fn run_prune(mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let Some(live) = tmux::live_sessions(None) else {
        println!("refusing prune: tmux unreachable, cannot tell live from dead");
        return Ok(());
    };
    let routes = bus::read_routes(&dir)?;
    let dead: Vec<String> = routes
        .iter()
        .filter(|(_, route)| !live.has(route.tmux.as_deref().unwrap_or("")))
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
    use super::resolve_dispatch_harness;
    use boop::registry::Registry;
    

    /// A named harness that is not registered must be refused, never quietly
    /// swapped for the first adapter, which would be a capability lie.
    #[test]
    fn dispatch_refuses_an_unregistered_harness() {
        let registry = Registry::discover();
        let error = match resolve_dispatch_harness(&registry, Some("kimi")) {
            Ok(_) => panic!("unregistered harness must be refused"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("kimi"), "message: {message}");
        assert!(message.contains("claude"), "registered set: {message}");
        assert!(message.contains("opencode"), "registered set: {message}");
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
    /// pid, rss, cpu, uptime, child count per lane.
    Ps {
        lane: Option<String>,
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
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Stop a lane and forget it, or bulk-delete by state.
    Delete {
        lane: Option<String>,
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
    /// Tokens and cost. A leaf with --group-by, and a parent of blocks and
    /// burn-rate; clap needs both attributes to accept the two forms.
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Usage {
        #[command(flatten)]
        args: UsageArgs,
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
    /// Bucket the report; omit for one totals row.
    #[arg(long, value_enum)]
    group_by: Option<usage::GroupBy>,
    /// Fold the session's whole spawn subtree into the numbers.
    #[arg(long)]
    rollup_subtree: bool,
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    since: Option<u64>,
    #[arg(long)]
    until: Option<u64>,
    #[arg(long)]
    limit: Option<u64>,
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
        BeepCmd::Ps { lane, mail_dir } => run_ps(mail_dir.as_deref(), lane.as_deref()),
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
            mail_dir,
        } => run_adopt(
            &lane,
            &tmux,
            harness.as_deref(),
            session_id.as_deref(),
            cwd.as_deref(),
            model.as_deref(),
            mode.as_deref(),
            mail_dir.as_deref(),
        ),
        LaneCmd::Delete {
            lane,
            state,
            mail_dir,
        } => match (lane, state) {
            (Some(lane), _) => run_lane_delete(mail_dir.as_deref(), &lane),
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
        line(&format!("{} {} {} {} {} {} {}",
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

/// Stop one lane and drop its route. Refuses when tmux is unreachable, for the
/// same reason the bulk delete does: it cannot tell live from dead.
fn run_lane_delete(mail_dir_arg: Option<&Path>, lane: &str) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        anyhow::bail!("no registry route for lane `{lane}`")
    };
    if let Some(session) = route.tmux.as_deref() {
        match tmux::has_session(None, session) {
            Ok(true) => tmux::kill_session(None, session)?,
            Ok(false) => {}
            Err(error) => anyhow::bail!("tmux unreachable, refusing to delete {lane}: {error}"),
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
fn run_ps(mail_dir_arg: Option<&Path>, lane: Option<&str>) -> Result<()> {
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
        let pane_pid = pane_pid(route.tmux.as_deref()).unwrap_or(0);
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
            None => println!("{}\t{}\t-\t-\t-\t-", name, pane_pid),
        }
    }
    Ok(())
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
        DbCmd::Usage { args, cmd } => match cmd {
            None => run_usage(&args),
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

fn usage_filter(args: &UsageArgs) -> usage::UsageQuery {
    usage::UsageQuery {
        session: args.session.clone(),
        since: args.since,
        until: args.until,
        rollup_subtree: args.rollup_subtree,
        limit: args.limit,
    }
}

/// `db usage`: one totals row, or one row per bucket. The report names the
/// models it could not price rather than folding them in as zero.
fn run_usage(args: &UsageArgs) -> Result<()> {
    let store = open_store()?;
    let filter = usage_filter(args);
    let rows = store.usage_report(args.group_by, &filter)?;
    emit_json_rows(&rows, args.format);
    for row in store.unpriced_models(&filter)? {
        line(&serde_json::json!({
            "unpriced_model": row["model"],
            "calls": row["calls"],
        })
        .to_string());
    }
    Ok(())
}

fn run_usage_blocks(args: &UsageArgs, window_hours: u64, active_only: bool) -> Result<()> {
    let store = open_store()?;
    let window_ms = (window_hours * 3_600_000) as i64;
    let blocks = store.usage_blocks(window_ms, &usage_filter(args))?;
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
    let store = open_store()?;
    let mut filter = usage_filter(args);
    if filter.since.is_none() {
        filter.since = Some(now_ms().saturating_sub(window_minutes * 60_000));
    }
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
