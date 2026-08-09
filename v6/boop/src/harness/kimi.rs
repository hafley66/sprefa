//! The kimi adapter: transcripts under
//! `~/.kimi-code/sessions/wd_<slug>/session_<uuid>/agents/{main,agent-N}/wire.jsonl`.
#![allow(dead_code)]

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::Value;

use crate::event::AgentEvent;
use crate::harness::{Harness, Ingested, ReadChunk, SessionRef};
use crate::ident::{Store, SyncStat, UsageRow};
use crate::tail;

pub struct Kimi;

impl Harness for Kimi {
    fn id(&self) -> &'static str {
        "kimi"
    }

    fn sessions(&self) -> anyhow::Result<Vec<SessionRef>> {
        sessions_in(&kimi_sessions_dir()?)
    }

    fn read_from(&self, session: &SessionRef, offset: u64) -> anyhow::Result<ReadChunk> {
        let mut file = File::open(&session.path)
            .with_context(|| format!("open transcript {}", session.path.display()))?;
        let result = tail::read_complete_lines(&mut file, offset)?;

        let mut events = Vec::new();
        let mut skipped = 0usize;
        for line in &result.lines {
            match parse_line(session, line) {
                Some(event) => events.push(event),
                None => skipped += 1,
            }
        }

        Ok(ReadChunk {
            events,
            next_offset: result.next_offset,
            reset: result.reset,
            skipped,
        })
    }

    fn ingest(&self, store: &Store, session: &SessionRef, from: u64) -> anyhow::Result<Ingested> {
        let mut file = File::open(&session.path)
            .with_context(|| format!("open transcript {}", session.path.display()))?;
        let result = tail::read_complete_lines(&mut file, from)?;
        if result.lines.is_empty() {
            return Ok(Ingested {
                stat: SyncStat::default(),
                next_cursor: from,
            });
        }
        let mut turn = store.begin_walk(&session.session_id)?;
        let mut stat = SyncStat::default();
        let mut pending_message_id: Option<String> = None;
        for line in &result.lines {
            project_line(
                store,
                session,
                line,
                &mut turn,
                &mut stat,
                &mut pending_message_id,
            )?;
        }
        Ok(Ingested {
            stat,
            next_cursor: result.next_offset,
        })
    }
}

fn kimi_sessions_dir() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join(".kimi-code").join("sessions"))
}

struct KimiState {
    cwd: Option<String>,
}

fn read_state(path: &Path) -> Option<KimiState> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let cwd = value
        .get("workDir")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some(KimiState { cwd })
}

/// `main` keeps the session uuid as its id; a sub-agent's id embeds its own
/// agent slot so two agents in one session never collide.
fn sessions_in(base: &Path) -> anyhow::Result<Vec<SessionRef>> {
    let mut sessions = Vec::new();
    let Ok(project_dirs) = std::fs::read_dir(base) else {
        return Ok(sessions);
    };
    for project_entry in project_dirs.filter_map(|entry| entry.ok()) {
        if !project_entry
            .file_type()
            .map(|kind| kind.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(session_dirs) = std::fs::read_dir(project_entry.path()) else {
            continue;
        };
        for session_entry in session_dirs.filter_map(|entry| entry.ok()) {
            if !session_entry
                .file_type()
                .map(|kind| kind.is_dir())
                .unwrap_or(false)
            {
                continue;
            }
            let session_path = session_entry.path();
            let Some(session_uuid) = session_path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix("session_"))
            else {
                continue;
            };
            let state =
                read_state(&session_path.join("state.json")).unwrap_or(KimiState { cwd: None });
            let Ok(agent_dirs) = std::fs::read_dir(session_path.join("agents")) else {
                continue;
            };
            for agent_entry in agent_dirs.filter_map(|entry| entry.ok()) {
                if !agent_entry
                    .file_type()
                    .map(|kind| kind.is_dir())
                    .unwrap_or(false)
                {
                    continue;
                }
                let agent_id = agent_entry.file_name().to_string_lossy().into_owned();
                let wire_path = agent_entry.path().join("wire.jsonl");
                if !wire_path.is_file() {
                    continue;
                }
                let (session_id, parent) = if agent_id == "main" {
                    (session_uuid.to_owned(), None)
                } else {
                    (
                        format!("{session_uuid}/{agent_id}"),
                        Some(session_uuid.to_owned()),
                    )
                };
                let metadata = wire_path.metadata().ok();
                let modified_ms = metadata
                    .as_ref()
                    .and_then(|meta| meta.modified().ok())
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_millis() as u64)
                    .unwrap_or(0);
                let size = metadata.map(|meta| meta.len()).unwrap_or(0);
                sessions.push(SessionRef {
                    harness: "kimi",
                    session_id,
                    nickname: agent_id,
                    path: wire_path,
                    cwd: state.cwd.clone(),
                    git_branch: None,
                    modified_ms,
                    size,
                    tmux: None,
                    tmux_socket: None,
                    parent,
                });
            }
        }
    }
    sessions.sort_by_key(|session| session.modified_ms);
    Ok(sessions)
}

/// Real events sit under `event` in a `context.append_loop_event` wrapper;
/// `context.append_message` and `usage.record` are unwrapped top-level lines.
fn parse_line(session: &SessionRef, line: &tail::CompleteLine) -> Option<AgentEvent> {
    let value: Value = serde_json::from_slice(&line.bytes).ok()?;
    let ts_ms = value.get("time").and_then(Value::as_u64).unwrap_or(0);
    let top_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    let event = value.get("event");
    let record_type = event
        .and_then(|event| event.get("type"))
        .and_then(Value::as_str)
        .unwrap_or(top_type)
        .to_owned();

    let mut tool_name = None;
    let mut paths = Vec::new();
    let mut urls = Vec::new();
    if let Some(event) = event {
        if record_type == "tool.call" {
            tool_name = event.get("name").and_then(Value::as_str).map(str::to_owned);
            let args = event.get("args");
            if let Some(path) = args
                .and_then(|args| args.get("path"))
                .and_then(Value::as_str)
            {
                let access = if tool_name.as_deref() == Some("Read") {
                    crate::event::Access::Read
                } else {
                    crate::event::Access::Write
                };
                paths.push(crate::event::ToolPath {
                    path: path.to_owned(),
                    access,
                });
            }
            if let Some(url) = args
                .and_then(|args| args.get("url"))
                .and_then(Value::as_str)
            {
                urls.push(url.to_owned());
            }
        }
    }

    Some(AgentEvent {
        harness: session.harness,
        session_id: session.session_id.clone(),
        ts_ms,
        uuid: None,
        parent_uuid: None,
        cwd: session.cwd.clone(),
        git_branch: session.git_branch.clone(),
        record_type,
        tool_name,
        paths,
        urls,
        raw_line_offset: line.start,
    })
}

fn record(stat: &mut SyncStat, inserted: usize) {
    if inserted == 0 {
        stat.dropped += 1;
    } else {
        stat.written += 1;
    }
}

/// `FetchURL` is renamed so it lands in the store's existing `webfetch`
/// dispatch; every other kimi tool name already matches lowercased.
fn normalize_tool_name(name: &str) -> &str {
    if name == "FetchURL" {
        "webfetch"
    } else {
        name
    }
}

fn append_message_text(content: Option<&Value>) -> String {
    let Some(blocks) = content.and_then(Value::as_array) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                parts.push(text.to_owned());
            }
        }
    }
    parts.join("\n")
}

#[allow(clippy::too_many_arguments)]
fn project_line(
    store: &Store,
    session: &SessionRef,
    line: &tail::CompleteLine,
    turn: &mut u64,
    stat: &mut SyncStat,
    pending_message_id: &mut Option<String>,
) -> anyhow::Result<()> {
    let value: Value = match serde_json::from_slice(&line.bytes) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let ts = value.get("time").and_then(Value::as_u64).unwrap_or(0);
    let top_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    let sid = session.session_id.clone();

    if top_type == "context.append_message" {
        let Some(message) = value.get("message") else {
            return Ok(());
        };
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let text = append_message_text(message.get("content"));
        if !text.is_empty() {
            *turn += 1;
            let inserted = store.write_turn(&sid, *turn, ts, role, &text)?;
            record(stat, inserted);
        }
        return Ok(());
    }

    if top_type == "usage.record" {
        let Some(usage) = value.get("usage").and_then(Value::as_object) else {
            return Ok(());
        };
        let count = |key: &str| -> i64 { usage.get(key).and_then(Value::as_i64).unwrap_or(0) };
        let model = value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let message_id = pending_message_id
            .take()
            .unwrap_or_else(|| line.start.to_string());
        let usage_row = UsageRow {
            ts,
            message_id: &message_id,
            request_id: "",
            model: &model,
            service_tier: None,
            input_tokens: count("inputOther"),
            output_tokens: count("output"),
            cache_create_5m_tokens: count("inputCacheCreation"),
            cache_create_1h_tokens: 0,
            cache_read_tokens: count("inputCacheRead"),
            is_sidechain: session.parent.is_some(),
            cost_usd_recorded: None,
        };
        let attach_turn = if *turn == 0 {
            *turn += 1;
            let inserted = store.write_turn(&sid, *turn, ts, "assistant", "")?;
            record(stat, inserted);
            *turn
        } else {
            *turn
        };
        let (is_new, changed) = store.write_usage(&sid, attach_turn, &usage_row)?;
        if changed {
            if is_new {
                stat.usage_written += 1;
            } else {
                stat.usage_updated += 1;
            }
        }
        return Ok(());
    }

    if top_type != "context.append_loop_event" {
        return Ok(());
    }
    let Some(event) = value.get("event") else {
        return Ok(());
    };
    let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
    match event_type {
        "content.part" => {
            let part = event.get("part");
            if part
                .and_then(|part| part.get("type"))
                .and_then(Value::as_str)
                == Some("text")
            {
                let text = part
                    .and_then(|part| part.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !text.is_empty() {
                    *turn += 1;
                    let inserted = store.write_turn(&sid, *turn, ts, "assistant", text)?;
                    record(stat, inserted);
                }
            }
        }
        "tool.call" => {
            let name = event.get("name").and_then(Value::as_str).unwrap_or("tool");
            *turn += 1;
            let inserted = store.write_turn(&sid, *turn, ts, "tool", "")?;
            record(stat, inserted);
            store.write_tool_fact(
                &sid,
                *turn,
                ts,
                normalize_tool_name(name),
                event.get("args"),
            )?;
        }
        "step.end" => {
            *pending_message_id = event
                .get("messageId")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::PathBuf;

    use crate::harness::{Harness, SessionRef};

    use super::{sessions_in, Kimi};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("boop_kimi_{}_{}", std::process::id(), name))
    }

    fn write_lines(path: &PathBuf, lines: &[&str]) {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    fn session_for(path: &std::path::Path, size: u64) -> SessionRef {
        SessionRef {
            harness: "kimi",
            session_id: "ses-kimi-1".to_owned(),
            nickname: "main".to_owned(),
            path: path.to_path_buf(),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size,
            tmux: None,
            tmux_socket: None,
            parent: None,
        }
    }

    #[test]
    fn kimi_capabilities_default_to_all_false() {
        let caps = Kimi.capabilities();
        assert!(!caps.send_midflight);
        assert!(!caps.resume);
        assert!(!caps.spawn);
        assert!(!caps.subagent_visible);
    }

    #[test]
    fn reads_a_user_message_and_a_tool_call() {
        let path = temp_path("jn1");
        write_lines(
            &path,
            &[
                r#"{"type":"metadata","protocol_version":"1.4","created_at":1}"#,
                r#"{"type":"context.append_message","message":{"role":"user","content":[{"type":"text","text":"read CLAUDE.md"}]},"time":2}"#,
                r#"{"type":"context.append_loop_event","event":{"type":"tool.call","uuid":"t1","name":"Read","args":{"path":"CLAUDE.md"}},"time":3}"#,
            ],
        );
        let metadata = path.metadata().unwrap();
        let kimi = Kimi;
        let chunk = kimi
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(
            chunk.events.len(),
            3,
            "every line decodes to an event, even metadata"
        );
        assert_eq!(chunk.skipped, 0);
        assert_eq!(chunk.events[2].tool_name.as_deref(), Some("Read"));
        assert_eq!(chunk.events[2].paths[0].path, "CLAUDE.md");
    }

    #[test]
    fn skips_an_invalid_json_line_but_keeps_the_rest() {
        let path = temp_path("jn2");
        write_lines(
            &path,
            &[
                r#"{"type":"context.append_message","message":{"role":"user","content":[{"type":"text","text":"hi"}]},"time":1}"#,
                r#"not json"#,
            ],
        );
        let metadata = path.metadata().unwrap();
        let kimi = Kimi;
        let chunk = kimi
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 1);
        assert_eq!(chunk.skipped, 1);
    }

    #[test]
    fn a_partial_trailing_line_is_not_consumed() {
        let path = temp_path("jn3");
        write_lines(
            &path,
            &[
                r#"{"type":"context.append_message","message":{"role":"user","content":[{"type":"text","text":"hi"}]},"time":1}"#,
            ],
        );
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        write!(file, "{{\"partial").unwrap();
        drop(file);
        let metadata = path.metadata().unwrap();
        let kimi = Kimi;
        let chunk = kimi
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 1);
        assert!(chunk.next_offset < metadata.len());
    }

    #[test]
    fn discovers_main_and_a_sub_agent_from_the_fixture() {
        let base = std::path::PathBuf::from("tests/fixtures/kimi");
        let sessions = sessions_in(&base).unwrap();
        let main = sessions
            .iter()
            .find(|session| session.nickname == "main")
            .expect("main agent transcript present in fixture");
        assert!(main.parent.is_none());
        let sub = sessions
            .iter()
            .find(|session| session.nickname == "agent-0")
            .expect("sub-agent transcript present in fixture");
        assert_eq!(sub.parent.as_deref(), Some(main.session_id.as_str()));
        assert_ne!(sub.session_id, main.session_id);
        assert!(main.cwd.is_some(), "state.json workDir recovered as cwd");
    }
}
