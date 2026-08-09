//! Layer 2 projection: the chat-repr door. One record type, one NDJSON line
//! per TURN (assistant text turn, user turn, tool call collapsed to its name
//! and primary arg). This is a projection of records the claude adapter
//! already parses, reading through the same byte-offset tailer (partial-line
//! law included). It is the door the zipf/word-frequency analysis walks:
//! `text` carries the human-visible words only, never tool payloads, base64,
//! or file contents inside tool results.
#![allow(dead_code)]

use serde::Serialize;

use crate::harness::SessionRef;
use crate::tail;

/// One projected turn.
#[derive(Clone, Debug, Serialize)]
pub struct ChatTurn {
    pub session: String,
    pub harness: &'static str,
    pub seq: u64,
    #[serde(rename = "ts")]
    pub ts_ms: u64,
    pub role: String,
    pub text: String,
    pub tool: Option<ToolRef>,
    pub branch: Option<String>,
}

/// A tool call collapsed to its name and a short primary argument.
#[derive(Clone, Debug, Serialize)]
pub struct ToolRef {
    pub name: String,
    pub arg: String,
}

/// Read a whole snapshot of turns from `session`, starting at byte 0. Returns
/// the turns and the offset to resume from.
pub fn snapshot(session: &SessionRef) -> anyhow::Result<(Vec<ChatTurn>, u64)> {
    let mut seq = 1u64;
    read_turns(session, 0, &mut seq)
}

/// Project the complete lines read forward from `offset`. Assigns `seq` from
/// the provided counter so a follow loop stays contiguous.
pub fn read_turns(
    session: &SessionRef,
    offset: u64,
    seq: &mut u64,
) -> anyhow::Result<(Vec<ChatTurn>, u64)> {
    let mut file = std::fs::File::open(&session.path)
        .map_err(|error| anyhow::anyhow!("open {}: {error}", session.path.display()))?;
    let result = tail::read_complete_lines(&mut file, offset)?;
    let mut turns = Vec::new();
    for line in &result.lines {
        for mut turn in project_line(session, line)? {
            turn.seq = *seq;
            *seq += 1;
            turns.push(turn);
        }
    }
    Ok((turns, result.next_offset))
}

/// Project one line into a flat list of turns (a user or assistant record can
/// yield several: text blocks plus tool calls).
fn project_line(session: &SessionRef, line: &tail::CompleteLine) -> anyhow::Result<Vec<ChatTurn>> {
    let value: serde_json::Value = match serde_json::from_slice(&line.bytes) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };
    let object = match value.as_object() {
        Some(object) => object,
        None => return Ok(Vec::new()),
    };
    let record_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let ts_ms = object
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .and_then(crate::harness::claude::parse_iso_ms)
        .unwrap_or(0);
    let branch = object
        .get("gitBranch")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let mut turns = Vec::new();
    match record_type {
        "user" => {
            for block in content_blocks(&value) {
                match block {
                    ContentBlock::Text(text) => turns.push(ChatTurn {
                        session: session.session_id.clone(),
                        harness: session.harness,
                        seq: 0,
                        ts_ms,
                        role: "user".into(),
                        text,
                        tool: None,
                        branch: branch.clone(),
                    }),
                    // tool results carry file payloads; they are never text.
                    ContentBlock::ToolResult => {}
                    ContentBlock::ToolUse { .. } => {}
                }
            }
        }
        "assistant" => {
            for block in content_blocks(&value) {
                match block {
                    ContentBlock::Text(text) => turns.push(ChatTurn {
                        session: session.session_id.clone(),
                        harness: session.harness,
                        seq: 0,
                        ts_ms,
                        role: "assistant".into(),
                        text,
                        tool: None,
                        branch: branch.clone(),
                    }),
                    ContentBlock::ToolResult => {}
                    ContentBlock::ToolUse { name, input } => turns.push(ChatTurn {
                        session: session.session_id.clone(),
                        harness: session.harness,
                        seq: 0,
                        ts_ms,
                        role: "tool".into(),
                        text: String::new(),
                        tool: Some(ToolRef {
                            name,
                            arg: primary_arg(&input),
                        }),
                        branch: branch.clone(),
                    }),
                }
            }
        }
        _ => {}
    }
    Ok(turns)
}

enum ContentBlock {
    Text(String),
    ToolUse {
        name: String,
        input: serde_json::Value,
    },
    ToolResult,
}

/// Walk `message.content`, accepting a plain string or an array of blocks.
fn content_blocks(record: &serde_json::Value) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();
    let message = match record.get("message").and_then(serde_json::Value::as_object) {
        Some(message) => message,
        None => return blocks,
    };
    let content = message.get("content");
    if let Some(text) = content.and_then(serde_json::Value::as_str) {
        blocks.push(ContentBlock::Text(text.to_owned()));
        return blocks;
    }
    let Some(array) = content.and_then(serde_json::Value::as_array) else {
        return blocks;
    };
    for block in array {
        let Some(object) = block.as_object() else {
            continue;
        };
        let Some(kind) = object.get("type").and_then(serde_json::Value::as_str) else {
            continue;
        };
        match kind {
            "text" => {
                let text = object
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                blocks.push(ContentBlock::Text(text.to_owned()));
            }
            "tool_use" => {
                let name = object
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let input = object
                    .get("input")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                blocks.push(ContentBlock::ToolUse { name, input });
            }
            "tool_result" => {
                blocks.push(ContentBlock::ToolResult);
            }
            _ => {}
        }
    }
    blocks
}

/// A short identifying argument for a tool call, chosen so `text` never holds
/// file contents: prefer the path or url, then bounded fields.
fn primary_arg(input: &serde_json::Value) -> String {
    let object = match input.as_object() {
        Some(object) => object,
        None => return String::new(),
    };
    for key in ["file_path", "url", "skill", "description"] {
        if let Some(value) = object.get(key).and_then(serde_json::Value::as_str) {
            return bound(value);
        }
    }
    if let Some(command) = object.get("command").and_then(serde_json::Value::as_str) {
        return bound(command);
    }
    if let Some(single) = object.get("pattern").and_then(serde_json::Value::as_str) {
        return bound(single);
    }
    if let Some(first) = object.values().next().and_then(serde_json::Value::as_str) {
        return bound(first);
    }
    String::new()
}

fn bound(value: &str) -> String {
    value
        .chars()
        .take(200)
        .collect::<String>()
        .replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::harness::SessionRef;

    use super::snapshot;

    fn fixture_session() -> SessionRef {
        SessionRef {
            harness: "claude",
            session_id: "fixture-session".to_owned(),
            path: PathBuf::from("tests/fixtures/chat_fixture.jsonl"),
            cwd: Some("/w".to_owned()),
            git_branch: Some("main".to_owned()),
            modified_ms: 0,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        }
    }

    #[test]
    fn projects_turns_and_never_leaks_tool_result_content() {
        let (turns, _) = snapshot(&fixture_session()).unwrap();
        // user text, assistant text, tool call = 3; the system record and the
        // tool_result block add none.
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].role, "user");
        assert_eq!(turns[0].text, "hello world this is human text");
        assert_eq!(turns[1].role, "assistant");
        assert_eq!(turns[2].role, "tool");
        assert_eq!(
            turns[2].tool.as_ref().map(|tool| tool.name.as_str()),
            Some("Read")
        );
        assert_eq!(
            turns[2].tool.as_ref().map(|tool| tool.arg.as_str()),
            Some("/tmp/secret-notes.md")
        );
        // The tool result's file body must not appear in any text field.
        for turn in &turns {
            assert!(
                !turn.text.contains("TOPSECRETFILEMARKER"),
                "tool result content leaked into text: {:?}",
                turn.text
            );
        }
    }
}
