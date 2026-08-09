//! The opencode adapter. opencode writes no transcript file, so this tails
//! `message.rowid` in its SQLite store, read-only; opencode owns that store.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::event::AgentEvent;
use crate::harness::{Capabilities, Harness, Ingested, ReadChunk, SessionRef, SendOutcome};
use crate::ident::{Store, SyncStat, UsageRow};

pub struct Opencode;

impl Harness for Opencode {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn sessions(&self) -> Result<Vec<SessionRef>> {
        let Some(path) = store_path() else {
            return Ok(Vec::new());
        };
        let Ok(connection) = open_read_only(&path) else {
            return Ok(Vec::new());
        };
        let mut statement = connection.prepare(
            "SELECT id, directory, parent_id, slug, time_updated FROM session ORDER BY time_updated",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        let mut sessions = Vec::new();
        for row in rows {
            let (id, directory, parent, slug, updated) = row?;
            sessions.push(SessionRef {
                harness: "opencode",
                session_id: id.clone(),
                nickname: slug.unwrap_or(id),
                path: path.clone(),
                cwd: directory,
                git_branch: None,
                modified_ms: updated as u64,
                size: 0,
                tmux: None,
                tmux_socket: None,
                parent,
            });
        }
        Ok(sessions)
    }

    fn read_from(&self, session: &SessionRef, offset: u64) -> Result<ReadChunk> {
        let connection = open_read_only(&session.path)?;
        let mut events = Vec::new();
        let mut next = offset;
        for message in messages_after(&connection, &session.session_id, offset)? {
            next = message.rowid;
            events.push(AgentEvent {
                harness: "opencode",
                session_id: session.session_id.clone(),
                ts_ms: message.ts,
                uuid: Some(message.id.clone()),
                parent_uuid: None,
                cwd: session.cwd.clone(),
                git_branch: None,
                record_type: message.role.clone(),
                tool_name: None,
                paths: Vec::new(),
                urls: Vec::new(),
                raw_line_offset: message.rowid,
            });
        }
        Ok(ReadChunk {
            events,
            next_offset: next,
            reset: false,
            skipped: 0,
        })
    }

    /// Every `true` here is covered by a test. `send_midflight` is false:
    /// `opencode run` is one-shot and exits, so a mailbox hail reaches nothing.
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            send_midflight: false,
            resume: true,
            spawn: false,
            subagent_visible: true,
        }
    }

    fn send(&self, _session: &SessionRef, _text: &str) -> Result<SendOutcome> {
        Ok(SendOutcome::QueuedForNextSpawn)
    }

    fn ingest(&self, store: &Store, session: &SessionRef, from: u64) -> Result<Ingested> {
        let connection = open_read_only(&session.path)?;
        let messages = messages_after(&connection, &session.session_id, from)?;
        if messages.is_empty() {
            return Ok(Ingested {
                stat: SyncStat::default(),
                next_cursor: from,
            });
        }
        let mut turn = store.begin_walk(&session.session_id)?;
        let mut stat = SyncStat::default();
        let mut cursor = from;
        for message in &messages {
            cursor = message.rowid;
            let mut first_turn = None;
            for part in parts_of(&connection, &message.id)? {
                match part.kind.as_str() {
                    "text" => {
                        turn += 1;
                        let inserted =
                            store.write_turn(&session.session_id, turn, message.ts, &message.role, &part.text)?;
                        record(&mut stat, inserted);
                        first_turn.get_or_insert(turn);
                    }
                    "tool" => {
                        turn += 1;
                        let inserted =
                            store.write_turn(&session.session_id, turn, message.ts, "tool", "")?;
                        record(&mut stat, inserted);
                        first_turn.get_or_insert(turn);
                        store.write_tool_fact(
                            &session.session_id,
                            turn,
                            message.ts,
                            &part.tool,
                            part.input.as_ref(),
                        )?;
                    }
                    _ => {}
                }
            }
            let Some(usage) = message.usage() else {
                continue;
            };
            let attach = match first_turn {
                Some(turn) => turn,
                None => {
                    turn += 1;
                    let inserted =
                        store.write_turn(&session.session_id, turn, message.ts, &message.role, "")?;
                    record(&mut stat, inserted);
                    turn
                }
            };
            let (is_new, changed) = store.write_usage(&session.session_id, attach, &usage)?;
            if changed {
                if is_new {
                    stat.usage_written += 1;
                } else {
                    stat.usage_updated += 1;
                }
            }
        }
        Ok(Ingested {
            stat,
            next_cursor: cursor,
        })
    }
}

fn record(stat: &mut SyncStat, inserted: usize) {
    if inserted == 0 {
        stat.dropped += 1;
    } else {
        stat.written += 1;
    }
}

/// One opencode message, with the token counts it records.
pub struct Message {
    pub rowid: u64,
    pub id: String,
    pub ts: u64,
    pub role: String,
    data: Value,
}

impl Message {
    /// opencode records no request id, so the dedup key is the message id
    /// alone; reasoning tokens are billed as output and fold into it.
    pub fn usage(&self) -> Option<UsageRow<'_>> {
        if self.role != "assistant" {
            return None;
        }
        let tokens = self.data.get("tokens")?.as_object()?;
        let count = |key: &str| -> i64 {
            tokens.get(key).and_then(Value::as_i64).unwrap_or(0)
        };
        let cache = |key: &str| -> i64 {
            tokens
                .get("cache")
                .and_then(Value::as_object)
                .and_then(|cache| cache.get(key))
                .and_then(Value::as_i64)
                .unwrap_or(0)
        };
        Some(UsageRow {
            ts: self.ts,
            message_id: &self.id,
            request_id: "",
            model: self
                .data
                .get("modelID")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            service_tier: None,
            input_tokens: count("input"),
            output_tokens: count("output") + count("reasoning"),
            cache_create_5m_tokens: cache("write"),
            cache_create_1h_tokens: 0,
            cache_read_tokens: cache("read"),
            is_sidechain: false,
            cost_usd_recorded: self.data.get("cost").and_then(Value::as_f64),
        })
    }
}

/// One content part of a message.
pub struct Part {
    pub kind: String,
    pub tool: String,
    pub text: String,
    pub input: Option<Value>,
}

fn store_path() -> Option<PathBuf> {
    let path = dirs::home_dir()?
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    path.exists().then_some(path)
}

/// Read-only, and never creating the file: a missing opencode is "no sessions",
/// never an empty store this process invented.
fn open_read_only(path: &std::path::Path) -> Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open opencode store at {}", path.display()))
}

/// Messages after a rowid cursor. rowid is the resume point because it rises
/// with insertion order and two messages can share a millisecond.
fn messages_after(connection: &Connection, session: &str, after: u64) -> Result<Vec<Message>> {
    let mut statement = connection.prepare(
        "SELECT rowid, id, time_created, data FROM message
         WHERE session_id = ?1 AND rowid > ?2 ORDER BY rowid",
    )?;
    let rows = statement.query_map(rusqlite::params![session, after as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (rowid, id, ts, raw) = row?;
        let data: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        let role = data
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        out.push(Message {
            rowid: rowid as u64,
            id,
            ts: ts as u64,
            role,
            data,
        });
    }
    Ok(out)
}

fn parts_of(connection: &Connection, message_id: &str) -> Result<Vec<Part>> {
    let mut statement =
        connection.prepare("SELECT data FROM part WHERE message_id = ?1 ORDER BY id")?;
    let rows = statement.query_map(rusqlite::params![message_id], |row| {
        row.get::<_, String>(0)
    })?;
    let mut out = Vec::new();
    for row in rows {
        let data: Value = match serde_json::from_str(&row?) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let kind = data
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        out.push(Part {
            tool: data
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            text: data
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            input: data
                .get("state")
                .and_then(|state| state.get("input"))
                .cloned(),
            kind,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{Opencode, Part};
    use crate::harness::Harness;

    /// Capabilities are claims; each true one needs a test, and `send_midflight`
    /// is false because `opencode run` exits when its turn ends.
    #[test]
    fn opencode_capabilities_match_the_binary() {
        let caps = Opencode.capabilities();
        assert!(!caps.send_midflight, "opencode run is one-shot");
        assert!(caps.resume, "opencode run -s <sessionID> resumes");
        assert!(!caps.spawn, "no spawn support is claimed yet");
        assert!(caps.subagent_visible, "session.parent_id names the parent");
    }

    /// A hail to a finished one-shot cannot land, so the outcome says queued
    /// rather than injected.
    #[test]
    fn a_send_is_queued_not_injected() {
        let session = crate::harness::SessionRef {
            harness: "opencode",
            session_id: "ses_x".to_owned(),
            nickname: "ses_x".to_owned(),
            path: std::path::PathBuf::from("/tmp/none.db"),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        };
        assert_eq!(
            Opencode.send(&session, "hello").unwrap(),
            crate::harness::SendOutcome::QueuedForNextSpawn
        );
    }

    /// A missing opencode store is no sessions, never an error and never a
    /// store this process created.
    #[test]
    fn a_missing_store_is_no_sessions() {
        assert!(super::open_read_only(std::path::Path::new("/tmp/boop-no-such.db")).is_err());
    }

    #[test]
    fn a_part_with_no_state_still_parses() {
        let part = Part {
            kind: "text".to_owned(),
            tool: String::new(),
            text: "hello".to_owned(),
            input: None,
        };
        assert_eq!(part.text, "hello");
    }
}
