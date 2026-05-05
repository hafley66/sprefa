//! `FactStore` — wide-row table store, trait + two impls.
//!
//!   trait FactStore           the surface FactWrite/FactRead consume
//!   MemFactStore              in-memory; HashMap<table, Vec<row>>
//!   SqliteFactStore           sqlite-backed; per-table wide schema with
//!                             FK columns into a shared strings intern table
//!
//! Sqlite shape: `declare(table, cols)` emits
//!   CREATE TABLE <table>_facts (
//!       id            INTEGER PRIMARY KEY AUTOINCREMENT,
//!       generation_id INTEGER NOT NULL DEFAULT 0,
//!       <c>_id        INTEGER NOT NULL REFERENCES sprf_strings(id)
//!       ...
//!   );
//! per-column indexes on `<c>_id`. Insert interns each declared cell into
//! `sprf_strings`; reads JOIN back. Schemas persist in `sprf_fact_schemas`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{Component, Node, RenderCtx};

use crate::Cursor;

pub trait FactStore: Send + Sync + 'static {
    /// Declare a table's column shape. Required for SqliteFactStore before
    /// any insert; no-op for stores that auto-discover columns from rows.
    /// Idempotent for matching schemas; panics on schema conflict.
    fn declare(&self, _table: &str, _cols: &[&str]) {}
    fn insert(&self, table: &str, row: Arc<Cursor>);
    fn read_where(&self, table: &str, col: &str, value: &str) -> Vec<Arc<Cursor>>;
    fn rows_of(&self, table: &str) -> Vec<Arc<Cursor>>;
    fn len(&self, table: &str) -> usize;
}

// ───────────────────────────────────────────────────────────────────
// MemFactStore
// ───────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct MemFactStore {
    tables: Mutex<HashMap<String, Vec<Arc<Cursor>>>>,
}

impl MemFactStore {
    pub fn new() -> Self { Self::default() }
}

impl FactStore for MemFactStore {
    fn insert(&self, table: &str, row: Arc<Cursor>) {
        self.tables
            .lock()
            .unwrap()
            .entry(table.to_string())
            .or_default()
            .push(row);
    }

    fn read_where(&self, table: &str, col: &str, value: &str) -> Vec<Arc<Cursor>> {
        let g = self.tables.lock().unwrap();
        let Some(rows) = g.get(table) else { return Vec::new() };
        rows.iter()
            .filter(|r| r.get(col).map(|v| v == value).unwrap_or(false))
            .cloned()
            .collect()
    }

    fn rows_of(&self, table: &str) -> Vec<Arc<Cursor>> {
        self.tables
            .lock()
            .unwrap()
            .get(table)
            .cloned()
            .unwrap_or_default()
    }

    fn len(&self, table: &str) -> usize {
        self.tables.lock().unwrap().get(table).map(|v| v.len()).unwrap_or(0)
    }
}

// ───────────────────────────────────────────────────────────────────
// SqliteFactStore — wide tables + strings intern
// ───────────────────────────────────────────────────────────────────

use rusqlite::{params, Connection};
use std::path::Path;

pub struct SqliteFactStore {
    conn:    Mutex<Connection>,
    schemas: Mutex<HashMap<String, Vec<String>>>,
}

fn validate_ident(name: &str) {
    let ok = !name.is_empty()
        && name.chars().next().unwrap().is_ascii_alphabetic()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    assert!(ok, "fact identifier must match [A-Za-z][A-Za-z0-9_]*: {name:?}");
}

impl SqliteFactStore {
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(&conn)?;
        let schemas = Self::load_schemas(&conn)?;
        Ok(Self { conn: Mutex::new(conn), schemas: Mutex::new(schemas) })
    }

    pub fn open_file(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(&conn)?;
        let schemas = Self::load_schemas(&conn)?;
        Ok(Self { conn: Mutex::new(conn), schemas: Mutex::new(schemas) })
    }

    fn init(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sprf_strings (
                id    INTEGER PRIMARY KEY AUTOINCREMENT,
                value TEXT NOT NULL UNIQUE
             );
             CREATE TABLE IF NOT EXISTS sprf_fact_schemas (
                table_name TEXT PRIMARY KEY,
                cols       TEXT NOT NULL
             );",
        )
    }

    fn load_schemas(conn: &Connection) -> rusqlite::Result<HashMap<String, Vec<String>>> {
        let mut stmt = conn.prepare("SELECT table_name, cols FROM sprf_fact_schemas")?;
        let rows = stmt.query_map([], |r| {
            let t: String = r.get(0)?;
            let c: String = r.get(1)?;
            Ok((t, c))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (t, c) = row?;
            let cols: Vec<String> = c.split(',').map(|s| s.to_string()).collect();
            out.insert(t, cols);
        }
        Ok(out)
    }

    fn intern(conn: &Connection, value: &str) -> i64 {
        conn.execute(
            "INSERT OR IGNORE INTO sprf_strings (value) VALUES (?1)",
            params![value],
        ).expect("intern insert");
        conn.query_row(
            "SELECT id FROM sprf_strings WHERE value = ?1",
            params![value],
            |r| r.get::<_, i64>(0),
        ).expect("intern lookup")
    }

    fn select_all_sql(table: &str, cols: &[String], where_clause: Option<&str>) -> String {
        let mut sql = String::from("SELECT ");
        for (i, c) in cols.iter().enumerate() {
            if i > 0 { sql.push_str(", "); }
            sql.push_str(&format!("s_{c}.value"));
        }
        sql.push_str(&format!(" FROM {table}_facts t"));
        for c in cols {
            sql.push_str(&format!(
                " JOIN sprf_strings s_{c} ON s_{c}.id = t.{c}_id"
            ));
        }
        if let Some(w) = where_clause {
            sql.push_str(" WHERE ");
            sql.push_str(w);
        }
        sql
    }
}

impl FactStore for SqliteFactStore {
    fn declare(&self, table: &str, cols: &[&str]) {
        validate_ident(table);
        for c in cols { validate_ident(c); }

        let cols_owned: Vec<String> = cols.iter().map(|s| s.to_string()).collect();

        let mut schemas = self.schemas.lock().unwrap();
        if let Some(existing) = schemas.get(table) {
            assert_eq!(
                existing, &cols_owned,
                "fact table {table:?} re-declared with different columns: \
                 existing={existing:?}, new={cols_owned:?}"
            );
            return;
        }

        let conn = self.conn.lock().unwrap();
        let mut create = format!(
            "CREATE TABLE IF NOT EXISTS {table}_facts (\n\
             \x20  id            INTEGER PRIMARY KEY AUTOINCREMENT,\n\
             \x20  generation_id INTEGER NOT NULL DEFAULT 0"
        );
        for c in &cols_owned {
            create.push_str(&format!(
                ",\n   {c}_id INTEGER NOT NULL REFERENCES sprf_strings(id)"
            ));
        }
        create.push_str("\n)");
        conn.execute(&create, []).expect("create facts table");

        for c in &cols_owned {
            let idx = format!(
                "CREATE INDEX IF NOT EXISTS {table}_facts_{c}_idx ON {table}_facts({c}_id)"
            );
            conn.execute(&idx, []).expect("create index");
        }

        conn.execute(
            "INSERT INTO sprf_fact_schemas (table_name, cols) VALUES (?1, ?2)",
            params![table, cols_owned.join(",")],
        ).expect("schema insert");

        schemas.insert(table.to_string(), cols_owned);
    }

    fn insert(&self, table: &str, row: Arc<Cursor>) {
        let schemas = self.schemas.lock().unwrap();
        let cols = schemas.get(table)
            .unwrap_or_else(|| panic!("insert before declare: {table:?}"))
            .clone();
        drop(schemas);

        let conn = self.conn.lock().unwrap();
        let ids: Vec<i64> = cols.iter()
            .map(|c| Self::intern(&conn, row.get(c).unwrap_or("")))
            .collect();

        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
        let col_list = cols.iter().map(|c| format!("{c}_id")).collect::<Vec<_>>().join(", ");
        let sql = format!(
            "INSERT INTO {table}_facts ({col_list}) VALUES ({})",
            placeholders.join(", ")
        );
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
        conn.execute(&sql, params.as_slice()).expect("fact insert");
    }

    fn read_where(&self, table: &str, col: &str, value: &str) -> Vec<Arc<Cursor>> {
        let schemas = self.schemas.lock().unwrap();
        let cols = match schemas.get(table) { Some(c) => c.clone(), None => return Vec::new() };
        drop(schemas);
        if !cols.iter().any(|c| c == col) { return Vec::new(); }

        let where_clause = format!(
            "t.{col}_id = (SELECT id FROM sprf_strings WHERE value = ?1)"
        );
        let sql = Self::select_all_sql(table, &cols, Some(&where_clause));

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql).expect("prepare read_where");
        let rows = stmt.query_map(params![value], |r| {
            let mut vals = Vec::with_capacity(cols.len());
            for i in 0..cols.len() { vals.push(r.get::<_, String>(i)?); }
            Ok(vals)
        }).expect("query read_where");

        rows.map(|row| {
            let vals = row.expect("row");
            let mut c = Cursor { value: Arc::<str>::from(""), terms: Vec::new() };
            for (name, v) in cols.iter().zip(vals.iter()) { c.set(name, v.as_str()); }
            Arc::new(c)
        }).collect()
    }

    fn rows_of(&self, table: &str) -> Vec<Arc<Cursor>> {
        let schemas = self.schemas.lock().unwrap();
        let cols = match schemas.get(table) { Some(c) => c.clone(), None => return Vec::new() };
        drop(schemas);

        let sql = Self::select_all_sql(table, &cols, None);

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql).expect("prepare rows_of");
        let rows = stmt.query_map([], |r| {
            let mut vals = Vec::with_capacity(cols.len());
            for i in 0..cols.len() { vals.push(r.get::<_, String>(i)?); }
            Ok(vals)
        }).expect("query rows_of");

        rows.map(|row| {
            let vals = row.expect("row");
            let mut c = Cursor { value: Arc::<str>::from(""), terms: Vec::new() };
            for (name, v) in cols.iter().zip(vals.iter()) { c.set(name, v.as_str()); }
            Arc::new(c)
        }).collect()
    }

    fn len(&self, table: &str) -> usize {
        let schemas = self.schemas.lock().unwrap();
        if !schemas.contains_key(table) { return 0; }
        drop(schemas);

        let conn = self.conn.lock().unwrap();
        conn.query_row(
            &format!("SELECT COUNT(*) FROM {table}_facts"),
            [],
            |r| r.get::<_, i64>(0),
        ).map(|n| n as usize).unwrap_or(0)
    }
}

// ───────────────────────────────────────────────────────────────────
// FactWrite / FactRead Components
// ───────────────────────────────────────────────────────────────────

/// `fact(:name) > FactWrite { cols }`. Row-INSERT. Pass-through.
pub struct FactWrite {
    pub store: Arc<dyn FactStore>,
    pub table: Arc<str>,
}

impl FactWrite {
    pub fn new(store: Arc<dyn FactStore>, table: impl Into<Arc<str>>) -> Self {
        Self { store, table: table.into() }
    }
}

impl Component for FactWrite {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.store.insert(&self.table, Arc::new(c.clone()));
        Node::Emit(Arc::new(c.clone()))
    }
}

/// `fact?(:name, KEY, [PROJ...])`. Row-SELECT.
pub struct FactRead {
    pub store:    Arc<dyn FactStore>,
    pub table:    Arc<str>,
    pub key_term: Arc<str>,
    pub project:  Vec<Arc<str>>,
}

impl FactRead {
    pub fn new(
        store: Arc<dyn FactStore>,
        table: impl Into<Arc<str>>,
        key_term: impl Into<Arc<str>>,
        project: &[&str],
    ) -> Self {
        Self {
            store,
            table:    table.into(),
            key_term: key_term.into(),
            project:  project.iter().map(|s| Arc::<str>::from(*s)).collect(),
        }
    }
}

impl Component for FactRead {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let Some(k) = c.get(&self.key_term) else { return Node::Done };
        let matches = self.store.read_where(&self.table, &self.key_term, k);
        if matches.is_empty() { return Node::Done; }

        let children: Vec<Node<Cursor>> = matches
            .iter()
            .map(|row| {
                let mut child = c.clone();
                for col in &self.project {
                    if let Some(v) = row.get(col) {
                        child.set(col, v);
                    }
                }
                Node::Emit(Arc::new(child))
            })
            .collect();
        Node::Many(children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use effect_runtime::v2::{
        expand, ExpandOpts, MemQueue, PipeInstance, QueueBackend,
    };

    struct Collector { sink: Arc<Mutex<Vec<Cursor>>> }
    impl Component for Collector {
        type Next = Cursor;
        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.sink.lock().unwrap().push(c.clone());
            Node::Done
        }
    }

    fn cursor(value: &str, kvs: &[(&str, &str)]) -> Arc<Cursor> {
        let mut c = Cursor { value: value.into(), terms: Vec::new() };
        for (k, v) in kvs { c.set(k, *v); }
        Arc::new(c)
    }

    fn run_insert_and_read_where_suite(store: Arc<dyn FactStore>) {
        store.declare("strings", &["FILE", "HIT"]);
        store.insert("strings", cursor("hi",  &[("FILE", "a.rs"), ("HIT", "hi")]));
        store.insert("strings", cursor("bye", &[("FILE", "a.rs"), ("HIT", "bye")]));
        store.insert("strings", cursor("hi",  &[("FILE", "b.rs"), ("HIT", "hi")]));

        let by_a = store.read_where("strings", "FILE", "a.rs");
        assert_eq!(by_a.len(), 2);
        assert_eq!(store.len("strings"), 3);
    }

    #[test] fn mem_insert_and_read_where()    { run_insert_and_read_where_suite(Arc::new(MemFactStore::new())); }
    #[test] fn sqlite_insert_and_read_where() { run_insert_and_read_where_suite(Arc::new(SqliteFactStore::open_in_memory().unwrap())); }

    #[test]
    fn sqlite_intern_dedups_repeated_strings() {
        let store = SqliteFactStore::open_in_memory().unwrap();
        store.declare("hits", &["FILE", "HIT"]);
        store.insert("hits", cursor("_", &[("FILE", "a.rs"), ("HIT", "hi")]));
        store.insert("hits", cursor("_", &[("FILE", "a.rs"), ("HIT", "bye")]));
        store.insert("hits", cursor("_", &[("FILE", "a.rs"), ("HIT", "hi")]));

        let conn = store.conn.lock().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sprf_strings", [], |r| r.get(0))
            .unwrap();
        // three distinct values: "a.rs", "hi", "bye"
        assert_eq!(n, 3);
    }

    #[test]
    fn sqlite_redeclare_same_cols_is_noop() {
        let store = SqliteFactStore::open_in_memory().unwrap();
        store.declare("hits", &["FILE", "HIT"]);
        store.declare("hits", &["FILE", "HIT"]);
    }

    #[test]
    #[should_panic(expected = "re-declared with different columns")]
    fn sqlite_redeclare_different_cols_panics() {
        let store = SqliteFactStore::open_in_memory().unwrap();
        store.declare("hits", &["FILE", "HIT"]);
        store.declare("hits", &["FILE", "OTHER"]);
    }

    #[test]
    fn fact_write_inserts_and_passes_through() {
        let store: Arc<dyn FactStore> = Arc::new(MemFactStore::new());
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(FactWrite::new(store.clone(), "hits")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe, queue,
            vec![
                cursor("a", &[("FILE", "x.rs")]),
                cursor("b", &[("FILE", "y.rs")]),
            ],
            ExpandOpts::default(),
        );

        assert_eq!(store.len("hits"), 2);
        assert_eq!(sink.lock().unwrap().len(), 2);
    }

    #[test]
    fn fact_read_cross_products_matches_into_input() {
        let store: Arc<dyn FactStore> = Arc::new(MemFactStore::new());
        store.insert("hits", cursor("hi",  &[("FILE", "a.rs"), ("HIT", "hi")]));
        store.insert("hits", cursor("bye", &[("FILE", "a.rs"), ("HIT", "bye")]));
        store.insert("hits", cursor("z",   &[("FILE", "b.rs"), ("HIT", "z")]));

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(FactRead::new(store, "hits", "FILE", &["HIT"]))
                as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe, queue,
            vec![cursor("seed", &[("FILE", "a.rs")])],
            ExpandOpts::default(),
        );

        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 2);
        let mut hits: Vec<&str> = got.iter().filter_map(|c| c.get("HIT")).collect();
        hits.sort();
        assert_eq!(hits, vec!["bye", "hi"]);
    }

    #[test]
    fn fact_read_drops_input_with_no_match() {
        let store: Arc<dyn FactStore> = Arc::new(MemFactStore::new());
        store.insert("hits", cursor("hi", &[("FILE", "a.rs"), ("HIT", "hi")]));

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(FactRead::new(store, "hits", "FILE", &["HIT"]))
                as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe, queue,
            vec![cursor("seed", &[("FILE", "no_match.rs")])],
            ExpandOpts::default(),
        );

        assert_eq!(sink.lock().unwrap().len(), 0);
    }

    #[test]
    fn sqlite_fact_store_persists_across_open() {
        let dir  = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("facts.db");

        // First open: declare + insert.
        {
            let store = SqliteFactStore::open_file(&path).unwrap();
            store.declare("strings", &["FILE", "HIT"]);
            store.insert("strings", cursor("_", &[("FILE", "a.rs"), ("HIT", "hi")]));
            store.insert("strings", cursor("_", &[("FILE", "a.rs"), ("HIT", "bye")]));
        }

        // Second open: schema + rows survive without re-declare.
        {
            let store = SqliteFactStore::open_file(&path).unwrap();
            assert_eq!(store.len("strings"), 2);
            let by_a = store.read_where("strings", "FILE", "a.rs");
            assert_eq!(by_a.len(), 2);
            let mut hits: Vec<&str> = by_a.iter().filter_map(|c| c.get("HIT")).collect();
            hits.sort();
            assert_eq!(hits, vec!["bye", "hi"]);
        }
    }
}
