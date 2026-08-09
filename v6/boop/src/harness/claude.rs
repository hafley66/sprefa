//! The claude adapter: transcripts under `~/.claude/projects/<encoded-cwd>/`.
#![allow(dead_code)]

use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use serde_json::Value;
use walkdir::WalkDir;

use crate::event::{Access, AgentEvent, ToolPath};
use crate::harness::{Capabilities, Harness, ReadChunk, SendOutcome, SessionRef, SpawnSpec};
use crate::tail;

/// The claude harness. Stateless; the trait methods read straight from disk.
pub struct Claude;

impl Harness for Claude {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn sessions(&self) -> anyhow::Result<Vec<SessionRef>> {
        let base = claude_projects_dir()?;
        let mut sessions = Vec::new();
        for entry in WalkDir::new(&base).into_iter().filter_map(|entry| entry.ok()) {
            if !entry.file_type().is_file() || entry.path().extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            let path = entry.path();
            let session_id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string();
            let metadata = path.metadata().ok();
            let modified_ms = metadata
                .as_ref()
                .and_then(|meta| meta.modified().ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0);
            let size = metadata.map(|meta| meta.len()).unwrap_or(0);

            let (cwd, git_branch) = first_record_context(path);
            sessions.push(SessionRef {
                harness: "claude",
                session_id,
                path: path.to_path_buf(),
                cwd,
                git_branch,
                modified_ms,
                size,
                tmux: None,
                tmux_socket: None,
                parent: None,
            });
        }
        sessions.sort_by_key(|session| session.modified_ms);
        Ok(sessions)
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

    fn capabilities(&self) -> crate::harness::Capabilities {
        Capabilities {
            send_midflight: true,
            resume: true,
            spawn: true,
            subagent_visible: true,
        }
    }

    fn spawn(&self, spec: &SpawnSpec) -> anyhow::Result<SessionRef> {
        let session_id = format!("agent-{}", random_hex());
        let tmux_name = format!("boop-{session_id}");
        let cwd = crate::worktree::prepare_spawn_dir(spec)?;
        let command = launch_command(spec);
        crate::tmux::new_detached_session(
            spec.socket.as_deref(),
            &tmux_name,
            &cwd.display().to_string(),
            &command,
        )?;
        Ok(SessionRef {
            harness: "claude",
            session_id: session_id.clone(),
            path: cwd.join(session_id).with_extension("jsonl"),
            cwd: Some(cwd.display().to_string()),
            git_branch: Some(spec.branch.clone()),
            modified_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            size: 0,
            tmux: Some(tmux_name),
            tmux_socket: spec.socket.clone(),
            parent: None,
        })
    }

    fn send(&self, session: &SessionRef, text: &str) -> anyhow::Result<SendOutcome> {
        match &session.tmux {
            Some(tmux) => {
                crate::tmux::send_keys_literal(session.tmux_socket.as_deref(), tmux, text)?;
                Ok(SendOutcome::Injected)
            }
            None => Ok(SendOutcome::QueuedForNextSpawn),
        }
    }

    fn stop(&self, session: &SessionRef) -> anyhow::Result<()> {
        if let Some(tmux) = &session.tmux {
            if crate::tmux::has_session(session.tmux_socket.as_deref(), tmux)? {
                crate::tmux::kill_session(session.tmux_socket.as_deref(), tmux)?;
            }
        }
        Ok(())
    }
}

/// The claude command line a spawn runs. Resuming an existing session wins
/// over a fresh prompt.
fn launch_command(spec: &SpawnSpec) -> String {
    match &spec.resume_session {
        Some(id) => format!("claude --resume {id}"),
        None => format!("claude {}", shell_quote(&spec.prompt)),
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
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

fn claude_projects_dir() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join(".claude").join("projects"))
}

/// Read the first complete line to recover the session cwd and git branch,
/// which some claude records omit.
fn first_record_context(path: &std::path::Path) -> (Option<String>, Option<String>) {
    let Ok(file) = File::open(path) else {
        return (None, None);
    };
    let mut file = file;
    let Ok(result) = tail::read_complete_lines(&mut file, 0) else {
        return (None, None);
    };
    let Some(first) = result.lines.into_iter().next() else {
        return (None, None);
    };
    let Ok(value) = serde_json::from_slice::<Value>(&first.bytes) else {
        return (None, None);
    };
    (
        value.get("cwd").and_then(Value::as_str).map(str::to_owned),
        value.get("gitBranch").and_then(Value::as_str).map(str::to_owned),
    )
}

/// Decode one JSONL line into an `AgentEvent`. An unrecognized record shape is
/// still an event, never an error and never dropped. Returns `None` only for a
/// line that fails to parse as JSON.
fn parse_line(session: &SessionRef, line: &tail::CompleteLine) -> Option<AgentEvent> {
    let value: Value = serde_json::from_slice(&line.bytes).ok()?;

    let record_type = value.get("type").and_then(Value::as_str).unwrap_or_default().to_owned();
    let ts_ms = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_iso_ms)
        .unwrap_or(0);
    let uuid = value.get("uuid").and_then(Value::as_str).map(str::to_owned);
    let parent_uuid = value.get("parentUuid").and_then(Value::as_str).map(str::to_owned);
    let record_cwd = value.get("cwd").and_then(Value::as_str).map(str::to_owned);
    let record_branch = value.get("gitBranch").and_then(Value::as_str).map(str::to_owned);
    let session_id = value
        .get("sessionId")
        .or_else(|| value.get("session_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| session.session_id.clone());

    let mut tool_name = None;
    let mut paths = Vec::new();
    let mut urls = Vec::new();
    collect_tool_use(&value, &mut tool_name, &mut paths, &mut urls);

    Some(AgentEvent {
        harness: session.harness,
        session_id,
        ts_ms,
        uuid,
        parent_uuid,
        cwd: record_cwd.or_else(|| session.cwd.clone()),
        git_branch: record_branch.or_else(|| session.git_branch.clone()),
        record_type,
        tool_name,
        paths,
        urls,
        raw_line_offset: line.start,
    })
}

/// Walk `message.content` for tool_use blocks and surface tool name, file paths,
/// and urls from them.
fn collect_tool_use(
    value: &Value,
    tool_name: &mut Option<String>,
    paths: &mut Vec<ToolPath>,
    urls: &mut Vec<String>,
) {
    let Some(message) = value.get("message") else {
        return;
    };
    let Some(content) = message.get("content").and_then(Value::as_array) else {
        return;
    };
    for block in content {
        let Some(block) = block.as_object() else { continue };
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let Some(name) = block.get("name").and_then(Value::as_str) else { continue };
        *tool_name = Some(name.to_owned());
        let input = block.get("input").and_then(Value::as_object);
        if let Some(file_path) = input.and_then(|input| input.get("file_path")).and_then(Value::as_str) {
            let access = match name {
                "Read" => Access::Read,
                _ => Access::Write,
            };
            paths.push(ToolPath {
                path: file_path.to_owned(),
                access,
            });
        }
        if let Some(url) = input.and_then(|input| input.get("url")).and_then(Value::as_str) {
            urls.push(url.to_owned());
        }
    }
}

/// Parse an ISO-8601 UTC timestamp into ms since the epoch.
pub(crate) fn parse_iso_ms(text: &str) -> Option<u64> {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    let parsed = OffsetDateTime::parse(text, &Rfc3339).ok()?;
    let seconds = parsed.unix_timestamp();
    u64::try_from(seconds)
        .ok()?
        .checked_mul(1000)?
        .checked_add(parsed.millisecond() as u64)
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::PathBuf;

    use crate::harness::Harness;
    use crate::harness::SessionRef;

    use super::{launch_command, parse_iso_ms, Claude};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("boop_claude_{}_{}", std::process::id(), name))
    }

    fn write_lines(path: &PathBuf, lines: &[&str]) {
        let mut file =
            OpenOptions::new().create(true).truncate(true).write(true).open(path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    fn session_for(path: &PathBuf, size: u64) -> SessionRef {
        SessionRef {
            harness: "claude",
            session_id: "test-session".to_owned(),
            path: path.clone(),
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
    fn parses_the_corpus_timestamp_shape() {
        assert_eq!(parse_iso_ms("2026-08-06T16:56:57.904Z"), Some(1_786_035_417_904));
        assert_eq!(parse_iso_ms("2026-01-01T00:00:00.000Z"), Some(1_767_225_600_000));
    }

    #[test]
    fn skips_an_invalid_json_line_but_keeps_the_rest() {
        let path = temp_path("jn1");
        write_lines(
            &path,
            &[
                r#"{"type":"user","sessionId":"s1","uuid":"u1"}"#,
                r#"this is not json at all"#,
                r#"{"type":"assistant","sessionId":"s1","uuid":"u2"}"#,
            ],
        );
        let metadata = path.metadata().unwrap();
        let claude = Claude;
        let chunk = claude
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 2);
        assert_eq!(chunk.skipped, 1);
        assert_eq!(chunk.next_offset, metadata.len());
        assert_eq!(chunk.events[0].record_type, "user");
        assert_eq!(chunk.events[1].record_type, "assistant");
    }

    #[test]
    fn extracts_file_paths_and_urls_from_tool_use() {
        let path = temp_path("jn2");
        let record = r#"{"type":"assistant","sessionId":"s1","uuid":"u1","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/tmp/x.rs"}},{"type":"tool_use","name":"WebFetch","input":{"url":"https://example.com"}},{"type":"tool_use","name":"Read","input":{"file_path":"/tmp/y.rs"}}]}}"#;
        write_lines(&path, &[record]);
        let metadata = path.metadata().unwrap();
        let claude = Claude;
        let chunk = claude
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 1);
        let event = &chunk.events[0];
        assert_eq!(event.tool_name.as_deref(), Some("Read"));
        assert_eq!(event.paths.len(), 2);
        assert_eq!(event.paths[0].path, "/tmp/x.rs");
        assert_eq!(event.paths[1].path, "/tmp/y.rs");
        assert_eq!(event.urls, vec!["https://example.com"]);
    }

    // ---- facet 3 ----

    /// A throwaway tmux server on its own socket; drop kills the whole server.
    struct TmuxGuard {
        socket: String,
    }

    impl TmuxGuard {
        fn new() -> TmuxGuard {
            let socket = format!("boop-test-{}-ctl", std::process::id());
            let _ = std::process::Command::new("tmux")
                .args(["-L", &socket, "kill-server"])
                .status();
            TmuxGuard { socket }
        }
    }

    impl Drop for TmuxGuard {
        fn drop(&mut self) {
            let _ = std::process::Command::new("tmux")
                .args(["-L", &self.socket, "kill-server"])
                .status();
        }
    }

    fn spec(guard: &TmuxGuard) -> crate::harness::SpawnSpec {
        crate::harness::SpawnSpec {
            harness: "claude",
            branch: "lane-test".to_owned(),
            base_sha: "0000000000000000000000000000000000000000".to_owned(),
            main_tree: true,
            setup: Vec::new(),
            prompt: "do the lane".to_owned(),
            resume_session: None,
            socket: Some(guard.socket.clone()),
            worktree_dir: None,
            repo: std::env::temp_dir(),
        }
    }

    /// A throwaway git repo (one seed commit) plus a worktree path; tests
    /// spawn against it and tear it down.
    mod temp_repo {
        use std::process::Command;

        pub struct TempRepo {
            pub dir: std::path::PathBuf,
            pub sha: String,
            pub worktree: std::path::PathBuf,
        }

        impl TempRepo {
            pub fn new() -> TempRepo {
                let dir = std::env::temp_dir().join(format!("boop-cl-repo-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&dir);
                let worktree = std::env::temp_dir().join(format!("boop-cl-wt-{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&worktree);
                Command::new("git").arg("init").arg("-q").arg(&dir).status().unwrap();
                let d = dir.display().to_string();
                Command::new("git").args(["-C", &d, "config", "user.email", "t@t"]).status().unwrap();
                Command::new("git").args(["-C", &d, "config", "user.name", "t"]).status().unwrap();
                std::fs::write(dir.join("seed.txt"), "s").unwrap();
                Command::new("git").args(["-C", &d, "add", "-A"]).status().unwrap();
                Command::new("git").args(["-C", &d, "commit", "-qm", "seed"]).status().unwrap();
                let sha = String::from_utf8_lossy(
                    &Command::new("git").args(["-C", &d, "rev-parse", "HEAD"]).output().unwrap().stdout,
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
    fn claude_capabilities_are_measured() {
        let c = Claude.capabilities();
        assert!(c.send_midflight);
        assert!(c.resume);
        assert!(c.spawn);
        assert!(c.subagent_visible);
    }

    #[test]
    fn claude_spawn_returns_handle_and_stop_tears_down() {
        let guard = TmuxGuard::new();
        let repo = temp_repo::TempRepo::new();
        let worktree = repo.worktree.clone();
        let mut s = spec(&guard);
        s.main_tree = false;
        s.base_sha = repo.sha.clone();
        s.repo = repo.dir.clone();
        s.worktree_dir = Some(worktree.clone());
        let claude = Claude;
        let session = claude.spawn(&s).unwrap();
        assert_eq!(session.tmux.as_deref().map(|t| t.starts_with("boop-agent-")), Some(true));
        assert_eq!(session.tmux_socket.as_deref(), Some(guard.socket.as_str()));
        assert!(worktree.join("seed.txt").exists(), "worktree must be created by spawn");
        claude.stop(&session).unwrap();
        // The spawned session (a shell claude runs under) must be gone after
        // stop; the guard also kills the whole server regardless.
        assert!(!has_session_on(&guard, session.tmux.as_deref().unwrap()));
    }

    #[test]
    fn claude_send_injects_into_a_live_pane() {
        let guard = TmuxGuard::new();
        // A plain shell session stays alive so the injected text can be read
        // back; the transport is what this capability proves.
        let name = format!("ctl-{}", std::process::id());
        let status = std::process::Command::new("tmux")
            .args(["-L", &guard.socket, "new-session", "-d", "-s", &name])
            .status()
            .unwrap();
        assert!(status.success());
        let session = SessionRef {
            harness: "claude",
            session_id: "ctl".to_owned(),
            path: PathBuf::from("/tmp/ctl.jsonl"),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size: 0,
            tmux: Some(name.clone()),
            tmux_socket: Some(guard.socket.clone()),
            parent: None,
        };
        let claude = Claude;
        let outcome = claude.send(&session, "hello from boop").unwrap();
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

    #[test]
    fn claude_launch_resumes_with_session_id() {
        let mut s = spec(&TmuxGuard::new());
        s.resume_session = Some("abc123".to_owned());
        assert!(launch_command(&s).contains("--resume abc123"));
    }

    fn has_session_on(guard: &TmuxGuard, name: &str) -> bool {
        std::process::Command::new("tmux")
            .args(["-L", &guard.socket, "has-session", "-t", name])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
