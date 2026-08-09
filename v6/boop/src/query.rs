//! The `boop db` read surface: one row shape per fact table, dict ids joined
//! back to TEXT at the read boundary and never stored as TEXT.

use anyhow::Result;

use crate::ident::{Row, Store};

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

    /// Sessions as the query surface exposes them, least recent first.
    pub fn query_sessions(&self, session: Option<&str>, limit: Option<u64>) -> Result<Vec<Row>> {
        let mut sql = String::from(
            "SELECT dict_session.value AS session, agent_session.nickname,
                    dict_harness.value AS harness, dict_cwd.value AS cwd,
                    dict_branch.value AS branch, agent_session.started_ts,
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
        self.rows(&sql, values)
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
        let floor = now_ms.saturating_sub(window_ms) as i64;
        let sql = "
            SELECT dict_session.value AS session, agent_session.nickname,
                   dict_harness.value AS harness, dict_cwd.value AS cwd,
                   parent.value AS parent_session,
                   MAX(agent_turn.ts) AS last_turn_ts,
                   COUNT(agent_turn.turn) AS turns,
                   (SELECT COUNT(*) FROM agent_usage u
                      WHERE u.session_id = agent_session.session_id AND u.ts >= ?1) AS calls_in_window,
                   (SELECT COALESCE(SUM(u.input_tokens + u.output_tokens + u.cache_create_5m_tokens
                                        + u.cache_create_1h_tokens + u.cache_read_tokens), 0)
                      FROM agent_usage u
                      WHERE u.session_id = agent_session.session_id AND u.ts >= ?1) AS tokens_in_window
            FROM agent_session
            JOIN dict_session ON dict_session.id = agent_session.session_id
            JOIN dict_harness ON dict_harness.id = agent_session.harness_id
            LEFT JOIN dict_cwd ON dict_cwd.id = agent_session.cwd_id
            LEFT JOIN agent_turn ON agent_turn.session_id = agent_session.session_id
            LEFT JOIN agent_edge ON agent_edge.child_session_id = agent_session.session_id
            LEFT JOIN dict_session AS parent ON parent.id = agent_edge.parent_session_id
            GROUP BY agent_session.session_id
            HAVING last_turn_ts >= ?1
            ORDER BY last_turn_ts DESC";
        self.rows(sql, vec![floor.into()])
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
}
