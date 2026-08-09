//! Layer 3: identity. Every plane issues its own id and none of them join
//! automatically; this layer owns the maps between them. Stored rels key on
//! INTEGER ids; natural TEXT keys live ONCE in dictionary tables with a UNIQUE
//! constraint on the natural key (the repo's surrogate-key law).

use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::harness::Harness;

/// The SQLite identity store at `~/.agent/boop.db`. A corrupt or unreadable db
/// prints its path and fails; it never panics and never silently recreates
/// itself.
pub struct Store {
    connection: Connection,
    path: PathBuf,
}

/// The spawn-tree edge values. `claude` keeps subagents in-file, so it
/// produces none of these; `opencode` and `codex` (later passes) disagree
/// about whether a subagent is a session, and that disagreement is kept as
/// DATA in `session_edge.relation` rather than resolved here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Relation {
    Spawned,
    Resumed,
    Subagent,
}

impl Relation {
    fn as_str(self) -> &'static str {
        match self {
            Relation::Spawned => "spawned",
            Relation::Resumed => "resumed",
            Relation::Subagent => "subagent",
        }
    }
}

#[allow(dead_code)]
impl Store {
    pub fn add_message_edge(&self, parent: i64, child: i64) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO message_edge (parent_message_id, child_message_id)
             VALUES (?1, ?2)",
            params![parent, child],
        )?;
        Ok(())
    }
}

#[allow(dead_code)]
impl Store {
    pub fn open(path: PathBuf) -> Result<Self> {
        let connection = Connection::open(&path)
            .with_context(|| format!("open boop.db at {}", path.display()))?;
        connection
            .execute_batch(SCHEMA)
            .with_context(|| format!("initialise boop.db schema at {}", path.display()))?;
        Ok(Store { connection, path })
    }

    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("resolve home directory")?;
        Ok(home.join(".agent").join("boop.db"))
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    fn intern(&self, table: &str, column: &str, value: &str) -> Result<i64> {
        let sql = format!("INSERT OR IGNORE INTO {table} (id, {column}) VALUES (NULL, ?1)");
        self.connection.execute(&sql, params![value])?;
        let sql = format!("SELECT id FROM {table} WHERE {column} = ?1");
        let id = self
            .connection
            .query_row(&sql, params![value], |row| row.get(0))?;
        Ok(id)
    }

    fn intern_harness(&self, value: &str) -> Result<i64> {
        self.intern("dict_harness", "name", value)
    }

    fn intern_cwd(&self, value: &str) -> Result<i64> {
        self.intern("dict_cwd", "path", value)
    }

    fn intern_branch(&self, value: &str) -> Result<i64> {
        self.intern("dict_branch", "branch", value)
    }

    fn intern_path(&self, value: &str) -> Result<i64> {
        self.intern("dict_path", "path", value)
    }

    fn intern_record_type(&self, value: &str) -> Result<i64> {
        self.intern("dict_record_type", "name", value)
    }

    /// Upsert one harness session; returns its surrogate id.
    pub fn upsert_session(
        &self,
        harness: &str,
        natural_id: &str,
        cwd: Option<&str>,
        branch: Option<&str>,
        first_ts_ms: u64,
        last_ts_ms: u64,
    ) -> Result<i64> {
        let harness_id = self.intern_harness(harness)?;
        let cwd_id = cwd.map(|value| self.intern_cwd(value)).transpose()?;
        let branch_id = branch.map(|value| self.intern_branch(value)).transpose()?;
        self.connection.execute(
            "INSERT OR IGNORE INTO session
             (session_id, harness_id, natural_id, cwd_id, branch_id, first_ts_ms, last_ts_ms)
             VALUES (NULL, ?1, ?2, ?3, ?4, ?5, ?6)",
            params![harness_id, natural_id, cwd_id, branch_id, first_ts_ms as i64, last_ts_ms as i64],
        )?;
        let session_id = self.connection.query_row(
            "SELECT session_id FROM session WHERE natural_id = ?1",
            params![natural_id],
            |row| row.get(0),
        )?;
        Ok(session_id)
    }

    pub fn session_id_for(&self, natural_id: &str) -> Option<i64> {
        self.connection
            .query_row("SELECT session_id FROM session WHERE natural_id = ?1", params![natural_id], |row| row.get(0))
            .ok()
    }

    /// Record a spawn-tree edge between two known sessions.
    pub fn add_session_edge(&self, parent: i64, child: i64, relation: Relation) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO session_edge (parent_session_id, child_session_id, relation)
             VALUES (?1, ?2, ?3)",
            params![parent, child, relation.as_str()],
        )?;
        Ok(())
    }

    /// The distinct `relation` values present in the store, with the child
    /// session's harness, oldest first.
    pub fn session_relations(&self) -> Result<Vec<(String, String)>> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT e.relation, h.name
             FROM session_edge e
             JOIN session s ON s.session_id = e.child_session_id
             JOIN dict_harness h ON h.id = s.harness_id
             ORDER BY h.name, e.relation",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn session_count(&self) -> Result<u64> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM session", [], |row| row.get(0))?;
        Ok(count as u64)
    }
}

/// Record a session row per claude transcript and return how many were added.
/// Claude produces no `session_edge` rows because its subagents live inside one
/// session file rather than as separate on-disk sessions.
pub fn sync_harness(store: &Store, harness: &dyn Harness) -> Result<u64> {
    let mut added = 0u64;
    for session in harness.sessions()? {
        store.upsert_session(
            harness.id(),
            &session.session_id,
            session.cwd.as_deref(),
            session.git_branch.as_deref(),
            session.modified_ms,
            session.modified_ms,
        )?;
        added += 1;
    }
    Ok(added)
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS dict_harness (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_cwd (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_branch (id INTEGER PRIMARY KEY, branch TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_path (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_record_type (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_uuid (id INTEGER PRIMARY KEY, uuid TEXT NOT NULL UNIQUE);

CREATE TABLE IF NOT EXISTS session (
  session_id INTEGER PRIMARY KEY,
  harness_id INTEGER NOT NULL REFERENCES dict_harness(id),
  natural_id TEXT NOT NULL UNIQUE,
  cwd_id INTEGER REFERENCES dict_cwd(id),
  branch_id INTEGER REFERENCES dict_branch(id),
  first_ts_ms INTEGER,
  last_ts_ms INTEGER
);

CREATE TABLE IF NOT EXISTS session_edge (
  parent_session_id INTEGER NOT NULL REFERENCES session(session_id),
  child_session_id INTEGER NOT NULL REFERENCES session(session_id),
  relation TEXT NOT NULL,
  PRIMARY KEY (parent_session_id, child_session_id, relation)
);

CREATE TABLE IF NOT EXISTS message (
  message_id INTEGER PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES session(session_id),
  uuid_id INTEGER REFERENCES dict_uuid(id),
  ts_ms INTEGER,
  record_type_id INTEGER NOT NULL REFERENCES dict_record_type(id),
  byte_offset INTEGER
);

CREATE TABLE IF NOT EXISTS message_edge (
  parent_message_id INTEGER NOT NULL REFERENCES message(message_id),
  child_message_id INTEGER NOT NULL REFERENCES message(message_id),
  PRIMARY KEY (parent_message_id, child_message_id)
);

CREATE TABLE IF NOT EXISTS tool_touch (
  message_id INTEGER NOT NULL REFERENCES message(message_id),
  path_id INTEGER NOT NULL REFERENCES dict_path(id),
  access INTEGER,
  PRIMARY KEY (message_id, path_id, access)
);
";

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Relation, Store};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("boop_ident_{}_{}", std::process::id(), name))
    }

    #[test]
    fn surrogate_keys_intern_a_natural_id_once() {
        let path = temp_path("db1");
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        let first = store.upsert_session("claude", "ses-1", Some("/w"), Some("main"), 1, 2).unwrap();
        let second = store.upsert_session("claude", "ses-1", Some("/w"), Some("main"), 1, 3).unwrap();
        // Same natural id maps to the same surrogate id (UNIQUE constraint).
        assert_eq!(first, second);
        assert_eq!(store.session_id_for("ses-1"), Some(first));
        assert!(store.session_relations().unwrap().is_empty());
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn session_edge_keeps_the_relation_as_data() {
        let path = temp_path("db2");
        let _ = std::fs::remove_file(&path);
        let store = Store::open(path.clone()).unwrap();
        let parent = store.upsert_session("opencode", "p", None, None, 0, 0).unwrap();
        let child = store.upsert_session("opencode", "c", None, None, 0, 0).unwrap();
        store.add_session_edge(parent, child, Relation::Subagent).unwrap();
        let relations = store.session_relations().unwrap();
        assert_eq!(relations, vec![("subagent".to_owned(), "opencode".to_owned())]);
        drop(store);
        let _ = std::fs::remove_file(&path);
    }
}
