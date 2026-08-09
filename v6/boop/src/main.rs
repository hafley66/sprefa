//! `boop`: the cross-harness agent-event reader, 1-1 with `bus` plus the four
//! verbs `bus` cannot do (read what an agent did, and measure what its
//! processes cost). The CLI routes to layers 0-3; it contains no `match` on
//! harness id and no direct `Command::new("tmux")` beyond the layer-1 helpers.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::bus::Route;
use crate::event::AgentEvent;
use crate::registry::Registry;

mod bus;
mod chat;
mod event;
mod harness;
mod ident;
mod proc;
use crate::proc::ProcReader;
mod registry;
mod tail;
mod tmux;
mod worktree;

#[derive(Parser)]
#[command(
    name = "boop",
    version,
    about = "Cross-harness agent transcript reader: tail agent events from every harness on this machine as one stream"
)]
struct Cli {
    #[command(subcommand)]
    command: SubCmd,
}

#[derive(Subcommand)]
enum SubCmd {
    /// List registered harness adapters, one per line. (pass 1)
    Harnesses,
    /// List on-disk sessions, newest last. (pass 1)
    Sessions {
        /// Only sessions from this harness (its stable id).
        #[arg(long)]
        harness: Option<String>,
    },
    /// Tail one session's events from a byte offset. (pass 1)
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
    Events {
        #[command(flatten)]
        query: QueryArgs,
    },
    /// List lanes and messages like `bus list`.
    List {
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Measure per-lane pid, rss, cpu, uptime, child count.
    Measure {
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Spawn a lane: tmux new-session + mailbox + registry route.
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
    },
    /// Resolve a lane's harness session id into its registry route.
    Resolve {
        #[arg(long)]
        to: String,
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Queue a message and inject it into a live pane.
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
    Prune {
        #[arg(long)]
        mail_dir: Option<PathBuf>,
    },
    /// Project sessions into NDJSON chat-repr turns (the zipf door).
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
    Sync {},
    /// Stream new facts into the db on a coarse poll (idle near-zero CPU).
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let registry = Registry::discover();
    match cli.command {
        SubCmd::Harnesses => run_harnesses(&registry),
        SubCmd::Sessions { harness } => run_sessions(&registry, harness.as_deref()),
        SubCmd::Tail { session_id, from, format } => {
            run_tail(&registry, &session_id, from.unwrap_or(0), format)
        }
        SubCmd::Events { query } => run_query(&query),
        SubCmd::Sync {} => run_sync_all(&registry),
        SubCmd::Follow { } => run_follow(&registry),
        SubCmd::Chat { query, all, follow, json } => {
            run_chat_query(&query, all, follow, json)
        }
        SubCmd::List { agent, all, mail_dir } => run_list(mail_dir.as_deref(), agent.as_deref(), all),
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
        } => run_dispatch(DispatchArgs {
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
        }),
        SubCmd::Resolve { to, mail_dir } => run_resolve(&to, mail_dir.as_deref()),
        SubCmd::Hail {
            to,
            body,
            from,
            kind,
            box_,
            socket,
            mail_dir,
        } => run_hail(&to, &body, from.as_deref(), kind.as_deref(), box_.as_deref(), socket.as_deref(), mail_dir.as_deref()),
        SubCmd::Sweep {
            agent,
            box_,
            close_routeless,
            max_age_days,
            mail_dir,
        } => run_sweep(mail_dir.as_deref(), box_.as_deref(), agent.as_deref(), close_routeless, max_age_days),
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
        } => run_lane(LaneArgs {
            name,
            cwd,
            harness,
            brief,
            model,
            tmux,
            parent,
            mail_dir,
            dry_run,
        }),
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
    }
}

// ---------------------------------------------------------------------------
// Pass 1 verbs: layer 2 (transcript)
// ---------------------------------------------------------------------------

fn run_harnesses(registry: &Registry) -> Result<()> {
    for harness in registry.all() {
        println!("{}", harness.id());
    }
    Ok(())
}

fn run_sessions(registry: &Registry, harness_id: Option<&str>) -> Result<()> {
    let harnesses: Vec<&dyn crate::harness::Harness> = match harness_id {
        Some(id) => vec![resolve_harness(registry, id)?],
        None => registry.all().iter().map(|boxed| boxed.as_ref()).collect(),
    };
    for adapter in harnesses {
        for session in adapter.sessions()? {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                session.session_id,
                session.harness,
                session.cwd.as_deref().unwrap_or("-"),
                session.git_branch.as_deref().unwrap_or("-"),
                session.modified_ms,
                session.size,
            );
        }
    }
    Ok(())
}

fn run_tail(registry: &Registry, session_id: &str, offset: u64, format: OutputFormat) -> Result<()> {
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
        println!("{}", serde_json::to_string(&edge)?);
    }
    Ok(())
}

fn emit_rows(rows: &[ident::Row], format: QueryFormat) {
    for row in rows {
        match format {
            QueryFormat::Ndjson => {
                if let Ok(line) = serde_json::to_string(row) {
                    println!("{line}");
                }
            }
            QueryFormat::Text => {
                println!(
                    "{} {} {} {} {}",
                    row["session"].as_str().unwrap_or(""),
                    row["turn"].as_i64().unwrap_or(0),
                    row["role"].as_str().unwrap_or(""),
                    row["ts"].as_i64().unwrap_or(0),
                    row["said"].as_str().unwrap_or(""),
                );
            }
        }
    }
}

/// `boop sync`: tail every harness forward from stored offsets into the db.
fn run_sync_all(registry: &Registry) -> Result<()> {
    let started = std::time::Instant::now();
    let store = ident::Store::open(ident::Store::default_path()?)?;
    store.begin()?;
    let result = (|| {
        let mut events = 0u64;
        for adapter in registry.all() {
            for session in adapter.sessions()? {
                events += ident::sync_session(&store, &session)?;
            }
        }
        Ok::<u64, anyhow::Error>(events)
    })();
    match result {
        Ok(events) => {
            store.commit()?;
            let elapsed_ms = started.elapsed().as_millis();
            let counts = store.counts()?;
            let db_bytes = store.db_bytes()?;
            let rate = (events as u128)
                .saturating_mul(1000)
                .checked_div(elapsed_ms.max(1))
                .unwrap_or(0) as u64;
            println!(
                "events={events} elapsed_ms={elapsed_ms} rate={}/s db_bytes={db_bytes} counts={}",
                rate,
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

/// `boop follow`: the same projection on a coarse poll. Sessions and their
/// mtimes are discovered once, and a file is only re-read when its mtime
/// changed, so steady-state idle is a stat per file plus a sleep.
fn run_follow(registry: &Registry) -> Result<()> {
    let mut sessions = Vec::new();
    for adapter in registry.all() {
        sessions.extend(adapter.sessions()?);
    }
    let mut last_mtime: std::collections::HashMap<String, u64> = sessions
        .iter()
        .map(|session| {
            let mtime = file_mtime_ms(&session.path).unwrap_or(0);
            (session.session_id.clone(), mtime)
        })
        .collect();
    loop {
        let store = ident::Store::open(ident::Store::default_path()?)?;
        store.begin()?;
        for session in &sessions {
            let mtime = file_mtime_ms(&session.path).unwrap_or(0);
            if last_mtime.get(&session.session_id).copied().unwrap_or(0) == mtime {
                continue;
            }
            let _ = ident::sync_session(&store, session)?;
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
        Ok(time) => Ok(time.duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)),
        Err(_) => Ok(0),
    }
}


fn resolve_harness<'a>(registry: &'a Registry, id: &str) -> Result<&'a dyn crate::harness::Harness> {
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
                println!(
                    "{} {} {} {} {} {} {}",
                    pad(state, 4),
                    padded_name,
                    padded_harness,
                    padded_mode,
                    padded_model,
                    padded_tmux,
                    route.cwd.as_deref().unwrap_or("-"),
                );
            }
            let messages = all_messages(&dir)?;
            let rows = if all {
                bus::fold(&messages)
            } else {
                bus::unacked(&messages)
            };
            for message in rows {
                println!("{}", bus::message_line(&message));
            }
            if !all {
                println!("{} open (closed history: --all)", bus::unacked(&all_messages(&dir)?).len());
            }
        }
        Some(agent_id) => {
            let messages = all_messages(&dir)?;
            let rows = bus::fold(&messages);
            let inbox: Vec<_> = rows.iter().filter(|m| m.to == agent_id).cloned().collect();
            let outbox: Vec<_> = rows.iter().filter(|m| m.from == agent_id).cloned().collect();
            for message in &inbox {
                println!("in  {}", bus::message_line(message));
            }
            for message in &outbox {
                println!("out {}", bus::message_line(message));
            }
            let mut combined = inbox.clone();
            combined.extend(outbox.iter().cloned());
            println!(
                "{agent_id}: {} in, {} out, {} unacked",
                inbox.len(),
                outbox.len(),
                bus::unacked(&combined).len()
            );
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
    println!("lane\tpid\trss_kb\tcpu_pct\tuptime_sec\tchildren");
    for (name, route) in &routes {
        let pane_pid = pane_pid(route.tmux.as_deref()).unwrap_or(0);
        match snapshot.process(pane_pid) {
            Some(info) => {
                let uptime = info.start_time_secs;
                println!(
                    "{}\t{}\t{}\t{:.1}\t{}\t{}",
                    name,
                    pane_pid,
                    info.rss_bytes / 1024,
                    info.cpu_percent,
                    uptime,
                    snapshot.descendent_count(pane_pid),
                );
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
}

fn run_dispatch(args: DispatchArgs) -> Result<()> {
    let harness = args.harness.unwrap_or_else(|| "opencode".into());
    let tmux_name = args.tmux.unwrap_or_else(|| args.to.clone());
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

    let stamped = format!("INSTANT_HARNESS={harness} {}", args.cmd);
    tmux::new_detached_session(args.socket.as_deref(), &tmux_name, &args.cwd, &stamped)
        .map_err(|error| {
            let _ = error;
            anyhow::anyhow!("tmux new-session failed; is tmux installed and reachable?")
        })?;

    let stamp = format!("[bus {}] dispatched: {}", message.id, args.cmd);
    tmux::send_keys_literal(args.socket.as_deref(), &tmux_name, &stamp)?;

    let dir = mail_dir(args.mail_dir.as_deref())?;
    let route = Route {
        harness: Some(harness),
        tmux: Some(tmux_name.clone()),
        cwd: Some(args.cwd.clone()),
        model: args.model.clone(),
        mode: args.mode.clone(),
        session_id: args.session_id.clone(),
        source_path: None,
    };
    write_route(&dir, &args.to, route)?;
    append_message(&dir, &message)?;
    println!("dispatched {} -> {} (tmux {})", message.id, args.to, tmux_name);
    std::thread::sleep(std::time::Duration::from_secs(args.resolve_wait));
    let _ = message;
    Ok(())
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
        println!("resolved {to} -> {} (self-reported)", route.session_id.as_deref().unwrap());
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

fn run_hail(
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
    tmux::send_keys_literal(socket, pane, &line)?;
    println!("injected into tmux {pane}");
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
                println!("expired {} -> {}: no registry route", message.id, message.to);
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
            println!("{} -> {}: no transcript hit, still unacked", message.id, message.to);
        }
    }
    println!("swept {} unacked, acked {acked}, expired {expired}", pending.len());
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
    let hits = value.get("hits").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
    Ok(hits.iter().any(|hit| {
        let source = hit.get("source_path").and_then(serde_json::Value::as_str).unwrap_or("");
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

fn run_lane(args: LaneArgs) -> Result<()> {
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
    run_dispatch(DispatchArgs {
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
        body: Some(format!("Read and execute the lane brief at {}", brief.display())),
        r#ref: Some(brief.display().to_string()),
        mail_dir: args.mail_dir,
        resolve_wait: 3,
    })
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
            let model = model
                .map(shell_quote_double)
                .unwrap_or_else(|| shell_quote_double("openrouter/deepseek/deepseek-v4-flash-0731"));
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

fn append_ack(dir: &std::path::Path, _box_name: Option<&str>, message: &bus::Message) -> Result<()> {
    let mut ack = message.clone();
    ack.to_timestamp = Some(bus::now_iso());
    append_message(dir, &ack)
}
