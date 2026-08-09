//! Layer 3: the relational conversation store at `~/.agent/boop.db`.
//!
//! The base tables of QUERY-SURFACE.md land here, dictionary-encoded per the
//! sql-relational-design law: every JOINED or GROUPED column (session id, cwd,
//! branch, path, role, program, url, skill, pr, harness) is an INTEGER id into
//! a dict table; only payload text (`said`, `argline`, `nickname`) stays TEXT
//! because it is never a key. `boop sync` projects transcripts into these
//! tables; `boop follow` is the same projection in a poll loop.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::harness::SessionRef;

/// The relational store.
pub struct Store {
    connection: Connection,
}

/// Bumped whenever stored rows mean something different. 2 = dense per-session
/// turn ordinals and per-transcript sync cursors. 3 = token usage.
pub const SCHEMA_VERSION: i64 = 3;

/// A row returned to the CLI, joined back from ids to the TEXT the query
/// surface exposes.
pub type Row = serde_json::Value;

/// One sync pass's row accounting. `dropped` can only rise when two writers
/// race on one ordinal, so a non-zero value is a defect signal, not a stat.
#[derive(Default, Clone, Copy, Debug)]
pub struct SyncStat {
    pub written: u64,
    pub dropped: u64,
    pub usage_written: u64,
    pub usage_updated: u64,
}

impl SyncStat {
    pub fn add(&mut self, other: SyncStat) {
        self.written += other.written;
        self.dropped += other.dropped;
        self.usage_written += other.usage_written;
        self.usage_updated += other.usage_updated;
    }
}

/// One LLM response's token counts. `input_tokens` EXCLUDES the cached buckets
/// (the opposite of the OTEL convention), so the five columns sum to the total.
pub struct UsageRow<'a> {
    pub ts: u64,
    pub message_id: &'a str,
    pub request_id: &'a str,
    pub model: &'a str,
    pub service_tier: Option<&'a str>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_create_5m_tokens: i64,
    pub cache_create_1h_tokens: i64,
    pub cache_read_tokens: i64,
    pub is_sidechain: bool,
    pub cost_usd_recorded: Option<f64>,
}

/// The per-transcript walk: the last ordinal handed out plus its accounting.
struct Walk {
    turn: u64,
    stat: SyncStat,
}

impl Walk {
    fn record(&mut self, inserted: usize) {
        if inserted == 0 {
            self.stat.dropped += 1;
        } else {
            self.stat.written += 1;
        }
    }
}

/// The shared turn-query filter set.
#[derive(Default, Clone)]
pub struct TurnQuery {
    pub harness: Option<String>,
    pub session: Option<String>,
    pub role: Option<String>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub turn_from: Option<u64>,
    pub turn_to: Option<u64>,
    pub path: Option<String>,
    pub limit: Option<u64>,
}

impl Store {
    pub fn open(path: PathBuf) -> Result<Self> {
        let connection = Connection::open(&path)
            .with_context(|| format!("open boop.db at {}", path.display()))?;
        let tables_before: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        connection
            .execute_batch(SCHEMA)
            .with_context(|| format!("initialise boop.db schema at {}", path.display()))?;
        let store = Store { connection };
        if tables_before == 0 {
            store.stamp_version()?;
        }
        Ok(store)
    }

    fn stamp_version(&self) -> Result<()> {
        self.connection
            .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    /// True when the rows on disk predate this schema version. Reading them is
    /// safe; appending dense ordinals beside sparse ones is not.
    pub fn is_stale(&self) -> Result<bool> {
        if self.schema_version()? >= SCHEMA_VERSION {
            return Ok(false);
        }
        let turns: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM agent_turn", [], |row| row.get(0))?;
        Ok(turns > 0)
    }

    /// Drop every table, recreate the schema, stamp the version. The caller
    /// re-syncs from byte 0; nothing here reads a transcript.
    pub fn rebuild(&self) -> Result<()> {
        let mut names = Vec::new();
        {
            let mut statement = self.connection.prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                names.push(row?);
            }
        }
        for name in &names {
            self.connection
                .execute_batch(&format!("DROP TABLE IF EXISTS \"{name}\""))?;
        }
        self.connection.execute_batch(SCHEMA)?;
        self.stamp_version()?;
        self.connection.execute_batch("VACUUM")?;
        Ok(())
    }

    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("resolve home directory")?;
        Ok(home.join(".agent").join("boop.db"))
    }

    /// Wrap the next writes in one transaction so a batch of per-fact INSERTs
    /// commits once instead of once per row (rusqlite autocommits otherwise).
    pub fn begin(&self) -> Result<()> {
        self.connection.execute_batch("BEGIN")?;
        Ok(())
    }

    pub fn commit(&self) -> Result<()> {
        self.connection.execute_batch("COMMIT")?;
        Ok(())
    }

    pub fn rollback(&self) -> Result<()> {
        self.connection.execute_batch("ROLLBACK")?;
        Ok(())
    }

    fn intern(&self, table: &str, value: &str) -> Result<i64> {
        let insert = format!("INSERT OR IGNORE INTO {table} (id, value) VALUES (NULL, ?1)");
        self.connection.execute(&insert, params![value])?;
        let select = format!("SELECT id FROM {table} WHERE value = ?1");
        Ok(self
            .connection
            .query_row(&select, params![value], |row| row.get(0))?)
    }

    fn session_id(&self, session: &str) -> Result<i64> {
        self.intern("dict_session", session)
    }

    fn upsert_session_row(
        &self,
        session: &str,
        harness: &str,
        nickname: &str,
        cwd: Option<&str>,
        branch: Option<&str>,
        started_ts: u64,
    ) -> Result<()> {
        let sid = self.session_id(session)?;
        let harness_id = self.intern("dict_harness", harness)?;
        let cwd_id = cwd.map(|c| self.intern("dict_cwd", c)).transpose()?;
        let branch_id = branch.map(|b| self.intern("dict_branch", b)).transpose()?;
        self.connection.execute(
            "INSERT INTO agent_session (session_id, harness_id, nickname, cwd_id, branch_id, started_ts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id) DO UPDATE SET harness_id=excluded.harness_id,
               nickname=excluded.nickname, cwd_id=excluded.cwd_id,
               branch_id=excluded.branch_id,
               started_ts=MIN(agent_session.started_ts, excluded.started_ts)",
            params![sid, harness_id, nickname, cwd_id, branch_id, started_ts as i64],
        )?;
        Ok(())
    }

    /// Returns rows inserted: 0 means the ordinal was already taken, which the
    /// caller counts as a defect rather than swallowing.
    fn add_turn(&self, session: &str, turn: u64, ts: u64, role: &str, said: &str) -> Result<usize> {
        let sid = self.session_id(session)?;
        let role_id = self.intern("dict_role", role)?;
        Ok(self.connection.execute(
            "INSERT OR IGNORE INTO agent_turn (session_id, turn, ts, role_id, said)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![sid, turn as i64, ts as i64, role_id, said],
        )?)
    }

    /// Intern the composite natural key of one LLM call. Returns the surrogate
    /// id and whether this call was seen for the first time.
    fn intern_request(&self, message_id: &str, request_id: &str) -> Result<(i64, bool)> {
        let inserted = self.connection.execute(
            "INSERT OR IGNORE INTO dict_request (message_id, request_id) VALUES (?1, ?2)",
            params![message_id, request_id],
        )?;
        let id: i64 = self.connection.query_row(
            "SELECT id FROM dict_request WHERE message_id = ?1 AND request_id = ?2",
            params![message_id, request_id],
            |row| row.get(0),
        )?;
        Ok((id, inserted > 0))
    }

    /// A repeat of a stored call updates counts only when `output_tokens` did
    /// not go backwards; the transcript writes snapshots first, final last.
    fn add_usage(
        &self,
        session: &str,
        turn: u64,
        request_ref: i64,
        is_new: bool,
        usage: &UsageRow,
    ) -> Result<bool> {
        let sid = self.session_id(session)?;
        let model_id = self.intern("dict_model", usage.model)?;
        let tier_id = usage
            .service_tier
            .map(|tier| self.intern("dict_service_tier", tier))
            .transpose()?;
        if is_new {
            self.connection.execute(
                "INSERT INTO agent_usage (session_id, turn, ts, request_ref, model_id,
                   service_tier_id, input_tokens, output_tokens, cache_create_5m_tokens,
                   cache_create_1h_tokens, cache_read_tokens, is_sidechain, cost_usd_recorded)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    sid,
                    turn as i64,
                    usage.ts as i64,
                    request_ref,
                    model_id,
                    tier_id,
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.cache_create_5m_tokens,
                    usage.cache_create_1h_tokens,
                    usage.cache_read_tokens,
                    i64::from(usage.is_sidechain),
                    usage.cost_usd_recorded,
                ],
            )?;
            return Ok(true);
        }
        let changed = self.connection.execute(
            "UPDATE agent_usage SET ts = ?2, input_tokens = ?3, output_tokens = ?4,
               cache_create_5m_tokens = ?5, cache_create_1h_tokens = ?6,
               cache_read_tokens = ?7, cost_usd_recorded = ?8
             WHERE request_ref = ?1 AND ?4 >= output_tokens",
            params![
                request_ref,
                usage.ts as i64,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_create_5m_tokens,
                usage.cache_create_1h_tokens,
                usage.cache_read_tokens,
                usage.cost_usd_recorded,
            ],
        )?;
        Ok(changed > 0)
    }

    /// The highest ordinal already stored for a session; the next sync pass
    /// hands out `max + 1` so a growing transcript never renumbers.
    fn max_turn(&self, session: &str) -> Result<u64> {
        let sid = self.session_id(session)?;
        let max: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(turn), 0) FROM agent_turn WHERE session_id = ?1",
            params![sid],
            |row| row.get(0),
        )?;
        Ok(max as u64)
    }

    /// Run a SELECT and hand back one JSON object per row, keyed by column
    /// name, so a read surface never restates the column list twice.
    pub(crate) fn rows(&self, sql: &str, values: Vec<rusqlite::types::Value>) -> Result<Vec<Row>> {
        let mut statement = self.connection.prepare(sql)?;
        let names: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut out = Vec::new();
        let iter = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            let mut object = serde_json::Map::new();
            for (index, name) in names.iter().enumerate() {
                let value = match row.get_ref(index)? {
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                    rusqlite::types::ValueRef::Integer(number) => serde_json::json!(number),
                    rusqlite::types::ValueRef::Real(number) => serde_json::json!(number),
                    rusqlite::types::ValueRef::Text(text) => {
                        serde_json::json!(String::from_utf8_lossy(text))
                    }
                    rusqlite::types::ValueRef::Blob(_) => serde_json::Value::Null,
                };
                object.insert(name.clone(), value);
            }
            Ok(serde_json::Value::Object(object))
        })?;
        for row in iter {
            out.push(row?);
        }
        Ok(out)
    }

    /// Sessions whose ordinals are not dense. Empty is the invariant every
    /// sync preserves; the CLI prints the count as a sync receipt.
    pub fn sparse_sessions(&self) -> Result<Vec<(String, i64, i64)>> {
        let mut statement = self.connection.prepare(
            "SELECT dict_session.value, COUNT(*), MAX(agent_turn.turn)
             FROM agent_turn
             JOIN dict_session ON dict_session.id = agent_turn.session_id
             GROUP BY agent_turn.session_id
             HAVING COUNT(*) <> MAX(agent_turn.turn)",
        )?;
        let mut out = Vec::new();
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn add_touch(&self, session: &str, turn: u64, ts: u64, path: &str, verb: &str) -> Result<()> {
        let sid = self.session_id(session)?;
        let path_id = self.intern("dict_path", path)?;
        let verb_id = self.intern("dict_verb", verb)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_touch (session_id, turn, ts, path_id, verb_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![sid, turn as i64, ts as i64, path_id, verb_id],
        )?;
        Ok(())
    }

    fn add_cmd(
        &self,
        session: &str,
        turn: u64,
        ts: u64,
        program: &str,
        argline: &str,
    ) -> Result<()> {
        let sid = self.session_id(session)?;
        let program_id = self.intern("dict_program", program)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_cmd (session_id, turn, ts, program_id, argline)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![sid, turn as i64, ts as i64, program_id, argline],
        )?;
        Ok(())
    }

    fn add_fetch(&self, session: &str, turn: u64, ts: u64, url: &str, domain: &str) -> Result<()> {
        let sid = self.session_id(session)?;
        let url_id = self.intern("dict_url", url)?;
        let domain_id = self.intern("dict_domain", domain)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_fetch (session_id, turn, ts, url_id, domain_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![sid, turn as i64, ts as i64, url_id, domain_id],
        )?;
        Ok(())
    }

    fn add_skill(&self, session: &str, turn: u64, skill: &str) -> Result<()> {
        let sid = self.session_id(session)?;
        let skill_id = self.intern("dict_skill", skill)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_skill (session_id, turn, skill_id) VALUES (?1, ?2, ?3)",
            params![sid, turn as i64, skill_id],
        )?;
        Ok(())
    }

    fn add_pr(&self, session: &str, turn: u64, pr_url: &str) -> Result<()> {
        let sid = self.session_id(session)?;
        let pr_id = self.intern("dict_pr", pr_url)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_pr (session_id, turn, pr_url_id) VALUES (?1, ?2, ?3)",
            params![sid, turn as i64, pr_id],
        )?;
        Ok(())
    }

    fn set_cursor(&self, session: &str, path: &str, offset: u64) -> Result<()> {
        let sid = self.session_id(session)?;
        let path_id = self.intern("dict_path", path)?;
        self.connection.execute(
            "INSERT INTO sync_cursor (session_id, path_id, offset) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id, path_id) DO UPDATE SET offset=excluded.offset",
            params![sid, path_id, offset as i64],
        )?;
        Ok(())
    }

    fn get_cursor(&self, session: &str, path: &str) -> Result<u64> {
        let sid = self.session_id(session)?;
        let path_id = self.intern("dict_path", path)?;
        let offset: i64 = self
            .connection
            .query_row(
                "SELECT offset FROM sync_cursor WHERE session_id = ?1 AND path_id = ?2",
                params![sid, path_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(offset as u64)
    }

    /// Record a spawn edge between two known sessions.
    pub fn add_edge(&self, parent: &str, child: &str, kind: &str) -> Result<()> {
        let parent_id = self.session_id(parent)?;
        let child_id = self.session_id(child)?;
        let kind_id = self.intern("dict_edekind", kind)?;
        self.connection.execute(
            "INSERT OR IGNORE INTO agent_edge (parent_session_id, child_session_id, edge_kind_id)
             VALUES (?1, ?2, ?3)",
            params![parent_id, child_id, kind_id],
        )?;
        Ok(())
    }

    /// Query spawn edges, joined back to the TEXT query surface. `session`
    /// filters to edges touching that session id; none means all edges.
    pub fn query_edges(&self, session: Option<&str>) -> Result<Vec<Row>> {
        let mut sql = String::from(
            "SELECT p.value AS parent, c.value AS child, e.value AS edge
             FROM agent_edge a
             JOIN dict_session p ON p.id = a.parent_session_id
             JOIN dict_session c ON c.id = a.child_session_id
             JOIN dict_edekind e ON e.id = a.edge_kind_id
             WHERE 1=1",
        );
        if session.is_some() {
            sql.push_str(" AND (c.value = ?1 OR p.value = ?1)");
        }
        sql.push_str(" ORDER BY p.value, c.value");
        let mut statement = self.connection.prepare(&sql)?;
        let params: Vec<rusqlite::types::Value> = session
            .map(|value| vec![value.to_string().into()])
            .unwrap_or_default();
        let mut rows = Vec::new();
        let iter = statement.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(serde_json::json!({
                "kind": "agent_edge",
                "parent": row.get::<_, String>(0)?,
                "child": row.get::<_, String>(1)?,
                "edge": row.get::<_, String>(2)?,
            }))
        })?;
        for row in iter {
            rows.push(row?);
        }
        Ok(rows)
    }

    /// Table row counts, for receipts.
    pub fn counts(&self) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut out = serde_json::Map::new();
        for table in [
            "agent_session",
            "agent_turn",
            "agent_touch",
            "agent_cmd",
            "agent_fetch",
            "agent_skill",
            "agent_pr",
            "agent_usage",
        ] {
            let n: i64 =
                self.connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })?;
            out.insert(table.into(), serde_json::json!(n));
        }
        Ok(out)
    }

    pub fn db_bytes(&self) -> Result<u64> {
        let bytes: i64 = self
            .connection
            .pragma_query_value(None, "page_count", |row| row.get(0))?;
        let size: i64 = self
            .connection
            .pragma_query_value(None, "page_size", |row| row.get(0))?;
        Ok((bytes * size) as u64)
    }

    // ------------------------------------------------------------------
    // Queries (join ids back to the TEXT query surface)
    // ------------------------------------------------------------------

    /// Query turns with the shared filter set. Returns JSON rows with the TEXT
    /// query surface joined back out.
    pub fn query_turns(&self, query: &TurnQuery) -> Result<Vec<Row>> {
        let harness = query.harness.as_deref();
        let session = query.session.as_deref();
        let role = query.role.as_deref();
        let since = query.since;
        let until = query.until;
        let turn_from = query.turn_from;
        let turn_to = query.turn_to;
        let path = query.path.as_deref();
        let limit = query.limit;
        let mut sql = String::from(
            "SELECT s.value AS session, h.value AS harness, t.turn, t.ts, r.value AS role, t.said
             FROM agent_turn t
             JOIN dict_session s ON s.id = t.session_id
             JOIN dict_harness h ON h.id = (SELECT harness_id FROM agent_session a WHERE a.session_id = t.session_id)
             JOIN dict_role r ON r.id = t.role_id
             WHERE 1=1",
        );
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(harness) = harness {
            sql.push_str(" AND h.value = ?");
            values.push(harness.to_string().into());
        }
        if let Some(session) = session {
            sql.push_str(" AND t.session_id IN (SELECT id FROM dict_session WHERE value = ?)");
            values.push(session.to_string().into());
        }
        if let Some(role) = role {
            sql.push_str(" AND r.value = ?");
            values.push(role.to_string().into());
        }
        if let Some(since) = since {
            sql.push_str(" AND t.ts >= ?");
            values.push((since as i64).into());
        }
        if let Some(until) = until {
            sql.push_str(" AND t.ts <= ?");
            values.push((until as i64).into());
        }
        if let Some(turn_from) = turn_from {
            sql.push_str(" AND t.turn >= ?");
            values.push((turn_from as i64).into());
        }
        if let Some(turn_to) = turn_to {
            sql.push_str(" AND t.turn <= ?");
            values.push((turn_to as i64).into());
        }
        if let Some(path) = path {
            sql.push_str(
                " AND t.session_id IN (
                    SELECT DISTINCT tc.session_id FROM agent_touch tc
                    JOIN dict_path p ON p.id = tc.path_id WHERE p.value LIKE ?) ",
            );
            values.push(format!("{path}%").into());
        }
        sql.push_str(" ORDER BY t.session_id, t.turn");
        if let Some(limit) = limit {
            sql.push_str(" LIMIT ");
            sql.push_str(&limit.to_string());
        }
        let mut statement = self.connection.prepare(&sql)?;
        let mut rows = Vec::new();
        let iter = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok(serde_json::json!({
                "session": row.get::<_, String>(0)?,
                "harness": row.get::<_, String>(1)?,
                "turn": row.get::<_, i64>(2)?,
                "ts": row.get::<_, i64>(3)?,
                "role": row.get::<_, String>(4)?,
                "said": row.get::<_, String>(5)?,
            }))
        })?;
        for row in iter {
            rows.push(row?);
        }
        Ok(rows)
    }
}

/// Project one transcript file forward from its stored cursor, writing session,
/// turn, touch, cmd, fetch, skill, pr facts. Returns the new offset. A second
/// run with nothing appended after the cursor writes nothing.
pub fn sync_session(store: &Store, session: &SessionRef) -> Result<SyncStat> {
    let offset = store.get_cursor(&session.session_id, &session.path.display().to_string())?;
    let mut file = std::fs::File::open(&session.path)
        .map_err(|error| anyhow::anyhow!("open {}: {error}", session.path.display()))?;
    let result = crate::tail::read_complete_lines(&mut file, offset)?;
    if result.lines.is_empty() {
        return Ok(SyncStat::default());
    }
    store.upsert_session_row(
        &session.session_id,
        session.harness,
        &session.nickname,
        session.cwd.as_deref(),
        session.git_branch.as_deref(),
        session.modified_ms,
    )?;
    if let Some(parent) = &session.parent {
        store.add_edge(parent, &session.session_id, "spawned")?;
    }
    let mut walk = Walk {
        turn: store.max_turn(&session.session_id)?,
        stat: SyncStat::default(),
    };
    for line in &result.lines {
        project_line(store, session, line, &mut walk)?;
    }
    store.set_cursor(
        &session.session_id,
        &session.path.display().to_string(),
        result.next_offset,
    )?;
    Ok(walk.stat)
}

/// Walk one complete line and emit its turns and typed facts.
fn project_line(
    store: &Store,
    session: &SessionRef,
    line: &crate::tail::CompleteLine,
    walk: &mut Walk,
) -> Result<()> {
    let value: serde_json::Value = match serde_json::from_slice(&line.bytes) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let object = match value.as_object() {
        Some(object) => object,
        None => return Ok(()),
    };
    let ts = object
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .and_then(crate::harness::claude::parse_iso_ms)
        .unwrap_or(0);
    let sid = session.session_id.clone();
    let record_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    if record_type == "pr-link" {
        let pr_url = object
            .get("prUrl")
            .or_else(|| object.get("url"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if !pr_url.is_empty() {
            walk.turn += 1;
            let inserted = store.add_turn(&sid, walk.turn, ts, "system", "")?;
            walk.record(inserted);
            store.add_pr(&sid, walk.turn, pr_url)?;
        }
        return Ok(());
    }

    if record_type != "user" && record_type != "assistant" {
        return Ok(());
    }
    let role = record_type;
    let Some(message) = object.get("message") else {
        return Ok(());
    };
    // `message` is an object with `content`, or a bare string on older user
    // records; both are one or more text/tool blocks.
    let blocks: Vec<serde_json::Value> = match message {
        serde_json::Value::String(text) => vec![serde_json::json!({"type":"text","text":text})],
        object => {
            let content = object.get("content");
            if let Some(text) = content.and_then(serde_json::Value::as_str) {
                vec![serde_json::json!({"type":"text","text":text})]
            } else {
                content
                    .and_then(serde_json::Value::as_array)
                    .map(|a| a.to_vec())
                    .unwrap_or_default()
            }
        }
    };
    let mut first_turn: Option<u64> = None;
    for block in &blocks {
        let Some(block) = block.as_object() else {
            continue;
        };
        let kind = block
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        // An ordinal is spent only when a row is written, so COUNT(*) equals
        // MAX(turn) per session and `turn` is a usable count.
        match kind {
            "text" => {
                let said = block
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                walk.turn += 1;
                let inserted = store.add_turn(&sid, walk.turn, ts, role, said)?;
                walk.record(inserted);
                first_turn.get_or_insert(walk.turn);
            }
            "tool_use" => {
                let name = block
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let input = block.get("input");
                walk.turn += 1;
                let inserted = store.add_turn(&sid, walk.turn, ts, "tool", "")?;
                walk.record(inserted);
                first_turn.get_or_insert(walk.turn);
                emit_tool_fact(store, &sid, walk.turn, ts, name, input)?;
            }
            _ => {}
        }
    }

    if let Some(usage) = parse_usage(object, message, ts) {
        let (request_ref, is_new) = store.intern_request(usage.message_id, usage.request_id)?;
        // Usage belongs to the whole response: it takes the first stored block's
        // ordinal, or its own when every block was thinking (31.1% measured).
        let turn = match (is_new, first_turn) {
            (false, _) => 0,
            (true, Some(turn)) => turn,
            (true, None) => {
                walk.turn += 1;
                let inserted = store.add_turn(&sid, walk.turn, ts, role, "")?;
                walk.record(inserted);
                walk.turn
            }
        };
        if store.add_usage(&sid, turn, request_ref, is_new, &usage)? {
            if is_new {
                walk.stat.usage_written += 1;
            } else {
                walk.stat.usage_updated += 1;
            }
        }
    }
    Ok(())
}

/// Read `message.usage` off one assistant record. `None` for every record that
/// is not an LLM response, which is every user record and every tool result.
fn parse_usage<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    message: &'a serde_json::Value,
    ts: u64,
) -> Option<UsageRow<'a>> {
    let usage = message.get("usage")?.as_object()?;
    let message_id = message.get("id").and_then(serde_json::Value::as_str)?;
    let count = |key: &str| -> i64 {
        usage
            .get(key)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    let split = usage.get("cache_creation").and_then(|v| v.as_object());
    let split_count = |key: &str| -> Option<i64> {
        split?.get(key).and_then(serde_json::Value::as_i64)
    };
    // The flat cache_creation_input_tokens cannot say which window it bought,
    // so it falls into the 5m bucket; 99.7% of records carry the split.
    let (create_5m, create_1h) = match (
        split_count("ephemeral_5m_input_tokens"),
        split_count("ephemeral_1h_input_tokens"),
    ) {
        (None, None) => (count("cache_creation_input_tokens"), 0),
        (five, hour) => (five.unwrap_or(0), hour.unwrap_or(0)),
    };
    Some(UsageRow {
        ts,
        message_id,
        request_id: object
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
        model: message
            .get("model")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown"),
        service_tier: usage.get("service_tier").and_then(serde_json::Value::as_str),
        input_tokens: count("input_tokens"),
        output_tokens: count("output_tokens"),
        cache_create_5m_tokens: create_5m,
        cache_create_1h_tokens: create_1h,
        cache_read_tokens: count("cache_read_input_tokens"),
        is_sidechain: object
            .get("isSidechain")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        cost_usd_recorded: object.get("costUSD").and_then(serde_json::Value::as_f64),
    })
}

fn emit_tool_fact(
    store: &Store,
    session: &str,
    turn: u64,
    ts: u64,
    name: &str,
    input: Option<&serde_json::Value>,
) -> Result<()> {
    let field = |key: &str| -> Option<String> {
        input
            .and_then(serde_json::Value::as_object)
            .and_then(|o| o.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    match name {
        "Read" | "Write" | "Edit" | "List" | "Glob" | "MultiEdit" | "Grep" => {
            let path = field("file_path")
                .or_else(|| field("pattern"))
                .or_else(|| field("path"));
            if let Some(path) = path {
                store.add_touch(session, turn, ts, &path, name)?;
            }
        }
        "Bash" => {
            let command = field("command").unwrap_or_default();
            if !command.is_empty() {
                let mut parts = command.splitn(2, char::is_whitespace);
                let program = parts.next().unwrap_or("").to_owned();
                let argline = parts.next().unwrap_or("").to_owned();
                store.add_cmd(session, turn, ts, &program, &argline)?;
            }
        }
        "WebFetch" => {
            if let Some(url) = field("url") {
                let domain = domain_of(&url);
                store.add_fetch(session, turn, ts, &url, &domain)?;
            }
        }
        "Skill" => {
            let skill = field("skill").or_else(|| field("name")).unwrap_or_default();
            if !skill.is_empty() {
                store.add_skill(session, turn, &skill)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn domain_of(url: &str) -> String {
    let remainder = url.split("://").nth(1).unwrap_or(url);
    remainder
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_owned()
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS dict_session (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_harness (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_cwd (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_branch (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_role (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_path (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_verb (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_program (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_url (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_domain (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_skill (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_pr (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_edekind (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_agenttype (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_status (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_model (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_service_tier (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

-- request_id is NOT NULL with '' for absent: SQLite treats NULLs in a UNIQUE
-- index as distinct, and 8.4% of measured records carry no requestId.
CREATE TABLE IF NOT EXISTS dict_request (
  id INTEGER PRIMARY KEY,
  message_id TEXT NOT NULL,
  request_id TEXT NOT NULL DEFAULT '',
  UNIQUE (message_id, request_id)
);

CREATE TABLE IF NOT EXISTS agent_session (
  session_id INTEGER PRIMARY KEY,
  harness_id INTEGER NOT NULL,
  nickname TEXT,
  cwd_id INTEGER,
  branch_id INTEGER,
  started_ts INTEGER
);
CREATE INDEX IF NOT EXISTS idx_session_cwd ON agent_session(cwd_id);

CREATE TABLE IF NOT EXISTS agent_turn (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  role_id INTEGER NOT NULL,
  said TEXT,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_touch (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  path_id INTEGER NOT NULL,
  verb_id INTEGER NOT NULL,
  PRIMARY KEY (session_id, turn, path_id, verb_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_touch_pathid ON agent_touch(path_id);

CREATE TABLE IF NOT EXISTS agent_cmd (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  program_id INTEGER NOT NULL,
  argline TEXT,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_fetch (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER,
  url_id INTEGER NOT NULL,
  domain_id INTEGER NOT NULL,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_skill (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  skill_id INTEGER NOT NULL,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_pr (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  pr_url_id INTEGER NOT NULL,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_span (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  line_start INTEGER,
  line_end INTEGER,
  PRIMARY KEY (session_id, turn, path_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_edge (
  parent_session_id INTEGER NOT NULL,
  child_session_id INTEGER NOT NULL,
  edge_kind_id INTEGER NOT NULL,
  agent_type_id INTEGER,
  model_id INTEGER,
  PRIMARY KEY (parent_session_id, child_session_id, edge_kind_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS agent_usage (
  session_id INTEGER NOT NULL,
  turn INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  request_ref INTEGER NOT NULL,
  model_id INTEGER NOT NULL,
  service_tier_id INTEGER,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cache_create_5m_tokens INTEGER NOT NULL DEFAULT 0,
  cache_create_1h_tokens INTEGER NOT NULL DEFAULT 0,
  cache_read_tokens INTEGER NOT NULL DEFAULT 0,
  is_sidechain INTEGER NOT NULL DEFAULT 0,
  cost_usd_recorded REAL,
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;
CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_request ON agent_usage(request_ref);
CREATE INDEX IF NOT EXISTS idx_usage_ts ON agent_usage(ts);
CREATE INDEX IF NOT EXISTS idx_usage_model_ts ON agent_usage(model_id, ts);

CREATE TABLE IF NOT EXISTS agent_live (
  session_id INTEGER PRIMARY KEY,
  pid INTEGER,
  tmux_pane_id INTEGER,
  status_id INTEGER
);

CREATE TABLE IF NOT EXISTS sync_cursor (
  session_id INTEGER NOT NULL,
  path_id INTEGER NOT NULL,
  offset INTEGER NOT NULL,
  PRIMARY KEY (session_id, path_id)
) WITHOUT ROWID;
";

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::PathBuf;

    use crate::harness::SessionRef;

    use super::{sync_session, Store};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("boop_rel_{}_{}", std::process::id(), name))
    }

    fn session_for(path: &std::path::Path) -> SessionRef {
        SessionRef {
            harness: "claude",
            session_id: "ses-1".to_owned(),
            nickname: "ses-1".to_owned(),
            path: path.to_path_buf(),
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
    fn sync_projects_facts_and_cursor_is_incremental() {
        let db_path = temp_path("db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();

        let lines_path = temp_path("log");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.100Z","gitBranch":"main","message":"hello"}}"#).unwrap();
        writeln!(file, r#"{{"type":"assistant","sessionId":"ses-1","timestamp":"2026-08-01T00:00:01.000Z","gitBranch":"main","message":{{"content":[{{"type":"tool_use","name":"Read","input":{{"file_path":"/tmp/a.rs"}}}},{{"type":"tool_use","name":"Bash","input":{{"command":"git diff --stat"}}}}]}}}}"#).unwrap();
        drop(file);

        let session = session_for(&lines_path);
        let first = sync_session(&store, &session).unwrap();
        assert_eq!(first.written, 3, "user text, tool Read, tool Bash");
        assert_eq!(first.dropped, 0);

        let counts = store.counts().unwrap();
        assert_eq!(counts["agent_turn"], 3);
        assert_eq!(counts["agent_touch"], 1);
        assert_eq!(counts["agent_cmd"], 1);
        assert_eq!(counts["agent_session"], 1);

        // second sync with nothing new carries the cursor and writes nothing
        let noop = sync_session(&store, &session).unwrap();
        assert_eq!(noop.written, 0);
        let counts2 = store.counts().unwrap();
        assert_eq!(counts2["agent_turn"], 3);

        // query turns back out as TEXT rows
        let filter = super::TurnQuery {
            session: Some("ses-1".to_owned()),
            ..Default::default()
        };
        let rows = store.query_turns(&filter).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["role"], "user");
        assert_eq!(rows[0]["said"], "hello");
        assert_eq!(rows[2]["role"], "tool");

        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    #[test]
    fn sync_writes_the_spawn_edge() {
        let db_path = temp_path("db3");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();

        let lines_path = temp_path("log3");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(
            file,
            r#"{{"type":"user","sessionId":"child","message":"go"}}"#
        )
        .unwrap();
        let mut session = session_for(&lines_path);
        session.session_id = "child".to_owned();
        session.parent = Some("parent".to_owned());
        drop(file);
        sync_session(&store, &session).unwrap();

        let edges = store.query_edges(Some("child")).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["kind"], "agent_edge");
        assert_eq!(edges[0]["parent"], "parent");
        assert_eq!(edges[0]["child"], "child");
        assert_eq!(edges[0]["edge"], "spawned");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    /// FAIL-FIRST (D1). A per-pass `turn = 0` renumbered from 1 into stored
    /// ordinals, and `add_turn`'s `INSERT OR IGNORE` dropped the collisions.
    #[test]
    fn ordinals_continue_across_an_incremental_sync() {
        let db_path = temp_path("d1db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines_path = temp_path("d1log");
        let _ = std::fs::remove_file(&lines_path);

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.100Z","message":"one"}}"#).unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.200Z","message":"two"}}"#).unwrap();
        drop(file);

        let session = session_for(&lines_path);
        sync_session(&store, &session).unwrap();

        let mut file = OpenOptions::new().append(true).open(&lines_path).unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.300Z","message":"three"}}"#).unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.400Z","message":"four"}}"#).unwrap();
        drop(file);

        sync_session(&store, &session).unwrap();

        let counts = store.counts().unwrap();
        assert_eq!(counts["agent_turn"], 4, "all four turns stored");

        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    /// FAIL-FIRST (D2). The ordinal ticked per content block but only text and
    /// tool_use wrote a row: 1293 of 1312 live sessions had COUNT < MAX(turn).
    #[test]
    fn a_thinking_block_burns_no_ordinal() {
        let db_path = temp_path("d2db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines_path = temp_path("d2log");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        writeln!(file, r#"{{"type":"assistant","sessionId":"ses-1","timestamp":"2026-08-01T00:00:01.000Z","message":{{"content":[{{"type":"thinking","thinking":"hmm"}},{{"type":"text","text":"said"}},{{"type":"tool_result","content":"r"}},{{"type":"tool_use","name":"Read","input":{{"file_path":"/tmp/a.rs"}}}}]}}}}"#).unwrap();
        drop(file);

        let session = session_for(&lines_path);
        sync_session(&store, &session).unwrap();

        let filter = super::TurnQuery {
            session: Some("ses-1".to_owned()),
            ..Default::default()
        };
        let rows = store.query_turns(&filter).unwrap();
        let ordinals: Vec<i64> = rows.iter().map(|row| row["turn"].as_i64().unwrap()).collect();
        assert_eq!(ordinals, vec![1, 2], "ordinals are dense, not 2 and 4");

        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&lines_path);
    }

    /// FAIL-FIRST (D3). `agent_edge.model_id` referenced a `dict_model` table
    /// that no `CREATE TABLE` in SCHEMA ever made.
    #[test]
    fn dict_model_exists() {
        let db_path = temp_path("d3db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let found: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='dict_model'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 1, "agent_edge.model_id needs a real dict_model");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// FAIL-FIRST (D5). One offset per session id made two transcripts sharing
    /// that id fight over it, re-reading one from byte 0 on every sync.
    #[test]
    fn a_cursor_is_per_transcript_not_per_session() {
        let db_path = temp_path("d5db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        store.set_cursor("ses-1", "/a/one.jsonl", 100).unwrap();
        store.set_cursor("ses-1", "/b/one.jsonl", 250).unwrap();
        assert_eq!(store.get_cursor("ses-1", "/a/one.jsonl").unwrap(), 100);
        assert_eq!(store.get_cursor("ses-1", "/b/one.jsonl").unwrap(), 250);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// FAIL-FIRST (migration). A fresh store already carries the version stamp
    /// that tells a pre-dense store apart from a current one.
    #[test]
    fn a_fresh_store_carries_the_schema_version() {
        let db_path = temp_path("mig");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let version: i64 = store
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, super::SCHEMA_VERSION, "a fresh store is stamped");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    fn usage_record(message_id: &str, request: &str, output: i64, extra: &str) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"ses-1","timestamp":"2026-08-01T00:00:01.000Z",{request}"message":{{"id":"{message_id}","model":"claude-opus-5","usage":{{"input_tokens":10,"output_tokens":{output},"cache_read_input_tokens":700,"service_tier":"standard","cache_creation":{{"ephemeral_5m_input_tokens":33,"ephemeral_1h_input_tokens":4}}}},"content":[{extra}]}}}}"#
        )
    }

    fn sync_lines(store: &Store, name: &str, lines: &[String]) -> super::SyncStat {
        let lines_path = temp_path(name);
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lines_path)
            .unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        drop(file);
        let stat = sync_session(store, &session_for(&lines_path)).unwrap();
        let _ = std::fs::remove_file(&lines_path);
        stat
    }

    fn usage_totals(store: &Store) -> (i64, i64, i64, i64) {
        store
            .connection
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(output_tokens),0), COALESCE(SUM(input_tokens),0),
                        COALESCE(SUM(cache_create_5m_tokens + cache_create_1h_tokens + cache_read_tokens),0)
                 FROM agent_usage",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap()
    }

    /// ACCEPTANCE. The corpus writes one response many times while it streams
    /// (2.07x measured); summing raw records doubles every number.
    #[test]
    fn usage_dedups_to_one_row_and_the_last_output_wins() {
        let db_path = temp_path("u1db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines: Vec<String> = [3, 3, 396]
            .iter()
            .map(|out| {
                usage_record(
                    "msg_a",
                    r#""requestId":"req_a","#,
                    *out,
                    r#"{"type":"text","text":"hi"}"#,
                )
            })
            .collect();
        let stat = sync_lines(&store, "u1log", &lines);
        assert_eq!(stat.usage_written, 1, "one insert for one API call");
        assert_eq!(stat.usage_updated, 2, "two snapshots raised the count");
        let (rows, output, input, cached) = usage_totals(&store);
        assert_eq!(rows, 1, "three records, one call");
        assert_eq!(output, 396, "the final count wins, not the first snapshot");
        assert_eq!(input, 10, "input is not summed across snapshots");
        assert_eq!(cached, 33 + 4 + 700);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// A replayed transcript must converge, so a lower output count is refused.
    #[test]
    fn a_lower_output_snapshot_never_wins() {
        let db_path = temp_path("u2db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines = vec![
            usage_record("msg_a", r#""requestId":"req_a","#, 396, r#"{"type":"text","text":"hi"}"#),
            usage_record("msg_a", r#""requestId":"req_a","#, 3, r#"{"type":"text","text":"hi"}"#),
        ];
        sync_lines(&store, "u2log", &lines);
        let (rows, output, _, _) = usage_totals(&store);
        assert_eq!(rows, 1);
        assert_eq!(output, 396, "the 3-token snapshot must not overwrite 396");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// 8.4% of measured records carry no requestId; NULLs in a SQLite UNIQUE
    /// index are distinct, so a nullable column would insert each one twice.
    #[test]
    fn a_missing_request_id_still_dedups() {
        let db_path = temp_path("u3db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines = vec![
            usage_record("msg_b", "", 5, r#"{"type":"text","text":"hi"}"#),
            usage_record("msg_b", "", 50, r#"{"type":"text","text":"hi"}"#),
        ];
        sync_lines(&store, "u3log", &lines);
        let (rows, output, _, _) = usage_totals(&store);
        assert_eq!(rows, 1, "one call, not two");
        assert_eq!(output, 50);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// 31.1% of measured usage records carry only thinking blocks, so the first
    /// stored block's ordinal does not exist for them.
    #[test]
    fn a_thinking_only_response_still_gets_a_usage_row() {
        let db_path = temp_path("u4db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let lines = vec![usage_record(
            "msg_c",
            r#""requestId":"req_c","#,
            77,
            r#"{"type":"thinking","thinking":"hmm"}"#,
        )];
        sync_lines(&store, "u4log", &lines);
        let (rows, output, _, _) = usage_totals(&store);
        assert_eq!(rows, 1);
        assert_eq!(output, 77);
        let turn: i64 = store
            .connection
            .query_row("SELECT turn FROM agent_usage", [], |row| row.get(0))
            .unwrap();
        assert_eq!(turn, 1, "a minted ordinal, dense with the turn table");
        assert!(store.sparse_sessions().unwrap().is_empty());
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    /// The time-range aggregate must seek the ts index, never scan the table.
    #[test]
    fn the_usage_time_range_query_seeks_an_index() {
        let db_path = temp_path("u5db");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let plan: String = store
            .connection
            .query_row(
                "EXPLAIN QUERY PLAN SELECT SUM(output_tokens) FROM agent_usage
                 WHERE ts >= 1 AND ts < 2",
                [],
                |row| row.get(3),
            )
            .unwrap();
        assert!(
            plan.contains("USING INDEX idx_usage_ts"),
            "plan must seek idx_usage_ts, got: {plan}"
        );
        assert!(!plan.contains("SCAN agent_usage"), "plan scans: {plan}");
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn surrogate_ids_intern_once() {
        let db_path = temp_path("db2");
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let a = store.intern("dict_path", "/tmp/a.rs");
        let b = store.intern("dict_path", "/tmp/a.rs");
        assert_eq!(a.unwrap(), b.unwrap());
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }
}
