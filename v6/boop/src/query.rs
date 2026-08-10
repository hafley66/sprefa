//! The `boop db` read surface: one row shape per fact table, dict ids joined
//! back to TEXT at the read boundary and never stored as TEXT.

use anyhow::Result;
use rusqlite::params;

use crate::ident::{Row, Store, TurnQuery};
use crate::rows::{CommandRow, FactCursor, FetchRow, SessionRow, StatusRow, TouchRow, TurnRow};

fn opt_string(value: Option<&str>) -> rusqlite::types::Value {
    value.map(|v| v.to_owned()).into()
}

fn opt_i64(value: Option<u64>) -> rusqlite::types::Value {
    value.map(|v| v as i64).into()
}

/// One fact table the `db` tree lists. Variant names are the CLI nouns.
#[derive(Clone, Copy, Debug)]
pub enum FactKind {
    Touch,
    Command,
    Fetch,
    Skill,
    Pr,
    Span,
}

impl FactKind {
    /// Table, plus the dict columns to join and the field each answers to.
    fn plan(
        self,
    ) -> (
        &'static str,
        &'static [(&'static str, &'static str, &'static str)],
    ) {
        match self {
            FactKind::Touch => (
                "agent_touch",
                &[
                    ("path_id", "dict_path", "path"),
                    ("verb_id", "dict_verb", "verb"),
                ],
            ),
            FactKind::Command => ("agent_cmd", &[("program_id", "dict_program", "program")]),
            FactKind::Fetch => (
                "agent_fetch",
                &[
                    ("url_id", "dict_url", "url"),
                    ("domain_id", "dict_domain", "domain"),
                    ("kind_id", "dict_netkind", "kind"),
                ],
            ),
            FactKind::Skill => ("agent_skill", &[("skill_id", "dict_skill", "skill")]),
            FactKind::Pr => ("agent_pr", &[("pr_url_id", "dict_pr", "pr_url")]),
            FactKind::Span => ("agent_span", &[("path_id", "dict_path", "path")]),
        }
    }

    fn has_ts(self) -> bool {
        !matches!(self, FactKind::Skill | FactKind::Pr | FactKind::Span)
    }
}

/// The filter every fact list shares. `None` means no filter.
#[derive(Default, Clone)]
pub struct FactQuery {
    pub session: Option<String>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    /// Prefix-matched against the kind's first dict column.
    pub like: Option<String>,
    pub limit: Option<u64>,
}

impl Store {
    /// List one fact table.
    pub fn query_facts(&self, kind: FactKind, filter: &FactQuery) -> Result<Vec<Row>> {
        let (table, dicts) = kind.plan();
        let mut columns = vec!["dict_session.value AS session".to_owned()];
        let mut joins =
            format!(" FROM {table} JOIN dict_session ON dict_session.id = {table}.session_id");
        for (index, (column, dict, name)) in dicts.iter().enumerate() {
            let alias = format!("d{index}");
            columns.push(format!("{alias}.value AS {name}"));
            joins.push_str(&format!(
                " LEFT JOIN {dict} AS {alias} ON {alias}.id = {table}.{column}"
            ));
        }
        columns.push(format!("{table}.turn"));
        if kind.has_ts() {
            columns.push(format!("{table}.ts"));
        }
        if matches!(kind, FactKind::Command) {
            columns.push(format!("{table}.argline"));
        }
        if matches!(kind, FactKind::Fetch) {
            columns.push(format!("{table}.query"));
        }
        if matches!(kind, FactKind::Span) {
            columns.push(format!("{table}.line_start"));
            columns.push(format!("{table}.line_end"));
        }
        let mut sql = format!("SELECT {}{joins} WHERE 1=1", columns.join(", "));
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(session) = &filter.session {
            sql.push_str(" AND dict_session.value = ?");
            values.push(session.clone().into());
        }
        if kind.has_ts() {
            if let Some(since) = filter.since {
                sql.push_str(&format!(" AND {table}.ts >= ?"));
                values.push((since as i64).into());
            }
            if let Some(until) = filter.until {
                sql.push_str(&format!(" AND {table}.ts < ?"));
                values.push((until as i64).into());
            }
        }
        if let Some(like) = &filter.like {
            sql.push_str(" AND d0.value LIKE ?");
            values.push(format!("{like}%").into());
        }
        sql.push_str(&format!(" ORDER BY {table}.session_id, {table}.turn"));
        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        self.rows(&sql, values)
    }

    /// Tool touches as typed rows, with the canonical verb and the raw spelling.
    pub fn touch_rows(&self, filter: &FactQuery) -> Result<Vec<TouchRow>> {
        let mut sql = String::from(
            "SELECT dict_session.value, t.turn, t.ts, dp.value, dv.value, drv.value
             FROM agent_touch t
             JOIN dict_session ON dict_session.id = t.session_id
             JOIN dict_path dp ON dp.id = t.path_id
             JOIN dict_verb dv ON dv.id = t.verb_id
             JOIN dict_verb drv ON drv.id = t.raw_verb_id
             WHERE 1=1",
        );
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(session) = &filter.session {
            sql.push_str(" AND dict_session.value = ?");
            values.push(session.clone().into());
        }
        if let Some(since) = filter.since {
            sql.push_str(" AND t.ts >= ?");
            values.push((since as i64).into());
        }
        if let Some(until) = filter.until {
            sql.push_str(" AND t.ts < ?");
            values.push((until as i64).into());
        }
        if let Some(like) = &filter.like {
            sql.push_str(" AND dp.value LIKE ?");
            values.push(format!("{like}%").into());
        }
        sql.push_str(" ORDER BY t.session_id, t.turn");
        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        let mut statement = self.connection().prepare(&sql)?;
        let iter = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok(TouchRow {
                session: row.get(0)?,
                turn: row.get(1)?,
                ts: row.get(2)?,
                path: row.get(3)?,
                verb: row.get(4)?,
                raw_verb: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in iter {
            out.push(row?);
        }
        Ok(out)
    }

    /// Shell commands as typed rows.
    pub fn command_rows(&self, filter: &FactQuery) -> Result<Vec<CommandRow>> {
        let mut sql = String::from(
            "SELECT dict_session.value, c.turn, c.ts, dp.value, c.argline
             FROM agent_cmd c
             JOIN dict_session ON dict_session.id = c.session_id
             JOIN dict_program dp ON dp.id = c.program_id
             WHERE 1=1",
        );
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(session) = &filter.session {
            sql.push_str(" AND dict_session.value = ?");
            values.push(session.clone().into());
        }
        if let Some(since) = filter.since {
            sql.push_str(" AND c.ts >= ?");
            values.push((since as i64).into());
        }
        if let Some(until) = filter.until {
            sql.push_str(" AND c.ts < ?");
            values.push((until as i64).into());
        }
        if let Some(like) = &filter.like {
            sql.push_str(" AND dp.value LIKE ?");
            values.push(format!("{like}%").into());
        }
        sql.push_str(" ORDER BY c.session_id, c.turn");
        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        let mut statement = self.connection().prepare(&sql)?;
        let iter = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok(CommandRow {
                session: row.get(0)?,
                turn: row.get(1)?,
                ts: row.get(2)?,
                program: row.get(3)?,
                argline: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in iter {
            out.push(row?);
        }
        Ok(out)
    }

    /// Network acts (fetch and search) as typed rows.
    pub fn fetch_rows(&self, filter: &FactQuery) -> Result<Vec<FetchRow>> {
        let mut sql = String::from(
            "SELECT dict_session.value, f.turn, f.ts, u.value, d.value, nk.value, f.query
             FROM agent_fetch f
             JOIN dict_session ON dict_session.id = f.session_id
             LEFT JOIN dict_url u ON u.id = f.url_id
             LEFT JOIN dict_domain d ON d.id = f.domain_id
             LEFT JOIN dict_netkind nk ON nk.id = f.kind_id
             WHERE 1=1",
        );
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(session) = &filter.session {
            sql.push_str(" AND dict_session.value = ?");
            values.push(session.clone().into());
        }
        if let Some(since) = filter.since {
            sql.push_str(" AND f.ts >= ?");
            values.push((since as i64).into());
        }
        if let Some(until) = filter.until {
            sql.push_str(" AND f.ts < ?");
            values.push((until as i64).into());
        }
        if let Some(like) = &filter.like {
            sql.push_str(" AND u.value LIKE ?");
            values.push(format!("{like}%").into());
        }
        sql.push_str(" ORDER BY f.session_id, f.turn");
        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        let mut statement = self.connection().prepare(&sql)?;
        let iter = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok(FetchRow {
                session: row.get(0)?,
                turn: row.get(1)?,
                ts: row.get(2)?,
                url: row.get(3)?,
                domain: row.get(4)?,
                kind: row.get(5)?,
                query: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in iter {
            out.push(row?);
        }
        Ok(out)
    }

    /// Turns as typed rows, filtered like `query_turns`.
    pub fn turn_rows(&self, query: &TurnQuery) -> Result<Vec<TurnRow>> {
        let sql = "SELECT s.value, h.value, t.turn, t.ts, r.value, t.said
                   FROM agent_turn t
                   JOIN dict_session s ON s.id = t.session_id
                   JOIN dict_harness h ON h.id = (SELECT harness_id FROM agent_session a WHERE a.session_id = t.session_id)
                   JOIN dict_role r ON r.id = t.role_id
                   WHERE (?1 IS NULL OR h.value = ?1)
                     AND (?2 IS NULL OR t.session_id IN (SELECT id FROM dict_session WHERE value = ?2))
                     AND (?3 IS NULL OR r.value = ?3)
                     AND (?4 IS NULL OR t.ts >= ?4)
                     AND (?5 IS NULL OR t.ts <= ?5)
                     AND (?6 IS NULL OR t.turn >= ?6)
                     AND (?7 IS NULL OR t.turn <= ?7)
                     AND (?8 IS NULL OR t.session_id IN (
                         SELECT DISTINCT tc.session_id FROM agent_touch tc
                         JOIN dict_path p ON p.id = tc.path_id WHERE p.value LIKE ?8))
                   ORDER BY t.session_id, t.turn";
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        for value in [
            query.harness.as_deref(),
            query.session.as_deref(),
            query.role.as_deref(),
        ] {
            values.push(opt_string(value));
        }
        values.push(opt_i64(query.since));
        values.push(opt_i64(query.until));
        values.push(opt_i64(query.turn_from));
        values.push(opt_i64(query.turn_to));
        let like = query.path.as_deref().map(|path| format!("{path}%"));
        values.push(opt_string(like.as_deref()));
        let mut statement = self.connection().prepare(sql)?;
        let limit = query.limit;
        let base = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok(TurnRow {
                session: row.get(0)?,
                harness: row.get(1)?,
                turn: row.get(2)?,
                ts: row.get(3)?,
                role: row.get(4)?,
                said: row.get(5)?,
            })
        })?;
        let mut out = base.collect::<Result<Vec<_>, _>>()?;
        out.truncate(limit.unwrap_or(u64::MAX) as usize);
        Ok(out)
    }

    /// The per-transcript resume cursor for each session: the harness, session
    /// id, transcript path, byte offset, and the last emitted record fields.
    pub fn query_cursors(&self, session: Option<&str>) -> Result<Vec<FactCursor>> {
        let sql = "SELECT dict_harness.value, dict_session.value, dict_path.value, cursor.offset,
                          COALESCE(dict_record.value, ''), cursor.turn, cursor.timestamp
                   FROM sync_cursor cursor
                   JOIN dict_session ON dict_session.id = cursor.session_id
                   JOIN dict_path ON dict_path.id = cursor.path_id
                   JOIN agent_session ON agent_session.session_id = cursor.session_id
                   JOIN dict_harness ON dict_harness.id = agent_session.harness_id
                   LEFT JOIN dict_record ON dict_record.id = cursor.record_id_id
                   WHERE (?1 IS NULL OR dict_session.value = ?1)
                   ORDER BY dict_session.value, dict_path.value";
        let mut statement = self.connection().prepare(sql)?;
        let filter: Option<String> = session.map(str::to_owned);
        let iter = statement.query_map(params![filter], |row| {
            Ok(FactCursor {
                harness: row.get(0)?,
                session: row.get(1)?,
                transcript: row.get(2)?,
                byte_offset: row.get::<_, i64>(3)? as u64,
                record_id: row.get(4)?,
                turn: row.get::<_, i64>(5)? as u64,
                timestamp: row.get::<_, i64>(6)? as u64,
            })
        })?;
        let mut out = Vec::new();
        for row in iter {
            out.push(row?);
        }
        Ok(out)
    }

    /// Sessions as the query surface exposes them, least recent first.
    pub fn query_sessions(&self, session: Option<&str>, limit: Option<u64>) -> Result<Vec<Row>> {
        let rows = self.session_rows(session, limit)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(serde_json::to_value(row)?);
        }
        Ok(out)
    }

    /// Sessions as typed rows.
    pub fn session_rows(
        &self,
        session: Option<&str>,
        limit: Option<u64>,
    ) -> Result<Vec<SessionRow>> {
        let mut sql = String::from(
            "SELECT dict_session.value, agent_session.nickname,
                    dict_harness.value, dict_cwd.value, dict_branch.value,
                    agent_session.started_ts,
                    (SELECT COUNT(*) FROM agent_turn t WHERE t.session_id = agent_session.session_id) AS turns,
                    (SELECT MAX(t.ts) FROM agent_turn t WHERE t.session_id = agent_session.session_id) AS last_ts
             FROM agent_session
             JOIN dict_session ON dict_session.id = agent_session.session_id
             JOIN dict_harness ON dict_harness.id = agent_session.harness_id
             LEFT JOIN dict_cwd ON dict_cwd.id = agent_session.cwd_id
             LEFT JOIN dict_branch ON dict_branch.id = agent_session.branch_id
             WHERE 1=1",
        );
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(session) = session {
            sql.push_str(" AND dict_session.value = ?");
            values.push(session.to_string().into());
        }
        sql.push_str(" ORDER BY last_ts");
        if let Some(limit) = limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        let mut statement = self.connection().prepare(&sql)?;
        let iter = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok(SessionRow {
                session: row.get(0)?,
                nickname: row.get(1)?,
                harness: row.get(2)?,
                cwd: row.get(3)?,
                branch: row.get(4)?,
                started_ts: row.get(5)?,
                turns: row.get(6)?,
                last_ts: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for row in iter {
            out.push(row?);
        }
        Ok(out)
    }

    /// How far ingest has read each transcript.
    pub fn query_sync_cursors(&self, limit: Option<u64>) -> Result<Vec<Row>> {
        let mut sql = String::from(
            "SELECT dict_session.value AS session, dict_path.value AS path, sync_cursor.offset
             FROM sync_cursor
             JOIN dict_session ON dict_session.id = sync_cursor.session_id
             JOIN dict_path ON dict_path.id = sync_cursor.path_id
             ORDER BY dict_session.value",
        );
        if let Some(limit) = limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        self.rows(&sql, Vec::new())
    }

    /// One row per session that moved inside the window. Liveness is layered on
    /// by the caller, the only part that needs tmux.
    pub fn query_status(&self, window_ms: u64, now_ms: u64) -> Result<Vec<Row>> {
        let rows = self.status_rows(window_ms, now_ms)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(serde_json::to_value(row)?);
        }
        Ok(out)
    }

    /// Sessions that moved inside the window, as typed rows.
    pub fn status_rows(&self, window_ms: u64, now_ms: u64) -> Result<Vec<StatusRow>> {
        let floor = now_ms.saturating_sub(window_ms) as i64;
        let sql = "
            SELECT dict_session.value, agent_session.nickname,
                   dict_harness.value, dict_cwd.value,
                   parent.value,
                   MAX(agent_turn.ts) AS last_turn_ts,
                   COUNT(agent_turn.turn) AS turns,
                   (SELECT COUNT(*) FROM agent_usage u
                      WHERE u.session_id = agent_session.session_id AND u.ts >= ?1) AS calls_in_window,
                   (SELECT COALESCE(SUM(u.input_tokens + u.output_tokens + u.cache_create_5m_tokens
                                        + u.cache_create_1h_tokens + u.cache_read_tokens), 0)
                      FROM agent_usage u
                      WHERE u.session_id = agent_session.session_id AND u.ts >= ?1) AS tokens_in_window,
                   NULL AS lane,
                   live_status.value AS state,
                   live.pid,
                   live_pane.value AS tmux_pane,
                   NULL AS rss_kb,
                   NULL AS cpu_pct,
                   NULL AS uptime_sec,
                   (SELECT MIN(from_ts) FROM agent_live_span span
                      WHERE span.session_id = agent_session.session_id) AS first_seen_ts,
                   (SELECT MAX(COALESCE(to_ts, from_ts)) FROM agent_live_span span
                      WHERE span.session_id = agent_session.session_id) AS last_seen_ts,
                   (SELECT MAX(to_ts) FROM agent_live_span span
                      WHERE span.session_id = agent_session.session_id AND to_ts IS NOT NULL
                        AND NOT EXISTS (SELECT 1 FROM agent_live_span open_span
                                        WHERE open_span.session_id = span.session_id
                                          AND open_span.to_ts IS NULL)) AS died_ts
            FROM agent_session
            JOIN dict_session ON dict_session.id = agent_session.session_id
            JOIN dict_harness ON dict_harness.id = agent_session.harness_id
            LEFT JOIN dict_cwd ON dict_cwd.id = agent_session.cwd_id
            LEFT JOIN agent_turn ON agent_turn.session_id = agent_session.session_id
            LEFT JOIN agent_edge ON agent_edge.child_session_id = agent_session.session_id
            LEFT JOIN dict_session AS parent ON parent.id = agent_edge.parent_session_id
            LEFT JOIN agent_live live ON live.session_id = agent_session.session_id
            LEFT JOIN dict_status live_status ON live_status.id = live.status_id
            LEFT JOIN dict_pane live_pane ON live_pane.id = live.tmux_pane_id
            GROUP BY agent_session.session_id
            HAVING last_turn_ts >= ?1
            ORDER BY last_turn_ts DESC";
        let mut statement = self.connection().prepare(sql)?;
        let iter = statement.query_map(params![floor], |row| {
            Ok(StatusRow {
                session: row.get(0)?,
                nickname: row.get(1)?,
                harness: row.get(2)?,
                cwd: row.get(3)?,
                parent_session: row.get(4)?,
                last_turn_ts: row.get(5)?,
                turns: row.get(6)?,
                calls_in_window: row.get(7)?,
                tokens_in_window: row.get(8)?,
                lane: row.get(9)?,
                state: row.get(10)?,
                pid: row.get(11)?,
                tmux_pane: row.get(12)?,
                rss_kb: row.get(13)?,
                cpu_pct: row.get(14)?,
                uptime_sec: row.get(15)?,
                first_seen_ts: row.get(16)?,
                last_seen_ts: row.get(17)?,
                died_ts: row.get(18)?,
            })
        })?;
        let mut out = Vec::new();
        for row in iter {
            out.push(row?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{FactKind, FactQuery};
    use crate::ident::Store;

    fn store() -> (Store, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "boop_query_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&path);
        (Store::open(path.clone()).unwrap(), path)
    }

    /// Every fact list must answer on an empty store rather than error: a new
    /// machine has no rows and `boop db touch list` still has to work.
    #[test]
    fn every_fact_list_runs_on_an_empty_store() {
        let (store, path) = store();
        for kind in [
            FactKind::Touch,
            FactKind::Command,
            FactKind::Fetch,
            FactKind::Skill,
            FactKind::Pr,
            FactKind::Span,
        ] {
            let rows = store
                .query_facts(kind, &FactQuery::default())
                .unwrap_or_else(|error| panic!("{kind:?} failed: {error}"));
            assert!(rows.is_empty(), "{kind:?} on an empty store");
        }
        assert!(store.query_sessions(None, None).unwrap().is_empty());
        assert!(store.query_sync_cursors(None).unwrap().is_empty());
        assert!(store.query_status(600_000, 1_000_000).unwrap().is_empty());
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// The prefix filter binds a parameter; it must never splice the value.
    #[test]
    fn a_path_filter_is_a_bound_prefix() {
        let (store, path) = store();
        let filter = FactQuery {
            like: Some("/tmp/'; DROP TABLE agent_touch; --".to_owned()),
            ..Default::default()
        };
        let rows = store.query_facts(FactKind::Touch, &filter).unwrap();
        assert!(rows.is_empty());
        let alive: i64 = store
            .query_facts(FactKind::Touch, &FactQuery::default())
            .map(|rows| rows.len() as i64)
            .unwrap();
        assert_eq!(alive, 0, "the table must still exist");
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    /// RECEIPT (item 4). Typed rows populate for a real synced session: the
    /// session, its first turn, and its touch's canonical verb with raw kept.
    #[test]
    fn typed_rows_populate_for_a_real_session() {
        use crate::harness::SessionRef;
        use crate::ident::{sync_session, TurnQuery};
        use std::io::Write;

        let (store, db_path) = store();
        let log_path = std::env::temp_dir().join(format!(
            "boop_typed_{}_{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let mut file = std::fs::File::create(&log_path).unwrap();
        writeln!(file, r#"{{"type":"user","sessionId":"ses-1","timestamp":"2026-08-01T00:00:00.100Z","message":"hello"}}"#).unwrap();
        writeln!(file, r#"{{"type":"assistant","sessionId":"ses-1","timestamp":"2026-08-01T00:00:01.000Z","message":{{"content":[{{"type":"tool_use","name":"Read","input":{{"file_path":"/tmp/a.rs"}}}}]}}}}"#).unwrap();
        drop(file);
        let session = SessionRef {
            harness: "claude",
            session_id: "ses-1".to_owned(),
            nickname: "ses-1".to_owned(),
            path: log_path.clone(),
            cwd: Some("/w".to_owned()),
            git_branch: Some("main".to_owned()),
            modified_ms: 0,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        };
        sync_session(&store, &crate::harness::claude::Claude, &session).unwrap();

        let sessions = store.session_rows(Some("ses-1"), None).unwrap();
        assert_eq!(sessions.len(), 1);
        let row = &sessions[0];
        assert_eq!(row.session, "ses-1");
        assert_eq!(row.harness, "claude");
        assert_eq!(row.cwd.as_deref(), Some("/w"));
        assert!(row.turns >= 2, "user text plus the tool turn");
        assert!(row.last_ts.is_some());
        println!("SessionRow receipt: {row:?}");

        let turns = store.turn_rows(&TurnQuery::default()).unwrap();
        assert!(
            turns.iter().any(|t| t.said == "hello"),
            "turn text retained"
        );

        let touches = store.touch_rows(&FactQuery::default()).unwrap();
        assert_eq!(touches.len(), 1);
        let touch = &touches[0];
        assert_eq!(touch.verb, "read", "canonical lowercase verb");
        assert_eq!(touch.raw_verb, "Read", "raw spelling retained");
        println!("TouchRow receipt: {touch:?}");

        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&log_path);
    }

    /// The item 6 edge evidence is observable through typing too.
    #[test]
    fn edge_rows_typed_receipt() {
        use crate::ident::Store;
        let path = std::env::temp_dir().join(format!(
            "boop_edge_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        store.add_edge_at("p", "c", "hail", 5).unwrap();
        store.add_edge_at("p", "c", "hail", 9).unwrap();
        let edges = store.edge_rows(None).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].n, 2);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }
}
