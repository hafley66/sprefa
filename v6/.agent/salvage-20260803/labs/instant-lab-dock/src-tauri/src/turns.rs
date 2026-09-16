// Read one agent session's turns (role + preview + ts) for the in-tab strip's
// pushed "agent-session" view (CONTRACT4). Two legs: claude transcript jsonl and
// opencode sqlite. Reuses the ledger readers so the strip parses the stores the
// same way the session sidebar's turn model does. Read-only; unknown harnesses
// return an empty vec, never an error. All fns take `home` as a parameter so
// tests can point them at a temp/nonexistent HOME.
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::ledger::AiMessage;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TurnRow {
    pub role: String,
    pub preview: String,
    pub ts: String, // ISO UTC ("" when unknown)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn turn_row(message: AiMessage) -> TurnRow {
    TurnRow {
        role: message.role,
        preview: message.preview,
        ts: crate::harness::ms_to_iso(message.ts),
    }
}

// claude stores one <sessionId>.jsonl per project dir (cwd-encoded); the strip
// only knows the session id, so find the file by id across every project dir.
fn find_claude_file(home: &Path, session_id: &str) -> Option<PathBuf> {
    let mut found: Option<PathBuf> = None;
    collect(&home.join(".claude").join("projects"), session_id, &mut found);
    found
}

fn collect(dir: &Path, session_id: &str, found: &mut Option<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, session_id, found);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
            && path.file_stem().and_then(|s| s.to_str()) == Some(session_id)
        {
            *found = Some(path);
            return;
        }
    }
}

fn claude_turns(home: &Path, session_id: &str) -> Vec<TurnRow> {
    match find_claude_file(home, session_id) {
        Some(path) => crate::ledger::read_claude(&path, session_id, None)
            .into_iter()
            .map(turn_row)
            .collect(),
        None => Vec::new(),
    }
}

fn opencode_turns(home: &Path, session_id: &str) -> Vec<TurnRow> {
    let db = home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    crate::ledger::read_opencode_at(&db, session_id, None)
        .into_iter()
        .map(turn_row)
        .collect()
}

fn turns_for(home: &Path, harness: &str, session_id: &str) -> Vec<TurnRow> {
    match harness {
        "claude" => claude_turns(home, session_id),
        "opencode" => opencode_turns(home, session_id),
        _ => Vec::new(),
    }
}

#[tauri::command]
pub fn agent_session_turns(harness: String, session_id: String) -> Vec<TurnRow> {
    match home() {
        Some(home) => turns_for(&home, &harness, &session_id),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("dock-strip-turns-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&home);
        home
    }

    // Proof #1: a fixture claude jsonl (user/assistant/meta lines plus ignored
    // system/mode openers) reads as ordered TurnRows with role + preview + ts.
    #[test]
    fn claude_fixture_yields_ordered_turn_rows() {
        let home = temp_home("claude");
        let file = home.join(".claude").join("projects").join("demo");
        fs::create_dir_all(&file).unwrap();
        fs::write(
            file.join("session-1.jsonl"),
            concat!(
                "{\"type\":\"mode\",\"mode\":\"normal\"}\n",
                "{\"type\":\"user\",\"uuid\":\"u1\",\"timestamp\":\"2026-07-20T10:00:00.000Z\",\"isMeta\":false,\"message\":{\"role\":\"user\",\"content\":\"hello there\"}}\n",
                "{\"type\":\"assistant\",\"uuid\":\"a1\",\"timestamp\":\"2026-07-20T10:00:01.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"I can help.\"}]}}\n",
                "{\"type\":\"user\",\"uuid\":\"u2\",\"timestamp\":\"2026-07-20T10:00:02.000Z\",\"promptSource\":\"system\",\"origin\":{\"kind\":\"task-notification\"},\"message\":{\"role\":\"user\",\"content\":\"<task-notification>\\n<task-id>1</task-id>\\n</task-notification>\"}}\n",
            ),
        )
        .unwrap();

        let rows = claude_turns(&home, "session-1");
        let cells: Vec<(String, String)> =
            rows.iter().map(|r| (r.role.clone(), r.preview.clone())).collect();
        assert_eq!(
            cells,
            vec![
                ("user".to_string(), "hello there".to_string()),
                ("assistant".to_string(), "I can help.".to_string()),
                (
                    "meta".to_string(),
                    "<task-notification> <task-id>1</task-id> </task-notification>".to_string(),
                ),
            ]
        );
        assert_eq!(rows[0].ts, "2026-07-20T10:00:00.000Z");
        assert_eq!(rows[1].ts, "2026-07-20T10:00:01.000Z");

        // Unknown session id (fresh project) -> empty vec, never an error.
        assert!(claude_turns(&home, "missing-session").is_empty());
        fs::remove_dir_all(&home).ok();
    }

    // Proof #2: a fixture opencode sqlite (message + part rows) reads as ordered
    // TurnRows newest-last (oldest first from the db), role from the message,
    // preview from its text part, ts from time_created.
    #[test]
    fn opencode_fixture_yields_ordered_turn_rows() {
        let home = temp_home("opencode");
        let db_dir = home.join(".local").join("share").join("opencode");
        fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("opencode.db");

        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, \
                 time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL); \
                 CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT NOT NULL, \
                 session_id TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, \
                 data TEXT NOT NULL);",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO message (id, session_id, time_created, time_updated, data) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["m1", "s1", 1_000, 1_000, r#"{"role":"user","time":{"created":1000}}"#],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO message (id, session_id, time_created, time_updated, data) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["m2", "s1", 2_000, 2_000, r#"{"role":"assistant","time":{"created":2000}}"#],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params!["p1", "m1", "s1", 1_000, 1_000, r#"{"type":"text","text":"first user turn"}"#],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params!["p2", "m2", "s1", 2_000, 2_000, r#"{"type":"text","text":"assistant reply"}"#],
            )
            .unwrap();
        }

        let rows = opencode_turns(&home, "s1");
        let cells: Vec<(String, String)> =
            rows.iter().map(|r| (r.role.clone(), r.preview.clone())).collect();
        assert_eq!(
            cells,
            vec![
                ("user".to_string(), "first user turn".to_string()),
                ("assistant".to_string(), "assistant reply".to_string()),
            ]
        );
        assert_eq!(rows[0].ts, "1970-01-01T00:00:01.000Z");
        assert_eq!(rows[1].ts, "1970-01-01T00:00:02.000Z");

        // Unknown harness -> empty vec, never an error.
        assert!(turns_for(&home, "codex", "s1").is_empty());
        fs::remove_dir_all(&home).ok();
    }
}
