//! The opencode adapter. opencode writes no transcript file, so this tails
//! `message.rowid` in its SQLite store, read-only; opencode owns that store.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::event::AgentEvent;
use crate::harness::{Capabilities, Harness, Ingested, ReadChunk, SendOutcome, SessionRef, SpawnSpec};
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

    /// `send_midflight` stays false: `opencode run` reads no stdin mid-turn,
    /// so a pane injection lands on dead air (transport itself is tested).
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            send_midflight: false,
            resume: true,
            spawn: true,
            subagent_visible: true,
        }
    }

    fn preview_command(&self, spec: &SpawnSpec) -> Option<String> {
        launch_command(spec).ok()
    }

    fn spawn(&self, spec: &SpawnSpec) -> Result<SessionRef> {
        let session_id = format!("agent-{}", random_hex());
        let tmux_name = format!("boop-{session_id}");
        let cwd = crate::worktree::prepare_spawn_dir(spec)?;
        let command = launch_command(spec)?;
        crate::tmux::new_detached_session(
            spec.socket.as_deref(),
            &tmux_name,
            &cwd.display().to_string(),
            &command,
        )?;
        Ok(SessionRef {
            harness: "opencode",
            session_id: session_id.clone(),
            nickname: session_id,
            path: opencode_db_path().unwrap_or_else(|| cwd.join("opencode.db")),
            cwd: Some(cwd.display().to_string()),
            git_branch: Some(spec.branch.clone()),
            modified_ms: now_ms(),
            size: 0,
            tmux: Some(tmux_name),
            tmux_socket: spec.socket.clone(),
            parent: None,
        })
    }

    fn send(&self, session: &SessionRef, text: &str) -> Result<SendOutcome> {
        match &session.tmux {
            Some(tmux) => {
                crate::tmux::send_keys_literal(session.tmux_socket.as_deref(), tmux, text)?;
                Ok(SendOutcome::Injected)
            }
            None => Ok(SendOutcome::QueuedForNextSpawn),
        }
    }

    fn stop(&self, session: &SessionRef) -> Result<()> {
        if let Some(tmux) = &session.tmux {
            if crate::tmux::has_session(session.tmux_socket.as_deref(), tmux)? {
                crate::tmux::kill_session(session.tmux_socket.as_deref(), tmux)?;
            }
        }
        Ok(())
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
    opencode_db_path().filter(|path| path.exists())
}

/// The db path regardless of existence; a spawn writes its command before
/// opencode has ever created the file.
fn opencode_db_path() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join(".local")
            .join("share")
            .join("opencode")
            .join("opencode.db"),
    )
}

/// The opencode command a spawn runs. Model comes from `BOOP_LANE_MODEL`,
/// resolved by the caller (`run_lane` in main.rs); never a default here.
fn launch_command(spec: &SpawnSpec) -> Result<String> {
    let model = std::env::var("BOOP_LANE_MODEL")
        .ok()
        .filter(|value| !value.is_empty())
        .context("BOOP_LANE_MODEL unset; opencode spawn needs a resolved model")?;
    let mut command = format!("opencode run -m {}", shell_quote(&model));
    if let Some(session) = &spec.resume_session {
        command.push_str(&format!(" -s {}", shell_quote(session)));
    }
    command.push_str(&format!(" --auto \"$(cat {})\"", shell_quote_double(&spec.prompt)));
    Ok(match &spec.env_stamp {
        Some(stamp) => format!("{stamp} {command}"),
        None => command,
    })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Double-quote a value for use inside an already-double-quoted `$(cat ...)`
/// substitution; bash nests nested `"..."` correctly inside `$(...)`.
fn shell_quote_double(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn random_hex() -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(8);
    for _ in 0..8 {
        let _ = write!(out, "{:02x}", randish());
    }
    out
}

fn randish() -> u8 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_nanos() >> 8) as u8)
        .unwrap_or(0)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
    use super::{launch_command, Opencode, Part};
    use crate::harness::{Harness, SpawnSpec};

    /// Env is process-global; these tests serialize on one mutex and restore
    /// the prior value, same convention as `identity::tests::temp_env`.
    mod temp_env {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        pub fn with_var(key: &str, value: Option<&str>, body: impl FnOnce()) {
            let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
            let saved = std::env::var(key).ok();
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
            body();
            match saved {
                Some(saved) => std::env::set_var(key, saved),
                None => std::env::remove_var(key),
            }
        }
    }

    /// Capabilities are claims; each true one needs a test.
    #[test]
    fn opencode_capabilities_match_the_binary() {
        let caps = Opencode.capabilities();
        assert!(!caps.send_midflight, "opencode run reads no stdin mid-turn");
        assert!(caps.resume, "opencode run -s <sessionID> resumes");
        assert!(caps.spawn, "spawn is implemented and tested below");
        assert!(caps.subagent_visible, "session.parent_id names the parent");
    }

    /// A hail with no live pane cannot land, so the outcome says queued
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

    // ---- facet 3 ----

    struct TmuxGuard {
        socket: String,
    }

    static NEXT_SOCKET: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    impl TmuxGuard {
        fn new() -> TmuxGuard {
            let socket = format!(
                "boop-test-{}-oc{}",
                std::process::id(),
                NEXT_SOCKET.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            crate::tmux::kill_test_server(&socket);
            TmuxGuard { socket }
        }
    }

    impl Drop for TmuxGuard {
        fn drop(&mut self) {
            crate::tmux::kill_test_server(&self.socket);
        }
    }

    fn spec(guard: &TmuxGuard) -> SpawnSpec {
        SpawnSpec {
            harness: "opencode".to_owned(),
            branch: "lane-test".to_owned(),
            base_sha: "0000000000000000000000000000000000000000".to_owned(),
            main_tree: true,
            setup: Vec::new(),
            prompt: "/tmp/brief.md".to_owned(),
            resume_session: None,
            socket: Some(guard.socket.clone()),
            worktree_dir: None,
            repo: std::env::temp_dir(),
            env_stamp: None,
        }
    }

    mod temp_repo {
        use std::process::Command;

        pub struct TempRepo {
            pub dir: std::path::PathBuf,
            pub sha: String,
            pub worktree: std::path::PathBuf,
        }

        impl TempRepo {
            pub fn new() -> TempRepo {
                let dir = std::env::temp_dir().join(format!("boop-oc-repo-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&dir);
                let worktree =
                    std::env::temp_dir().join(format!("boop-oc-wt-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&worktree);
                Command::new("git").arg("init").arg("-q").arg(&dir).status().unwrap();
                let d = dir.display().to_string();
                Command::new("git")
                    .args(["-C", &d, "config", "user.email", "t@t"])
                    .status()
                    .unwrap();
                Command::new("git")
                    .args(["-C", &d, "config", "user.name", "t"])
                    .status()
                    .unwrap();
                std::fs::write(dir.join("seed.txt"), "s").unwrap();
                Command::new("git").args(["-C", &d, "add", "-A"]).status().unwrap();
                Command::new("git").args(["-C", &d, "commit", "-qm", "seed"]).status().unwrap();
                let sha = String::from_utf8_lossy(
                    &Command::new("git")
                        .args(["-C", &d, "rev-parse", "HEAD"])
                        .output()
                        .unwrap()
                        .stdout,
                )
                .trim()
                .to_owned();
                TempRepo { dir, sha, worktree }
            }
        }

        impl Drop for TempRepo {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.dir);
                let _ = std::fs::remove_dir_all(&self.worktree);
            }
        }
    }

    #[test]
    fn a_missing_model_refuses_to_build_a_command() {
        temp_env::with_var("BOOP_LANE_MODEL", None, || {
            let guard = TmuxGuard::new();
            let error = launch_command(&spec(&guard)).unwrap_err();
            assert!(error.to_string().contains("BOOP_LANE_MODEL"));
        });
    }

    #[test]
    fn launch_command_cats_the_prompt_path_under_the_resolved_model() {
        temp_env::with_var("BOOP_LANE_MODEL", Some("openrouter/deepseek/deepseek-v4-flash-0731"), || {
            let guard = TmuxGuard::new();
            let command = launch_command(&spec(&guard)).unwrap();
            assert!(command.contains("opencode run -m 'openrouter/deepseek/deepseek-v4-flash-0731'"));
            assert!(command.contains("--auto \"$(cat \"/tmp/brief.md\")\""));
        });
    }

    #[test]
    fn opencode_launch_resumes_with_session_id() {
        temp_env::with_var("BOOP_LANE_MODEL", Some("m"), || {
            let guard = TmuxGuard::new();
            let mut req = spec(&guard);
            req.resume_session = Some("ses_abc123".to_owned());
            let command = launch_command(&req).unwrap();
            assert!(command.contains("-s 'ses_abc123'"));
        });
    }

    #[test]
    fn opencode_spawn_returns_handle_and_stop_tears_down() {
        temp_env::with_var("BOOP_LANE_MODEL", Some("m"), || {
            let guard = TmuxGuard::new();
            let repo = temp_repo::TempRepo::new();
            let worktree = repo.worktree.clone();
            let mut req = spec(&guard);
            req.main_tree = false;
            req.base_sha = repo.sha.clone();
            req.repo = repo.dir.clone();
            req.worktree_dir = Some(worktree.clone());
            let opencode = Opencode;
            let session = opencode.spawn(&req).unwrap();
            assert_eq!(
                session.tmux.as_deref().map(|t| t.starts_with("boop-agent-")),
                Some(true)
            );
            assert_eq!(session.tmux_socket.as_deref(), Some(guard.socket.as_str()));
            assert!(
                worktree.join("seed.txt").exists(),
                "worktree must be created by spawn"
            );
            opencode.stop(&session).unwrap();
            assert!(!has_session_on(&guard, session.tmux.as_deref().unwrap()));
        });
    }

    #[test]
    fn opencode_send_injects_into_a_live_pane() {
        let guard = TmuxGuard::new();
        let name = format!("ctl-{}", std::process::id());
        let status = std::process::Command::new("tmux")
            .args(["-L", &guard.socket, "new-session", "-d", "-s", &name])
            .status()
            .unwrap();
        assert!(status.success());
        let session = crate::harness::SessionRef {
            harness: "opencode",
            session_id: "ctl".to_owned(),
            nickname: "ctl".to_owned(),
            path: std::path::PathBuf::from("/tmp/ctl.db"),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size: 0,
            tmux: Some(name.clone()),
            tmux_socket: Some(guard.socket.clone()),
            parent: None,
        };
        let opencode = Opencode;
        let outcome = opencode.send(&session, "hello from boop").unwrap();
        assert_eq!(outcome, crate::harness::SendOutcome::Injected);
        let pane = std::process::Command::new("tmux")
            .args(["-L", &guard.socket, "capture-pane", "-t", &name, "-p"])
            .output()
            .unwrap()
            .stdout;
        assert!(
            String::from_utf8_lossy(&pane).contains("hello from boop"),
            "injected text not found in pane"
        );
    }

    fn has_session_on(guard: &TmuxGuard, name: &str) -> bool {
        std::process::Command::new("tmux")
            .args(["-L", &guard.socket, "has-session", "-t", name])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
