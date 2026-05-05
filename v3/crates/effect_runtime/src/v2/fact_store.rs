//! `FactStore<R: Row>` — wide-row table store, the canonical state
//! backbone for the v2 runtime.
//!
//! Replaces the (key, value) `MutationStore<T>`: a MutationStore is a
//! degenerate FactStore (1-col, 1-row-per-key). Async-effect writeback
//! becomes `store.insert("<table>", row)` followed by either
//! `queue.dispatch_park(...)` (fast path) or `bus.dispatch_dirty(...)`
//! (cache-fan-out path).
//!
//!   trait FactStore<R>        the surface FactWrite/FactRead consume
//!   MemFactStore<R>           in-memory; HashMap<table, Vec<row>>
//!   SqliteFactStore<R>        sqlite-backed; per-table wide schema
//!                             with FK columns into shared sprf_strings
//!
//! Sqlite shape: `declare(table, cols)` emits
//!   CREATE TABLE <table>_facts (
//!       id            INTEGER PRIMARY KEY AUTOINCREMENT,
//!       generation_id INTEGER NOT NULL DEFAULT 0,
//!       <c>_id        INTEGER NOT NULL REFERENCES sprf_strings(id),
//!       ...
//!   );
//! per-column indexes on `<c>_id`. Insert interns each declared cell
//! into `sprf_strings`; reads JOIN back. Schemas persist in
//! `sprf_fact_schemas`.
//!
//! Dirty-publish convention: `commit(gen, bus)` drains pending inserts
//! and publishes `Event::Dirty { domain: "fact:<table>", key: H(<table>
//! || <key_col> || <key_val>) }` for the FIRST declared column of each
//! row. Listeners filter by domain prefix; the key matches anything
//! parking on the same first-col value.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::event_bus::EventBus;
use super::next_key::NextKey;
use super::row::Row;

// ───────────────────────────────────────────────────────────────────
// trait FactStore
// ───────────────────────────────────────────────────────────────────

pub trait FactStore<R: Row>: Send + Sync + 'static {
    /// Declare a table's column shape. Required for SqliteFactStore
    /// before any insert; no-op for stores that auto-discover columns.
    /// Idempotent for matching schemas; panics on schema conflict.
    fn declare(&self, _table: &str, _cols: &[&str]) {}

    /// Insert a row into `table`. May buffer until `commit`.
    fn insert(&self, table: &str, row: Arc<R>);

    /// Read rows where column `col` equals `value`.
    fn read_where(&self, table: &str, col: &str, value: &str) -> Vec<Arc<R>>;

    /// All rows of `table`.
    fn rows_of(&self, table: &str) -> Vec<Arc<R>>;

    /// Row count for `table`.
    fn len(&self, table: &str) -> usize;

    /// Drain any buffered writes and (if `bus` is provided) publish
    /// dirty events for each newly-inserted row, keyed by the FIRST
    /// declared column of the table.
    ///
    /// Default impl publishes nothing — stores that buffer (sqlite)
    /// override.
    fn commit(&self, _gen: u64, _bus: Option<&EventBus>) {}
}

/// `domain` for a fact-table dirty event.
pub fn fact_domain(table: &str) -> String {
    format!("fact:{table}")
}

/// `key` for a fact-table dirty event: H(table || col || value).
pub fn fact_dirty_key(table: &str, col: &str, value: &str) -> NextKey {
    let mut h = blake3::Hasher::new();
    h.update(table.as_bytes());
    h.update(b"\0");
    h.update(col.as_bytes());
    h.update(b"\0");
    h.update(value.as_bytes());
    NextKey(*h.finalize().as_bytes())
}

// ───────────────────────────────────────────────────────────────────
// MemFactStore
// ───────────────────────────────────────────────────────────────────

pub struct MemFactStore<R: Row> {
    tables:  Mutex<HashMap<String, Vec<Arc<R>>>>,
    schemas: Mutex<HashMap<String, Vec<String>>>,
    /// Pending inserts (table, row) since the last commit. Consumed by
    /// `commit` to publish dirty events.
    pending: Mutex<Vec<(String, Arc<R>)>>,
}

impl<R: Row> Default for MemFactStore<R> {
    fn default() -> Self {
        Self {
            tables:  Mutex::new(HashMap::new()),
            schemas: Mutex::new(HashMap::new()),
            pending: Mutex::new(Vec::new()),
        }
    }
}

impl<R: Row> MemFactStore<R> {
    pub fn new() -> Self { Self::default() }
}

impl<R: Row> FactStore<R> for MemFactStore<R> {
    fn declare(&self, table: &str, cols: &[&str]) {
        let cols_owned: Vec<String> = cols.iter().map(|s| s.to_string()).collect();
        let mut s = self.schemas.lock().unwrap();
        if let Some(existing) = s.get(table) {
            assert_eq!(
                existing, &cols_owned,
                "fact table {table:?} re-declared with different columns: \
                 existing={existing:?}, new={cols_owned:?}"
            );
            return;
        }
        s.insert(table.to_string(), cols_owned);
    }

    fn insert(&self, table: &str, row: Arc<R>) {
        self.tables
            .lock()
            .unwrap()
            .entry(table.to_string())
            .or_default()
            .push(row.clone());
        self.pending.lock().unwrap().push((table.to_string(), row));
    }

    fn read_where(&self, table: &str, col: &str, value: &str) -> Vec<Arc<R>> {
        let g = self.tables.lock().unwrap();
        let Some(rows) = g.get(table) else { return Vec::new() };
        rows.iter()
            .filter(|r| r.get(col).map(|v| v == value).unwrap_or(false))
            .cloned()
            .collect()
    }

    fn rows_of(&self, table: &str) -> Vec<Arc<R>> {
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

    fn commit(&self, _gen: u64, bus: Option<&EventBus>) {
        let drained: Vec<(String, Arc<R>)> = std::mem::take(&mut *self.pending.lock().unwrap());
        let Some(bus) = bus else { return };
        let schemas = self.schemas.lock().unwrap();
        for (table, row) in &drained {
            let Some(cols) = schemas.get(table) else { continue };
            let Some(key_col) = cols.first() else { continue };
            let Some(val) = row.get(key_col) else { continue };
            let key = fact_dirty_key(table, key_col, val);
            bus.dispatch_dirty(fact_domain(table), Some(key));
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// SqliteFactStore — wide tables + strings intern
// ───────────────────────────────────────────────────────────────────

#[cfg(feature = "sqlite")]
mod sqlite {
    use super::*;
    use std::marker::PhantomData;
    use std::path::Path;
    use rusqlite::{params, Connection};

    fn validate_table(name: &str) {
        let ok = !name.is_empty()
            && name.chars().next().unwrap().is_ascii_alphabetic()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        assert!(ok, "fact table must match [A-Za-z][A-Za-z0-9_]*: {name:?}");
    }

    /// Column identifier: same as table, plus a leading `:` is allowed
    /// to distinguish internal terms from user captures (sprefa naming).
    /// The `:` is stripped for the on-disk column name; lookups still
    /// happen by the original `:foo` key against the in-memory schema.
    fn validate_col(name: &str) {
        let stripped = name.strip_prefix(':').unwrap_or(name);
        let ok = !stripped.is_empty()
            && stripped.chars().next().unwrap().is_ascii_alphabetic()
            && stripped.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        assert!(ok, "fact column must match :?[A-Za-z][A-Za-z0-9_]*: {name:?}");
    }

    /// On-disk column safe form (sqlite identifier). The schema map
    /// remembers the original name; SQL strings use this. We map by
    /// stripping the leading colon — for the current set of column
    /// names this is unambiguous.
    fn col_sql(name: &str) -> &str { name.strip_prefix(':').unwrap_or(name) }

    pub struct SqliteFactStore<R: Row> {
        pub(crate) conn:    Mutex<Connection>,
        pub(crate) schemas: Mutex<HashMap<String, Vec<String>>>,
        /// Pending (table, row) accumulated since last commit. Drained
        /// by `commit` to publish dirty events. Sqlite writes happen
        /// inline at `insert` time today — buffering for transaction
        /// batching is a future change.
        pub(crate) pending: Mutex<Vec<(String, Arc<R>)>>,
        _marker: PhantomData<fn() -> R>,
    }

    impl<R: Row> SqliteFactStore<R> {
        pub fn open_in_memory() -> rusqlite::Result<Self> {
            let conn = Connection::open_in_memory()?;
            Self::init(&conn)?;
            let schemas = Self::load_schemas(&conn)?;
            Ok(Self {
                conn: Mutex::new(conn),
                schemas: Mutex::new(schemas),
                pending: Mutex::new(Vec::new()),
                _marker: PhantomData,
            })
        }

        pub fn open_file(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
            let conn = Connection::open(path)?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            Self::init(&conn)?;
            let schemas = Self::load_schemas(&conn)?;
            Ok(Self {
                conn: Mutex::new(conn),
                schemas: Mutex::new(schemas),
                pending: Mutex::new(Vec::new()),
                _marker: PhantomData,
            })
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
                let cs = col_sql(c);
                if i > 0 { sql.push_str(", "); }
                sql.push_str(&format!("s_{cs}.value"));
            }
            sql.push_str(&format!(" FROM {table}_facts t"));
            for c in cols {
                let cs = col_sql(c);
                sql.push_str(&format!(
                    " JOIN sprf_strings s_{cs} ON s_{cs}.id = t.{cs}_id"
                ));
            }
            if let Some(w) = where_clause {
                sql.push_str(" WHERE ");
                sql.push_str(w);
            }
            sql
        }
    }

    impl<R: Row> FactStore<R> for SqliteFactStore<R> {
        fn declare(&self, table: &str, cols: &[&str]) {
            validate_table(table);
            for c in cols { validate_col(c); }

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
                let cs = col_sql(c);
                create.push_str(&format!(
                    ",\n   {cs}_id INTEGER NOT NULL REFERENCES sprf_strings(id)"
                ));
            }
            create.push_str("\n)");
            conn.execute(&create, []).expect("create facts table");

            for c in &cols_owned {
                let cs = col_sql(c);
                let idx = format!(
                    "CREATE INDEX IF NOT EXISTS {table}_facts_{cs}_idx ON {table}_facts({cs}_id)"
                );
                conn.execute(&idx, []).expect("create index");
            }

            conn.execute(
                "INSERT INTO sprf_fact_schemas (table_name, cols) VALUES (?1, ?2)",
                params![table, cols_owned.join(",")],
            ).expect("schema insert");

            schemas.insert(table.to_string(), cols_owned);
        }

        fn insert(&self, table: &str, row: Arc<R>) {
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
            let col_list = cols.iter().map(|c| format!("{}_id", col_sql(c))).collect::<Vec<_>>().join(", ");
            let sql = format!(
                "INSERT INTO {table}_facts ({col_list}) VALUES ({})",
                placeholders.join(", ")
            );
            let params: Vec<&dyn rusqlite::ToSql> =
                ids.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
            conn.execute(&sql, params.as_slice()).expect("fact insert");
            drop(conn);

            self.pending.lock().unwrap().push((table.to_string(), row));
        }

        fn read_where(&self, table: &str, col: &str, value: &str) -> Vec<Arc<R>> {
            let schemas = self.schemas.lock().unwrap();
            let cols = match schemas.get(table) { Some(c) => c.clone(), None => return Vec::new() };
            drop(schemas);
            if !cols.iter().any(|c| c == col) { return Vec::new(); }

            let where_clause = format!(
                "t.{}_id = (SELECT id FROM sprf_strings WHERE value = ?1)",
                col_sql(col)
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
                let mut c = R::default();
                for (name, v) in cols.iter().zip(vals.iter()) { c.set(name, v.as_str()); }
                Arc::new(c)
            }).collect()
        }

        fn rows_of(&self, table: &str) -> Vec<Arc<R>> {
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
                let mut c = R::default();
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

        fn commit(&self, _gen: u64, bus: Option<&EventBus>) {
            let drained: Vec<(String, Arc<R>)> =
                std::mem::take(&mut *self.pending.lock().unwrap());
            let Some(bus) = bus else { return };
            let schemas = self.schemas.lock().unwrap();
            for (table, row) in &drained {
                let Some(cols) = schemas.get(table) else { continue };
                let Some(key_col) = cols.first() else { continue };
                let Some(val) = row.get(key_col) else { continue };
                let key = fact_dirty_key(table, key_col, val);
                bus.dispatch_dirty(fact_domain(table), Some(key));
            }
        }
    }
}

#[cfg(feature = "sqlite")]
pub use sqlite::SqliteFactStore;
