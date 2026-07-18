//! Agent-harness session readers (the at-rest tier).
//!
//! One `AgentHarness` impl per coding-agent harness, mirroring `type_langs()`.
//! Each reads the harness's on-disk session store for `repo_root` and returns
//! the newest session's file edits with REPO-RELATIVE paths. The engine projects
//! these into the built-in `agent_edit` / `latest_touch` relations, so a dl page
//! joins `changed` (the worktree diff) against the latest turn's edits with no
//! adapter rule. ACP is the live/primary tier (see
//! plans/2026-06-28-agent-turn-builtin-and-harness-plug.md); these cover plain
//! terminal runs the daemon can only observe at rest.

// This module reads a THIRD-PARTY tool's own SQLite schema (opencode's
// session store) read-only; it is not sprefa's data model, so `Db` (welded
// to the engine's own schema/pragmas/scalar-fn registry) does not fit.
// @rusqlite-ok: foreign schema needs a plain, unopinionated connection.
use rusqlite::Connection;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TurnEdit {
    /// Turn index within the session: every edit of one turn shares it, so the
    /// engine's `max(idx)` selects a whole turn's file set, not a single edit.
    pub idx: i64,
    /// Repo-relative, forward-slashed.
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct AgentSession {
    pub id: String,
    pub edits: Vec<TurnEdit>,
}

pub trait AgentHarness {
    fn name(&self) -> &'static str;
    /// Newest session for `repo_root` (absolute), best-effort: a missing store
    /// or parse error yields an empty Vec, never an error.
    fn sessions_for(&self, repo_root: &Path) -> Vec<AgentSession>;
    /// Skills loaded in the newest session, as (session_id, skill_name): explicit
    /// `Skill` tool calls plus dl's own prior hook injections. Default none (a
    /// harness whose store we can't read for skills).
    fn skill_loads(&self, _repo_root: &Path) -> Vec<(String, String)> { vec![] }
}

pub fn agent_harnesses() -> Vec<Box<dyn AgentHarness>> {
    vec![Box::new(ClaudeCodeJsonl), Box::new(OpenCodeDb)]
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Strip `repo_root` from an absolute path, forward-slash. None if outside root.
fn relativize(repo_root: &Path, abs: &str) -> Option<String> {
    Path::new(abs)
        .strip_prefix(repo_root)
        .ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
}

// ---- Claude Code: ~/.claude/projects/<slug>/<session>.jsonl ------------------

pub struct ClaudeCodeJsonl;

/// Claude Code's project-dir slug: the abs repo path with every non-alphanumeric
/// byte replaced by '-' (so "/Users/x/p" -> "-Users-x-p").
fn cc_slug(repo_root: &Path) -> String {
    repo_root
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Parse one Claude Code JSONL transcript: assistant `tool_use`
/// Edit/Write/MultiEdit `input.file_path`, repo-relative. `idx` = 1-based line.
pub fn cc_edits_from_text(text: &str, repo_root: &Path) -> Vec<TurnEdit> {
    let mut edits = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let idx = (i + 1) as i64;
        let Ok(rec) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if rec.get("type").and_then(|v| v.as_str()) != Some("assistant") { continue; }
        let Some(content) = rec.pointer("/message/content").and_then(|v| v.as_array()) else { continue };
        for c in content {
            if c.get("type").and_then(|v| v.as_str()) != Some("tool_use") { continue; }
            match c.get("name").and_then(|v| v.as_str()) {
                Some("Edit") | Some("Write") | Some("MultiEdit") => {}
                _ => continue,
            }
            let Some(fp) = c.pointer("/input/file_path").and_then(|v| v.as_str()) else { continue };
            if let Some(rel) = relativize(repo_root, fp) {
                edits.push(TurnEdit { idx, path: rel });
            }
        }
    }
    edits
}

/// The newest `.jsonl` transcript for `repo_root`, or None. Shared by the
/// edit reader and the skill-load reader.
fn cc_newest_transcript(repo_root: &Path) -> Option<PathBuf> {
    let base = match std::env::var_os("SPREFA_CLAUDE_PROJECTS") {
        Some(p) => PathBuf::from(p),
        None => home()?.join(".claude").join("projects"),
    };
    let rd = std::fs::read_dir(base.join(cc_slug(repo_root))).ok()?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for ent in rd.flatten() {
        let p = ent.path();
        if p.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }
        let Ok(mt) = ent.metadata().and_then(|m| m.modified()) else { continue };
        if newest.as_ref().map_or(true, |(t, _)| mt > *t) { newest = Some((mt, p)); }
    }
    newest.map(|(_, p)| p)
}

/// Skills loaded in the newest Claude Code transcript, as (session_id,
/// skill_name): explicit `Skill` tool calls PLUS dl's own past hook injections
/// (the `additionalContext` marker lands in the transcript). Powers the built-in
/// `skill_loaded` relation behind the hook's declarative "load once" guard.
pub fn cc_skill_loads(repo_root: &Path) -> Vec<(String, String)> {
    let Some(path) = cc_newest_transcript(repo_root) else { return vec![] };
    let sid = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let Ok(text) = std::fs::read_to_string(&path) else { return vec![] };
    let mut out = Vec::new();
    for line in text.lines() {
        // dl's own injection: the additionalContext marker is recorded verbatim.
        if line.contains("(auto-loaded by dl --hook)") {
            if let Some(name) = marker_skill(line) { out.push((sid.clone(), name)); }
        }
        // an explicit `Skill` tool call
        let Ok(rec) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if rec.get("type").and_then(|v| v.as_str()) != Some("assistant") { continue; }
        let Some(content) = rec.pointer("/message/content").and_then(|v| v.as_array()) else { continue };
        for c in content {
            if c.get("type").and_then(|v| v.as_str()) != Some("tool_use") { continue; }
            if c.get("name").and_then(|v| v.as_str()) != Some("Skill") { continue; }
            if let Some(name) = c.pointer("/input/skill").and_then(|v| v.as_str()) {
                out.push((sid.clone(), name.to_string()));
            }
        }
    }
    out
}

/// Pull `<name>` from a `Skill \`<name>\` (auto-loaded by dl --hook)` marker line.
fn marker_skill(line: &str) -> Option<String> {
    let i = line.find("Skill `")? + "Skill `".len();
    let rest = &line[i..];
    let j = rest.find('`')?;
    Some(rest[..j].to_string())
}

impl AgentHarness for ClaudeCodeJsonl {
    fn name(&self) -> &'static str { "claude-code" }
    fn skill_loads(&self, repo_root: &Path) -> Vec<(String, String)> {
        cc_skill_loads(repo_root)
    }
    fn sessions_for(&self, repo_root: &Path) -> Vec<AgentSession> {
        // SPREFA_CLAUDE_PROJECTS overrides ~/.claude/projects (testing / non-std).
        let base = match std::env::var_os("SPREFA_CLAUDE_PROJECTS") {
            Some(p) => PathBuf::from(p),
            None => match home() { Some(h) => h.join(".claude").join("projects"), None => return vec![] },
        };
        let dir = base.join(cc_slug(repo_root));
        let Ok(rd) = std::fs::read_dir(&dir) else { return vec![] };
        let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension().and_then(|e| e.to_str()) != Some("jsonl") { continue; }
            let Ok(mt) = ent.metadata().and_then(|m| m.modified()) else { continue };
            if newest.as_ref().map_or(true, |(t, _)| mt > *t) { newest = Some((mt, p)); }
        }
        let Some((_, path)) = newest else { return vec![] };
        let Ok(text) = std::fs::read_to_string(&path) else { return vec![] };
        let edits = cc_edits_from_text(&text, repo_root);
        if edits.is_empty() { return vec![]; }
        let id = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        vec![AgentSession { id, edits }]
    }
}

// ---- opencode: ~/.local/share/opencode/opencode.db --------------------------

pub struct OpenCodeDb;

/// The newest session's edit/write file edits for `repo_root`, repo-relative.
/// `idx` = the message's rank by time (every edit in a message shares it, so a
/// multi-file turn surfaces as one `max(idx)`, mirroring Claude Code's per-line).
pub fn oc_edits_from_conn(conn: &Connection, repo_root: &Path) -> Vec<AgentSession> {
    let dir = repo_root.to_string_lossy().to_string();
    let sid: Option<String> = conn
        .query_row(
            "SELECT id FROM session WHERE directory=?1 ORDER BY time_updated DESC LIMIT 1",
            [&dir],
            |r| r.get(0),
        )
        .ok();
    let Some(sid) = sid else { return vec![] };
    let Ok(mut stmt) = conn.prepare("SELECT message_id, time_created, data FROM part WHERE session_id=?1") else { return vec![] };
    let Ok(rows) = stmt.query_map([&sid], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?))
    }) else { return vec![] };
    // (message_id, time, repo-relative path) for edit/write tool parts.
    let mut raw: Vec<(String, i64, String)> = Vec::new();
    for row in rows.flatten() {
        let (mid, t, data) = row;
        let Ok(j) = serde_json::from_str::<serde_json::Value>(&data) else { continue };
        if j.get("type").and_then(|v| v.as_str()) != Some("tool") { continue; }
        match j.get("tool").and_then(|v| v.as_str()) {
            Some("edit") | Some("write") => {}
            _ => continue,
        }
        let Some(fp) = j.pointer("/state/input/filePath").and_then(|v| v.as_str()) else { continue };
        if let Some(rel) = relativize(repo_root, fp) { raw.push((mid, t, rel)); }
    }
    if raw.is_empty() { return vec![]; }
    // Message turn index = rank by the message's max time_created.
    let mut msg_time: std::collections::BTreeMap<String, i64> = Default::default();
    for (mid, t, _) in &raw {
        let e = msg_time.entry(mid.clone()).or_insert(*t);
        if *t > *e { *e = *t; }
    }
    let mut msgs: Vec<(String, i64)> = msg_time.into_iter().collect();
    msgs.sort_by_key(|(_, t)| *t);
    let rank: std::collections::HashMap<String, i64> =
        msgs.into_iter().enumerate().map(|(i, (m, _))| (m, (i + 1) as i64)).collect();
    let edits = raw
        .into_iter()
        .map(|(mid, _, path)| TurnEdit { idx: rank[&mid], path })
        .collect();
    vec![AgentSession { id: sid, edits }]
}

impl AgentHarness for OpenCodeDb {
    fn name(&self) -> &'static str { "opencode" }
    fn sessions_for(&self, repo_root: &Path) -> Vec<AgentSession> {
        // SPREFA_OPENCODE_DB overrides the default db path (testing / non-std).
        let db = match std::env::var_os("SPREFA_OPENCODE_DB") {
            Some(p) => PathBuf::from(p),
            None => match home() { Some(h) => h.join(".local/share/opencode/opencode.db"), None => return vec![] },
        };
        if !db.exists() { return vec![]; }
        // @rusqlite-ok: read-only open of opencode's own db, not sprefa's schema.
        let Ok(conn) = Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { return vec![] };
        oc_edits_from_conn(&conn, repo_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cc_parses_assistant_tool_use_edits_repo_relative() {
        let root = Path::new("/repo");
        let text = [
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/repo/b.txt"}}]}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/repo/a.txt"}},{"type":"tool_use","name":"Write","input":{"file_path":"/outside/x.txt"}}]}}"#,
        ].join("\n");
        let edits = cc_edits_from_text(&text, root);
        // b.txt on line 2, a.txt on line 3; /outside dropped (not under root).
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].path, "b.txt");
        assert_eq!(edits[0].idx, 2);
        assert_eq!(edits[1].path, "a.txt");
        assert_eq!(edits[1].idx, 3);
        let maxidx = edits.iter().map(|e| e.idx).max().unwrap();
        let latest: Vec<&str> = edits.iter().filter(|e| e.idx == maxidx).map(|e| e.path.as_str()).collect();
        assert_eq!(latest, vec!["a.txt"]); // latest turn touched a.txt
    }

    #[test]
    fn oc_groups_message_edits_into_one_turn_idx() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session(id TEXT, directory TEXT, time_updated INT);
             CREATE TABLE part(message_id TEXT, session_id TEXT, time_created INT, data TEXT);
             INSERT INTO session VALUES('s1','/repo',100);",
        ).unwrap();
        let mk = |tool: &str, fp: &str| format!(
            r#"{{"type":"tool","tool":"{tool}","state":{{"input":{{"filePath":"{fp}"}}}}}}"#);
        // message m1 (older) edits b.txt; message m2 (newer) edits a.txt + c.txt.
        conn.execute("INSERT INTO part VALUES('m1','s1',10,?1)", [mk("edit","/repo/b.txt")]).unwrap();
        conn.execute("INSERT INTO part VALUES('m2','s1',20,?1)", [mk("edit","/repo/a.txt")]).unwrap();
        conn.execute("INSERT INTO part VALUES('m2','s1',21,?1)", [mk("write","/repo/c.txt")]).unwrap();
        let sessions = oc_edits_from_conn(&conn, Path::new("/repo"));
        assert_eq!(sessions.len(), 1);
        let edits = &sessions[0].edits;
        let maxidx = edits.iter().map(|e| e.idx).max().unwrap();
        let mut latest: Vec<&str> = edits.iter().filter(|e| e.idx == maxidx).map(|e| e.path.as_str()).collect();
        latest.sort();
        assert_eq!(latest, vec!["a.txt", "c.txt"]); // newest message's two files share max idx
    }
}
