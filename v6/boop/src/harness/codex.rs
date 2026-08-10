//! The codex adapter: transcripts under `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`.
//! Every line wraps a `payload` object whose own `type` names the real record.
#![allow(dead_code)]

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::Value;
use walkdir::WalkDir;

use crate::event::AgentEvent;
use crate::harness::{Harness, Ingested, ReadChunk, SessionRef};
use crate::ident::{Store, SyncStat, UsageRow};
use crate::tail;

pub struct Codex;

impl Harness for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn sessions(&self) -> anyhow::Result<Vec<SessionRef>> {
        sessions_in(&codex_sessions_dir()?)
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

    /// `token_count` snapshots carry no per-call id; a turn's snapshots sum
    /// into one row keyed `{session}#t{turn}` (dict_request is global).
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
        let mut current_model = String::from("unknown");
        let mut turn_tokens = TurnTokens::default();
        for line in &result.lines {
            project_line(
                store,
                session,
                line,
                &mut turn,
                &mut stat,
                &mut current_model,
                &mut turn_tokens,
            )?;
        }
        Ok(Ingested {
            stat,
            next_cursor: result.next_offset,
        })
    }
}

fn codex_sessions_dir() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join(".codex").join("sessions"))
}

/// A file whose first line is not `session_meta` is skipped, never guessed
/// at from the filename.
struct SessionMeta {
    session_id: String,
    parent: Option<String>,
    cwd: Option<String>,
    nickname: String,
}

fn first_session_meta(path: &Path) -> Option<SessionMeta> {
    let mut file = File::open(path).ok()?;
    let result = tail::read_complete_lines(&mut file, 0).ok()?;
    let first = result.lines.into_iter().next()?;
    let value: Value = serde_json::from_slice(&first.bytes).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = value.get("payload")?.as_object()?;
    let session_id = payload.get("id").and_then(Value::as_str)?.to_owned();
    let parent = payload
        .get("forked_from_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let nickname = payload
        .get("agent_nickname")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| session_id.clone());
    Some(SessionMeta {
        session_id,
        parent,
        cwd,
        nickname,
    })
}

fn sessions_in(base: &Path) -> anyhow::Result<Vec<SessionRef>> {
    let mut sessions = Vec::new();
    for entry in WalkDir::new(base)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|ext| ext != "jsonl")
        {
            continue;
        }
        let path = entry.path();
        let Some(meta) = first_session_meta(path) else {
            continue;
        };
        let metadata = path.metadata().ok();
        let modified_ms = metadata
            .as_ref()
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let size = metadata.map(|meta| meta.len()).unwrap_or(0);

        sessions.push(SessionRef {
            harness: "codex",
            session_id: meta.session_id,
            nickname: meta.nickname,
            path: path.to_path_buf(),
            cwd: meta.cwd,
            git_branch: None,
            modified_ms,
            size,
            tmux: None,
            tmux_socket: None,
            parent: meta.parent,
        });
    }
    sessions.sort_by_key(|session| session.modified_ms);
    Ok(sessions)
}

/// `record_type` is `payload.type`, since the outer wrapper only ever says
/// `session_meta`/`response_item`/`event_msg`.
fn parse_line(session: &SessionRef, line: &tail::CompleteLine) -> Option<AgentEvent> {
    let value: Value = serde_json::from_slice(&line.bytes).ok()?;
    let ts_ms = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(crate::harness::claude::parse_iso_ms)
        .unwrap_or(0);
    let outer_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let payload = value.get("payload").and_then(Value::as_object);
    let record_type = payload
        .and_then(|payload| payload.get("type"))
        .and_then(Value::as_str)
        .unwrap_or(outer_type)
        .to_owned();
    let uuid = payload
        .and_then(|payload| payload.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let record_cwd = payload
        .and_then(|payload| payload.get("cwd"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    let mut tool_name = None;
    let mut paths = Vec::new();
    if let Some(payload) = payload {
        match record_type.as_str() {
            "function_call" | "custom_tool_call" => {
                tool_name = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            "patch_apply_end" => {
                if let Some(changes) = payload.get("changes").and_then(Value::as_object) {
                    for (path, change) in changes {
                        paths.push(crate::event::ToolPath {
                            path: path.clone(),
                            access: access_for_change(change),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Some(AgentEvent {
        harness: session.harness,
        session_id: session.session_id.clone(),
        ts_ms,
        uuid,
        parent_uuid: None,
        cwd: record_cwd.or_else(|| session.cwd.clone()),
        git_branch: session.git_branch.clone(),
        record_type,
        tool_name,
        paths,
        urls: Vec::new(),
        raw_line_offset: line.start,
    })
}

fn access_for_change(change: &Value) -> crate::event::Access {
    match change.get("type").and_then(Value::as_str) {
        Some("add") => crate::event::Access::Create,
        Some("delete") => crate::event::Access::Delete,
        _ => crate::event::Access::Write,
    }
}

fn record(stat: &mut SyncStat, inserted: usize) {
    if inserted == 0 {
        stat.dropped += 1;
    } else {
        stat.written += 1;
    }
}

/// `reasoning`/other block kinds are skipped: a thinking-only block burns no
/// ordinal, mirroring claude.
fn message_text(payload: &serde_json::Map<String, Value>) -> String {
    let Some(blocks) = payload.get("content").and_then(Value::as_array) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for block in blocks {
        let Some(block) = block.as_object() else {
            continue;
        };
        let kind = block.get("type").and_then(Value::as_str).unwrap_or("");
        if kind == "input_text" || kind == "output_text" || kind == "text" {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                parts.push(text.to_owned());
            }
        }
    }
    parts.join("\n")
}

#[allow(clippy::too_many_arguments)]
/// Running totals for one turn's `token_count` snapshots.
#[derive(Default)]
struct TurnTokens {
    turn: u64,
    input: i64,
    output: i64,
    cache_write: i64,
    cached: i64,
}

fn project_line(
    store: &Store,
    session: &SessionRef,
    line: &tail::CompleteLine,
    turn: &mut u64,
    stat: &mut SyncStat,
    current_model: &mut String,
    turn_tokens: &mut TurnTokens,
) -> anyhow::Result<()> {
    let value: Value = match serde_json::from_slice(&line.bytes) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let ts = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(crate::harness::claude::parse_iso_ms)
        .unwrap_or(0);
    let Some(payload) = value.get("payload").and_then(Value::as_object) else {
        return Ok(());
    };
    let record_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
    let sid = session.session_id.clone();

    match record_type {
        "message" => {
            let role = payload
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user");
            let text = message_text(payload);
            if !text.is_empty() {
                *turn += 1;
                let inserted = store.write_turn(&sid, *turn, ts, role, &text)?;
                record(stat, inserted);
            }
        }
        "function_call" | "custom_tool_call" => {
            // Argument shapes vary by tool/version; no agent_cmd/agent_touch
            // fact is derived here.
            let name = payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            *turn += 1;
            let inserted = store.write_turn(&sid, *turn, ts, "tool", "")?;
            record(stat, inserted);
            store.write_tool_fact(&sid, *turn, ts, name, None)?;
        }
        "patch_apply_end" => {
            let Some(changes) = payload.get("changes").and_then(Value::as_object) else {
                return Ok(());
            };
            if changes.is_empty() {
                return Ok(());
            }
            *turn += 1;
            let inserted = store.write_turn(&sid, *turn, ts, "tool", "")?;
            record(stat, inserted);
            for (path, change) in changes {
                // The store's verb vocabulary (read/write/edit/...) has no
                // delete; a deleted file is not recorded as a touch.
                let verb = match change.get("type").and_then(Value::as_str) {
                    Some("add") => "Write",
                    Some("update") => "Edit",
                    _ => continue,
                };
                store.write_tool_fact(
                    &sid,
                    *turn,
                    ts,
                    verb,
                    Some(&serde_json::json!({ "file_path": path })),
                )?;
            }
        }
        "thread_settings_applied" => {
            if let Some(model) = payload
                .get("thread_settings")
                .and_then(|settings| settings.get("model"))
                .and_then(Value::as_str)
            {
                *current_model = model.to_owned();
            }
        }
        "token_count" => {
            let Some(last) = payload
                .get("info")
                .and_then(|info| info.get("last_token_usage"))
                .and_then(Value::as_object)
            else {
                return Ok(());
            };
            let count = |key: &str| -> i64 { last.get(key).and_then(Value::as_i64).unwrap_or(0) };
            // `input_tokens` includes the cached subset (OTEL convention); the
            // store wants it excluded, so it is subtracted out here.
            let cached = count("cached_input_tokens");
            let cache_write = count("cache_write_input_tokens");
            let input_tokens = (count("input_tokens") - cached - cache_write).max(0);
            let attach_turn = if *turn == 0 {
                *turn += 1;
                let inserted = store.write_turn(&sid, *turn, ts, "assistant", "")?;
                record(stat, inserted);
                *turn
            } else {
                *turn
            };
            if turn_tokens.turn != attach_turn {
                *turn_tokens = TurnTokens {
                    turn: attach_turn,
                    ..TurnTokens::default()
                };
            }
            turn_tokens.input += input_tokens;
            turn_tokens.output += count("output_tokens");
            turn_tokens.cache_write += cache_write;
            turn_tokens.cached += cached;
            let message_id = format!("{sid}#t{attach_turn}");
            let usage = UsageRow {
                ts,
                message_id: &message_id,
                request_id: "",
                model: current_model.as_str(),
                service_tier: None,
                input_tokens: turn_tokens.input,
                output_tokens: turn_tokens.output,
                cache_create_5m_tokens: turn_tokens.cache_write,
                cache_create_1h_tokens: 0,
                cache_read_tokens: turn_tokens.cached,
                is_sidechain: session.parent.is_some(),
                cost_usd_recorded: None,
            };
            let (is_new, changed) = store.write_usage(&sid, attach_turn, &usage)?;
            if changed {
                if is_new {
                    stat.usage_written += 1;
                } else {
                    stat.usage_updated += 1;
                }
            }
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

    use crate::harness::{claude, Harness, SessionRef};
    use crate::Store;

    use super::{sessions_in, Codex};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("boop_codex_{}_{}", std::process::id(), name))
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
            harness: "codex",
            session_id: "ses-codex-1".to_owned(),
            nickname: "ses-codex-1".to_owned(),
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
    fn codex_capabilities_default_to_all_false() {
        let caps = Codex.capabilities();
        assert!(!caps.send_midflight);
        assert!(!caps.resume);
        assert!(!caps.spawn);
        assert!(!caps.subagent_visible);
    }

    #[test]
    fn reads_a_message_and_a_token_count_line() {
        let path = temp_path("jn1");
        write_lines(
            &path,
            &[
                r#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"response_item","payload":{"type":"message","id":"msg_1","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
                r#"{"timestamp":"2026-08-09T17:20:06.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":10}}}}"#,
            ],
        );
        let metadata = path.metadata().unwrap();
        let codex = Codex;
        let chunk = codex
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 2);
        assert_eq!(chunk.skipped, 0);
        assert_eq!(chunk.events[0].record_type, "message");
        assert_eq!(chunk.events[1].record_type, "token_count");
    }

    /// Fail-first receipt: pre-fix, same-turn `token_count` snapshots each
    /// INSERTed and collided on the agent_usage (session_id, turn) key.
    #[test]
    fn same_turn_token_counts_sum_into_one_usage_row() {
        let db_path = temp_path("jn4db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let path = temp_path("jn4");
        write_lines(
            &path,
            &[
                r#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"response_item","payload":{"type":"message","id":"msg_1","role":"user","content":[{"type":"input_text","text":"hello"}]}}"#,
                r#"{"timestamp":"2026-08-09T17:20:06.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":10}}}}"#,
                r#"{"timestamp":"2026-08-09T17:20:07.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":50,"cached_input_tokens":0,"output_tokens":5}}}}"#,
                r#"{"timestamp":"2026-08-09T17:20:08.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":10,"output_tokens":1}}}}"#,
            ],
        );
        let metadata = path.metadata().unwrap();
        let codex = Codex;
        let ingested = codex
            .ingest(&store, &session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(ingested.stat.usage_written, 1);
        assert_eq!(ingested.stat.usage_updated, 2);
        drop(store);
        let connection = rusqlite::Connection::open(&db_path).unwrap();
        let (row_count, input_tokens, output_tokens, cache_read_tokens): (i64, i64, i64, i64) =
            connection
                .query_row(
                    "SELECT COUNT(*), SUM(input_tokens), SUM(output_tokens),
                       SUM(cache_read_tokens) FROM agent_usage",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .unwrap();
        assert_eq!(row_count, 1);
        assert_eq!(input_tokens, 110);
        assert_eq!(output_tokens, 16);
        assert_eq!(cache_read_tokens, 50);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn skips_an_invalid_json_line_but_keeps_the_rest() {
        let path = temp_path("jn2");
        write_lines(
            &path,
            &[
                r#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"response_item","payload":{"type":"message","id":"msg_1","role":"user","content":[{"type":"input_text","text":"hi"}]}}"#,
                r#"this is not json at all"#,
            ],
        );
        let metadata = path.metadata().unwrap();
        let codex = Codex;
        let chunk = codex
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 1);
        assert_eq!(chunk.skipped, 1);
    }

    /// A partial trailing line is neither parsed nor counted into the offset;
    /// the harness trait doc requires this for every file-backed adapter.
    #[test]
    fn a_partial_trailing_line_is_not_consumed() {
        let path = temp_path("jn3");
        write_lines(
            &path,
            &[
                r#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"response_item","payload":{"type":"message","id":"msg_1","role":"user","content":[{"type":"input_text","text":"hi"}]}}"#,
            ],
        );
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        write!(file, "{{\"partial").unwrap();
        drop(file);
        let metadata = path.metadata().unwrap();
        let codex = Codex;
        let chunk = codex
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 1);
        assert!(chunk.next_offset < metadata.len());
    }

    #[test]
    fn extracts_patch_apply_paths() {
        let path = temp_path("jn4");
        let record = r#"{"timestamp":"2026-08-09T17:20:05.152Z","type":"event_msg","payload":{"type":"patch_apply_end","changes":{"/tmp/a.rs":{"type":"add"},"/tmp/b.rs":{"type":"update"}}}}"#;
        write_lines(&path, &[record]);
        let metadata = path.metadata().unwrap();
        let codex = Codex;
        let chunk = codex
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 1);
        assert_eq!(chunk.events[0].paths.len(), 2);
    }

    #[test]
    fn discovers_a_forked_subagent_session_from_the_fixture() {
        let base = std::path::PathBuf::from("tests/fixtures/codex");
        let sessions = sessions_in(&base).unwrap();
        assert!(
            sessions.len() >= 2,
            "root and forked session both discovered"
        );
        let root = sessions
            .iter()
            .find(|session| session.parent.is_none())
            .expect("a root session with no forked_from_id");
        let child = sessions
            .iter()
            .find(|session| session.parent.as_deref() == Some(root.session_id.as_str()))
            .expect("a forked session naming the root as its parent");
        assert_ne!(root.session_id, child.session_id);
    }

    #[test]
    fn parses_the_corpus_timestamp_shape() {
        assert_eq!(
            claude::parse_iso_ms("2026-08-09T17:20:05.152Z"),
            Some(1_786_296_005_152)
        );
    }
}
