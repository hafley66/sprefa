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
//! Identity-uniform rows: every fact table carries a synthetic
//! `_id TEXT NOT NULL UNIQUE` column. At insert time, `_id` is minted
//! as `blake3(table || canonical(fields))` and stamped on the row
//! before storage. Same domain across all tables, same dirty-publish
//! key, same future cross-table-join target. Duplicate inserts of the
//! same logical row produce the same id (dedup-friendly: SQLite uses
//! `INSERT OR IGNORE` on the UNIQUE id).
//!
//! Sqlite shape: `declare(table, cols)` emits
//!   CREATE TABLE <table>_facts (
//!       id               INTEGER PRIMARY KEY AUTOINCREMENT,
//!       __generation_id  INTEGER NOT NULL DEFAULT 0,
//!       _id              TEXT NOT NULL UNIQUE,
//!       <c>_id        INTEGER NOT NULL REFERENCES sprf_strings(id),
//!       ...
//!   );
//! per-user-column indexes on `<c>_id`. Insert interns each declared
//! cell into `sprf_strings`; reads JOIN back. Schemas persist in
//! `sprf_fact_schemas` (user-declared cols only; `_id` is implicit).
//!
//! Dirty-publish convention: `commit(gen, bus)` drains pending inserts
//! and publishes `Event::Dirty { domain: "row", key:
//! row_dirty_key(_id) }` per inserted row, plus one
//! `Event::Dirty { domain: "table", key: table_dirty_key(table) }`
//! per changed table. Listeners filter by domain.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use super::event_bus::EventBus;
use super::next_key::NextKey;
use super::row::Row;

/// Synthetic identity column on every fact row. Content-derived hex
/// of `blake3(table || canonical(fields))`. Set by `insert` before
/// the row reaches storage.
pub const ID_COL: &str = "_id";

/// Universal dirty-publish domain. All fact-table commits publish on
/// this single domain; per-row uniqueness comes from the key.
pub const ROW_DOMAIN: &str = "row";

/// Dirty-publish domain for relation/table-level invalidation.
pub const TABLE_DOMAIN: &str = "table";

// ───────────────────────────────────────────────────────────────────
// trait FactStore
// ───────────────────────────────────────────────────────────────────

pub trait FactStore<R: Row>: Send + Sync + 'static {
    /// Declare a table's column shape. Required for SqliteFactStore
    /// before any insert; no-op for stores that auto-discover columns.
    /// Idempotent for matching schemas; panics on schema conflict.
    /// `cols` is the user-declared shape only — `_id` is implicit and
    /// must NOT be included.
    fn declare(&self, _table: &str, _cols: &[&str]) {}

    /// Declared user columns for `table`, excluding `_id`.
    fn declared_cols(&self, _table: &str) -> Option<Vec<String>> { None }

    /// Insert a row into `table`. The store mints `_id =
    /// content_id(table, row)` and stamps it on the row before
    /// storage; rows arriving with `_id` already set are accepted
    /// (re-insert paths) but the stored value is the freshly-computed
    /// content id. May buffer until `commit`.
    fn insert(&self, table: &str, row: Arc<R>);

    /// Bulk insert. Default falls back to per-row `insert`. Stores
    /// override to take ONE lock + ONE transaction for the whole batch
    /// — the difference between O(rows) lock acquisitions and 1.
    fn insert_batch(&self, table: &str, rows: Vec<Arc<R>>) {
        for r in rows { self.insert(table, r); }
    }

    /// Read rows where column `col` equals `value`.
    fn read_where(&self, table: &str, col: &str, value: &str) -> Vec<Arc<R>>;

    /// All rows of `table`.
    fn rows_of(&self, table: &str) -> Vec<Arc<R>>;

    /// Row count for `table`.
    fn len(&self, table: &str) -> usize;

    /// Monotonic table change token. Used by query caches to avoid
    /// hashing whole relation contents on every batch. Stores should
    /// bump this when accepted inserts or deletes change visible rows.
    fn table_version(&self, _table: &str) -> u64 { 0 }

    /// Delete rows from `table` where every `(col, value)` predicate
    /// matches. Returns the number of removed rows. Stores that do not
    /// support deletion can keep the default no-op.
    fn delete_matching(&self, _table: &str, _predicates: &[(&str, &str)]) -> usize { 0 }

    /// Drain any buffered writes and (if `bus` is provided) publish
    /// dirty events for each newly-inserted row, keyed by the FIRST
    /// declared column of the table.
    ///
    /// Default impl publishes nothing — stores that buffer (sqlite)
    /// override.
    fn commit(&self, _gen: u64, _bus: Option<&EventBus>) {}
}

/// Per-row dirty key. `H(_id_hex)`. Same value any consumer can
/// derive given the row id.
pub fn row_dirty_key(id: &str) -> NextKey {
    NextKey(*blake3::hash(id.as_bytes()).as_bytes())
}

/// Per-table dirty key. `H(table_name)`.
pub fn table_dirty_key(table: &str) -> NextKey {
    NextKey(*blake3::hash(table.as_bytes()).as_bytes())
}

/// Compute the content-derived `_id` for a row destined for `table`.
/// Canonical form: collect `row.fields()` into a Vec, sort by key,
/// concatenate `k || \0 || v || \0 || ...`. Stable across field
/// insertion orderings.
pub fn content_id<R: Row>(table: &str, row: &R) -> String {
    let mut pairs: Vec<(&str, &str)> = row.fields()
        .into_iter()
        .filter(|(k, _)| *k != ID_COL)
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let mut h = blake3::Hasher::new();
    h.update(table.as_bytes());
    h.update(b"\0");
    for (k, v) in &pairs {
        h.update(k.as_bytes());
        h.update(b"\0");
        h.update(v.as_bytes());
        h.update(b"\0");
    }
    h.finalize().to_hex().to_string()
}

// ───────────────────────────────────────────────────────────────────
// MemFactStore
// ───────────────────────────────────────────────────────────────────

pub struct MemFactStore<R: Row> {
    tables:  Mutex<HashMap<String, MemTable<R>>>,
    schemas: Mutex<HashMap<String, Vec<String>>>,
    /// Pending inserts (table, row) since the last commit. Consumed by
    /// `commit` to publish dirty events.
    pending: Mutex<Vec<(String, Arc<R>)>>,
    dirty_tables: Mutex<HashSet<String>>,
    table_versions: Mutex<HashMap<String, u64>>,
}

/// In-memory bucket for one table. Carries an `_id`-keyed HashSet
/// alongside the row Vec so insert dedup is O(1) instead of O(n).
struct MemTable<R: Row> {
    rows:    Vec<Arc<R>>,
    seen:    HashSet<String>,
    _phant:  std::marker::PhantomData<R>,
}
impl<R: Row> Default for MemTable<R> {
    fn default() -> Self {
        Self { rows: Vec::new(), seen: HashSet::new(), _phant: std::marker::PhantomData }
    }
}

impl<R: Row> Default for MemFactStore<R> {
    fn default() -> Self {
        Self {
            tables:  Mutex::new(HashMap::new()),
            schemas: Mutex::new(HashMap::new()),
            pending: Mutex::new(Vec::new()),
            dirty_tables: Mutex::new(HashSet::new()),
            table_versions: Mutex::new(HashMap::new()),
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

    fn declared_cols(&self, table: &str) -> Option<Vec<String>> {
        self.schemas.lock().unwrap().get(table).cloned()
    }

    fn insert(&self, table: &str, row: Arc<R>) {
        let mut owned: R = Arc::unwrap_or_clone(row);
        let id = content_id(table, &owned);
        owned.set(ID_COL, &id);
        let arced = Arc::new(owned);
        let mut tables = self.tables.lock().unwrap();
        let bucket = tables.entry(table.to_string()).or_default();
        if !bucket.seen.insert(id) { return; } // O(1) dedup
        bucket.rows.push(arced.clone());
        drop(tables);
        bump_table_version(&self.table_versions, table);
        self.pending.lock().unwrap().push((table.to_string(), arced));
        self.dirty_tables.lock().unwrap().insert(table.to_string());
    }

    fn insert_batch(&self, table: &str, rows: Vec<Arc<R>>) {
        if rows.is_empty() { return; }
        // ID minting + content_id is per-row; do it OUTSIDE the lock.
        let mut prepared: Vec<(String, Arc<R>)> = Vec::with_capacity(rows.len());
        for row in rows {
            let mut owned: R = Arc::unwrap_or_clone(row);
            let id = content_id(table, &owned);
            owned.set(ID_COL, &id);
            prepared.push((id, Arc::new(owned)));
        }
        // ONE lock for the whole batch.
        let mut tables = self.tables.lock().unwrap();
        let bucket = tables.entry(table.to_string()).or_default();
        bucket.rows.reserve(prepared.len());
        let mut accepted: Vec<Arc<R>> = Vec::with_capacity(prepared.len());
        for (id, row) in prepared {
            if bucket.seen.insert(id) {
                bucket.rows.push(row.clone());
                accepted.push(row);
            }
        }
        drop(tables);
        if !accepted.is_empty() {
            bump_table_version(&self.table_versions, table);
            let mut pending = self.pending.lock().unwrap();
            pending.reserve(accepted.len());
            for row in accepted { pending.push((table.to_string(), row)); }
            self.dirty_tables.lock().unwrap().insert(table.to_string());
        }
    }

    fn read_where(&self, table: &str, col: &str, value: &str) -> Vec<Arc<R>> {
        let g = self.tables.lock().unwrap();
        let Some(b) = g.get(table) else { return Vec::new() };
        b.rows.iter()
            .filter(|r| r.get(col).map(|v| v == value).unwrap_or(false))
            .cloned()
            .collect()
    }

    fn rows_of(&self, table: &str) -> Vec<Arc<R>> {
        self.tables.lock().unwrap()
            .get(table)
            .map(|b| b.rows.clone())
            .unwrap_or_default()
    }

    fn len(&self, table: &str) -> usize {
        self.tables.lock().unwrap().get(table).map(|b| b.rows.len()).unwrap_or(0)
    }

    fn table_version(&self, table: &str) -> u64 {
        *self.table_versions.lock().unwrap().get(table).unwrap_or(&0)
    }

    fn delete_matching(&self, table: &str, predicates: &[(&str, &str)]) -> usize {
        if predicates.is_empty() { return 0; }

        let mut removed_ids = HashSet::new();
        let mut tables = self.tables.lock().unwrap();
        let Some(bucket) = tables.get_mut(table) else { return 0 };
        let before = bucket.rows.len();
        bucket.rows.retain(|row| {
            let matched = predicates.iter().all(|(col, value)| {
                row.get(col).map(|got| got == *value).unwrap_or(false)
            });
            if matched {
                if let Some(id) = row.get(ID_COL) {
                    removed_ids.insert(id.to_string());
                } else {
                    removed_ids.insert(content_id(table, row.as_ref()));
                }
            }
            !matched
        });
        for id in &removed_ids {
            bucket.seen.remove(id);
        }
        let removed = before - bucket.rows.len();
        drop(tables);

        if removed > 0 {
            bump_table_version(&self.table_versions, table);
            self.pending.lock().unwrap().retain(|(pending_table, row)| {
                if pending_table != table {
                    return true;
                }
                let id = row.get(ID_COL)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| content_id(table, row.as_ref()));
                !removed_ids.contains(&id)
            });
            self.dirty_tables.lock().unwrap().insert(table.to_string());
        }

        removed
    }

    fn commit(&self, _gen: u64, bus: Option<&EventBus>) {
        let drained: Vec<(String, Arc<R>)> = std::mem::take(&mut *self.pending.lock().unwrap());
        let dirty_tables: HashSet<String> =
            std::mem::take(&mut *self.dirty_tables.lock().unwrap());
        let Some(bus) = bus else { return };
        for (_table, row) in &drained {
            let Some(id) = row.get(ID_COL) else { continue };
            bus.dispatch_dirty(ROW_DOMAIN, Some(row_dirty_key(id)));
        }
        for table in dirty_tables {
            bus.dispatch_dirty(TABLE_DOMAIN, Some(table_dirty_key(&table)));
        }
    }
}

fn bump_table_version(versions: &Mutex<HashMap<String, u64>>, table: &str) {
    let mut versions = versions.lock().unwrap();
    let version = versions.entry(table.to_string()).or_insert(0);
    *version = version.saturating_add(1);
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
            && name.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        assert!(ok, "fact table must match [A-Za-z_][A-Za-z0-9_]*: {name:?}");
    }

    /// Column identifier: same as table, plus a leading `:` is allowed
    /// to distinguish internal terms from user captures (sprefa naming).
    /// The `:` is stripped for the on-disk column name; lookups still
    /// happen by the original `:foo` key against the in-memory schema.
    fn validate_col(name: &str) {
        let stripped = name.strip_prefix(':').unwrap_or(name);
        let ok = !stripped.is_empty()
            && stripped.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
            && stripped.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        assert!(ok, "fact column must match :?[A-Za-z_][A-Za-z0-9_]*: {name:?}");
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
        pub(crate) dirty_tables: Mutex<HashSet<String>>,
        pub(crate) table_versions: Mutex<HashMap<String, u64>>,
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
                dirty_tables: Mutex::new(HashSet::new()),
                table_versions: Mutex::new(HashMap::new()),
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
                dirty_tables: Mutex::new(HashSet::new()),
                table_versions: Mutex::new(HashMap::new()),
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
                 \x20  id              INTEGER PRIMARY KEY AUTOINCREMENT,\n\
                 \x20  __generation_id INTEGER NOT NULL DEFAULT 0,\n\
                 \x20  _id             TEXT NOT NULL UNIQUE"
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

        fn declared_cols(&self, table: &str) -> Option<Vec<String>> {
            self.schemas.lock().unwrap().get(table).cloned()
        }

        fn insert(&self, table: &str, row: Arc<R>) {
            let schemas = self.schemas.lock().unwrap();
            let cols = schemas.get(table)
                .unwrap_or_else(|| panic!("insert before declare: {table:?}"))
                .clone();
            drop(schemas);

            let mut owned: R = Arc::unwrap_or_clone(row);
            let id_hex = content_id(table, &owned);
            owned.set(ID_COL, &id_hex);

            let conn = self.conn.lock().unwrap();
            let ids: Vec<i64> = cols.iter()
                .map(|c| Self::intern(&conn, owned.get(c).unwrap_or("")))
                .collect();

            // _id is bound as ?1, user-col interned ids follow.
            let mut placeholders: Vec<String> = vec!["?1".to_string()];
            for i in 0..ids.len() {
                placeholders.push(format!("?{}", i + 2));
            }
            let mut col_list = String::from("_id");
            for c in &cols {
                col_list.push_str(", ");
                col_list.push_str(&format!("{}_id", col_sql(c)));
            }
            let sql = format!(
                "INSERT OR IGNORE INTO {table}_facts ({col_list}) VALUES ({})",
                placeholders.join(", ")
            );
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + ids.len());
            params.push(&id_hex as &dyn rusqlite::ToSql);
            for i in &ids { params.push(i as &dyn rusqlite::ToSql); }
            let changed = conn.execute(&sql, params.as_slice()).expect("fact insert");
            drop(conn);

            // INSERT OR IGNORE: 0 rows changed = duplicate content,
            // do not republish dirty.
            if changed == 0 { return; }

            bump_table_version(&self.table_versions, table);
            self.pending.lock().unwrap().push((table.to_string(), Arc::new(owned)));
            self.dirty_tables.lock().unwrap().insert(table.to_string());
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

        fn table_version(&self, table: &str) -> u64 {
            *self.table_versions.lock().unwrap().get(table).unwrap_or(&0)
        }

        fn delete_matching(&self, table: &str, predicates: &[(&str, &str)]) -> usize {
            if predicates.is_empty() { return 0; }

            let schemas = self.schemas.lock().unwrap();
            let cols = match schemas.get(table) { Some(c) => c.clone(), None => return 0 };
            drop(schemas);

            let mut clauses = Vec::with_capacity(predicates.len());
            let mut values = Vec::with_capacity(predicates.len());
            for (idx, (col, value)) in predicates.iter().enumerate() {
                if *col == ID_COL {
                    clauses.push(format!("_id = ?{}", idx + 1));
                    values.push((*value).to_string());
                    continue;
                }
                if !cols.iter().any(|c| c == col) {
                    return 0;
                }
                clauses.push(format!(
                    "{}_id = (SELECT id FROM sprf_strings WHERE value = ?{})",
                    col_sql(col),
                    idx + 1,
                ));
                values.push((*value).to_string());
            }

            let sql = format!(
                "DELETE FROM {table}_facts WHERE {}",
                clauses.join(" AND "),
            );
            let conn = self.conn.lock().unwrap();
            let params: Vec<&dyn rusqlite::ToSql> = values
                .iter()
                .map(|v| v as &dyn rusqlite::ToSql)
                .collect();
            let changed = conn.execute(&sql, params.as_slice()).expect("fact delete");
            drop(conn);

            if changed > 0 {
                bump_table_version(&self.table_versions, table);
                self.pending.lock().unwrap().retain(|(pending_table, row)| {
                    if pending_table != table {
                        return true;
                    }
                    !predicates.iter().all(|(col, value)| {
                        row.get(col).map(|got| got == *value).unwrap_or(false)
                    })
                });
                self.dirty_tables.lock().unwrap().insert(table.to_string());
            }

            changed
        }

        fn commit(&self, _gen: u64, bus: Option<&EventBus>) {
            let drained: Vec<(String, Arc<R>)> =
                std::mem::take(&mut *self.pending.lock().unwrap());
            let dirty_tables: HashSet<String> =
                std::mem::take(&mut *self.dirty_tables.lock().unwrap());
            let Some(bus) = bus else { return };
            for (_table, row) in &drained {
                let Some(id) = row.get(ID_COL) else { continue };
                bus.dispatch_dirty(ROW_DOMAIN, Some(row_dirty_key(id)));
            }
            for table in dirty_tables {
                bus.dispatch_dirty(TABLE_DOMAIN, Some(table_dirty_key(&table)));
            }
        }
    }
}

#[cfg(feature = "sqlite")]
pub use sqlite::SqliteFactStore;
