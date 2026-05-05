//! `FactStore` — wide-row table store, trait + two impls.
//!
//!   trait FactStore           the surface FactWrite/FactRead consume
//!   MemFactStore              in-memory; HashMap<table, Vec<row>>
//!   SqliteFactStore           sqlite-backed; rows as blobs (cursor_codec)
//!
//! Wide-row table abstraction. Each table is a `Vec<Arc<Cursor>>`;
//! reads scan linearly. Indexing-by-column lands when reactivity
//! grows fine-grain key-reconcile semantics. Today it's brutish.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{Component, Node, RenderCtx};

use crate::Cursor;
use crate::cursor_codec;

pub trait FactStore: Send + Sync + 'static {
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
// SqliteFactStore
// ───────────────────────────────────────────────────────────────────
//
// Schema: one table for everything. Rows are cursor_codec blobs;
// table membership is the `tab` column. Reads decode + filter
// row-side, no per-column index. Persistable across process restart.
//
//   CREATE TABLE sprf_facts (
//       id   INTEGER PRIMARY KEY AUTOINCREMENT,
//       tab  TEXT NOT NULL,
//       blob BLOB NOT NULL
//   );
//   CREATE INDEX sprf_facts_tab ON sprf_facts(tab);

use rusqlite::{params, Connection};
use std::path::Path;

pub struct SqliteFactStore {
    conn: Mutex<Connection>,
}

impl SqliteFactStore {
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn open_file(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn init(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sprf_facts (
                id   INTEGER PRIMARY KEY AUTOINCREMENT,
                tab  TEXT NOT NULL,
                blob BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS sprf_facts_tab ON sprf_facts(tab);",
        )
    }
}

impl FactStore for SqliteFactStore {
    fn insert(&self, table: &str, row: Arc<Cursor>) {
        let blob = cursor_codec::encode(&row);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sprf_facts (tab, blob) VALUES (?1, ?2)",
            params![table, blob],
        )
        .expect("sqlite insert");
    }

    fn read_where(&self, table: &str, col: &str, value: &str) -> Vec<Arc<Cursor>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT blob FROM sprf_facts WHERE tab = ?1")
            .expect("sqlite prepare");
        let rows = stmt
            .query_map(params![table], |r| {
                let blob: Vec<u8> = r.get(0)?;
                Ok(blob)
            })
            .expect("sqlite query");
        let mut out = Vec::new();
        for r in rows {
            let blob = r.expect("row");
            let cursor = cursor_codec::decode(&blob).expect("decode");
            if cursor.get(col).map(|v| v == value).unwrap_or(false) {
                out.push(Arc::new(cursor));
            }
        }
        out
    }

    fn rows_of(&self, table: &str) -> Vec<Arc<Cursor>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT blob FROM sprf_facts WHERE tab = ?1")
            .expect("sqlite prepare");
        let rows = stmt
            .query_map(params![table], |r| {
                let blob: Vec<u8> = r.get(0)?;
                Ok(blob)
            })
            .expect("sqlite query");
        rows.map(|r| Arc::new(cursor_codec::decode(&r.expect("row")).expect("decode")))
            .collect()
    }

    fn len(&self, table: &str) -> usize {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM sprf_facts WHERE tab = ?1",
            params![table],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n as usize)
        .unwrap_or(0)
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

        // First open: insert.
        {
            let store = SqliteFactStore::open_file(&path).unwrap();
            store.insert("strings", cursor("hi", &[("FILE", "a.rs"), ("HIT", "hi")]));
            store.insert("strings", cursor("bye", &[("FILE", "a.rs"), ("HIT", "bye")]));
        }

        // Second open: rows survive.
        {
            let store = SqliteFactStore::open_file(&path).unwrap();
            assert_eq!(store.len("strings"), 2);
            let by_a = store.read_where("strings", "FILE", "a.rs");
            assert_eq!(by_a.len(), 2);
        }
    }
}
