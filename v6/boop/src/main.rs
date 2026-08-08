//! `boop`: the cross-harness agent transcript reader.

use std::io::Write;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::event::AgentEvent;
use crate::harness::SessionRef;
use crate::registry::Registry;

mod event;
mod harness;
mod registry;
mod tail;

#[derive(Parser)]
#[command(
    name = "boop",
    version,
    about = "Cross-harness agent transcript reader: tail agent events from every harness on this machine as one stream"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List registered harnesses, one per line.
    Harnesses,
    /// List on-disk sessions, newest last.
    Sessions {
        /// Only sessions from this harness (its stable id).
        #[arg(long)]
        harness: Option<String>,
    },
    /// Tail one session's events from a byte offset.
    Tail {
        /// The session id to read.
        session_id: String,
        /// Byte offset to start from. Defaults to 0.
        #[arg(long)]
        from: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Stream events across sessions, optionally filtered by time.
    Events {
        /// Only sessions from this harness (its stable id).
        #[arg(long)]
        harness: Option<String>,
        /// Only events at or after this timestamp (ms since the epoch).
        #[arg(long)]
        since_ms: Option<u64>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
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
        Command::Harnesses => {
            for harness in registry.all() {
                println!("{}", harness.id());
            }
        }
        Command::Sessions { harness } => {
            for session in collect_sessions(&registry, harness.as_deref())? {
                print_session(&session);
            }
        }
        Command::Tail { session_id, from, format } => {
            let offset = from.unwrap_or(0);
            let chunk = tail_session(&registry, &session_id, offset)?;
            emit_append_notes(&chunk.reset, chunk.skipped);
            for event in &chunk.events {
                emit_event(event, format);
            }
            if matches!(format, OutputFormat::Text) {
                eprintln!("resume offset: {}", chunk.next_offset);
            }
        }
        Command::Events { harness, since_ms, format } => {
            let harnesses: Vec<&dyn crate::harness::Harness> = match &harness {
                Some(id) => vec![resolve_harness(&registry, id)?],
                None => registry.all().iter().map(|boxed| boxed.as_ref()).collect(),
            };
            for adapter in harnesses {
                for session in adapter.sessions()? {
                    let chunk = adapter.read_from(&session, 0)?;
                    emit_append_notes(&chunk.reset, chunk.skipped);
                    for event in chunk.events.into_iter().filter(|event| {
                        since_ms.map(|since| event.ts_ms >= since).unwrap_or(true)
                    }) {
                        emit_event(&event, format);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Sessions from every harness, or from one harness when filtered, newest last
/// within each harness.
fn collect_sessions(registry: &Registry, harness_id: Option<&str>) -> Result<Vec<SessionRef>> {
    let harnesses: Vec<&dyn crate::harness::Harness> = match harness_id {
        Some(id) => vec![resolve_harness(registry, id)?],
        None => registry.all().iter().map(|boxed| boxed.as_ref()).collect(),
    };
    let mut sessions = Vec::new();
    for harness in harnesses {
        sessions.extend(harness.sessions()?);
    }
    Ok(sessions)
}

fn resolve_harness<'a>(registry: &'a Registry, id: &str) -> Result<&'a dyn crate::harness::Harness> {
    registry
        .by_id(id)
        .with_context(|| format!("no harness registered with id `{id}`"))
}

/// Find a session by id across every harness and tail it from `offset`.
fn tail_session(
    registry: &Registry,
    session_id: &str,
    offset: u64,
) -> Result<crate::harness::ReadChunk> {
    for harness in registry.all() {
        for session in harness.sessions()? {
            if session.session_id == session_id {
                return harness.read_from(&session, offset);
            }
        }
    }
    anyhow::bail!("no session found with id `{session_id}`")
}

fn emit_append_notes(reset: &bool, skipped: usize) {
    if *reset {
        eprintln!("note: transcript shorter than stored offset; restarted from byte 0");
    }
    if skipped > 0 {
        eprintln!("note: skipped {skipped} line(s) that failed to parse as JSON");
    }
}

fn print_session(session: &SessionRef) {
    println!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        session.session_id,
        session.harness,
        session.cwd.as_deref().unwrap_or("-"),
        session.git_branch.as_deref().unwrap_or("-"),
        session.modified_ms,
        session.size,
    )
}

fn emit_event(event: &AgentEvent, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let encoded = serde_json::to_string(event).unwrap_or_default();
            println!("{encoded}");
        }
        OutputFormat::Text => {
            let paths = event
                .paths
                .iter()
                .map(|path| {
                    let access = match path.access {
                        crate::event::Access::Read => "r",
                        crate::event::Access::Write => "w",
                        crate::event::Access::Create => "c",
                        crate::event::Access::Delete => "d",
                        crate::event::Access::Rename => "m",
                    };
                    format!("{}({access})", path.path)
                })
                .collect::<Vec<_>>()
                .join(",");
            let tool = event.tool_name.as_deref().unwrap_or("-");
            if paths.is_empty() && event.urls.is_empty() {
                println!("[{}] {} {} {} {}", event.harness, event.ts_ms, event.record_type, tool, event.session_id);
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
    let _ = std::io::stdout().flush();
}
