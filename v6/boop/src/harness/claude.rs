//! The claude adapter: transcripts under `~/.claude/projects/<encoded-cwd>/`.

use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use serde_json::Value;
use walkdir::WalkDir;

use crate::event::{Access, AgentEvent, ToolPath};
use crate::harness::{Harness, ReadChunk, SessionRef};
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

/// Parse an ISO-8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SS.mmmZ`) into ms since
/// the epoch. The transcript corpus writes this exact shape.
fn parse_iso_ms(text: &str) -> Option<u64> {
    let digits = text.as_bytes();
    if digits.len() < 19 || digits[4] != b'-' || digits[7] != b'-' || digits[10] != b'T' {
        return None;
    }
    let year = atoi(&digits[0..4])?;
    let month = atoi(&digits[5..7])?;
    let day = atoi(&digits[8..10])?;
    let hour = atoi(&digits[11..13])?;
    let minute = atoi(&digits[14..16])?;
    let second = atoi(&digits[17..19])?;

    let mut milli = 0u64;
    if digits.get(19) == Some(&b'.') {
        let fraction_digits: Vec<u8> = digits[20..]
            .iter()
            .copied()
            .take_while(|byte| byte.is_ascii_digit())
            .take(3)
            .collect();
        if fraction_digits.is_empty() {
            return None;
        }
        milli = atoi(&fraction_digits)?;
    }

    let days = days_from_civil(year, month, day)?;
    let seconds = days * 86400 + hour * 3600 + minute * 60 + second;
    Some(seconds * 1000 + milli)
}

fn atoi(bytes: &[u8]) -> Option<u64> {
    let mut value: u64 = 0;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + (byte - b'0') as u64;
    }
    Some(value)
}

/// Number of days since 1970-01-01 for a civil date (Hinnant's civil algorithm).
fn days_from_civil(year: u64, month: u64, day: u64) -> Option<u64> {
    if !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    let mut year = year as i64;
    let month = month as i64;
    year -= i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = (year - era * 400) as u64;
    let shift = if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * (month + shift) + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year as u64;
    let epoch_days = era * 146097 + day_of_era as i64 - 719468;
    u64::try_from(epoch_days).ok()
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::PathBuf;

    use crate::harness::Harness;
    use crate::harness::SessionRef;

    use super::{parse_iso_ms, Claude};

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
}
