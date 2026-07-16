//! Counted relational-storage seam.
//!
//! `Storage` is deliberately shaped like the small DB-API surface the engine
//! already uses. The `Db` implementation delegates to its existing counted and
//! plural methods, preserving SQLite behavior while giving callers a backend
//! boundary that does not expose the raw database connection.
//!
use anyhow::Result;

use crate::ast::Value;
use crate::db::Db;
use crate::spine::SymSink;

pub(crate) mod call;

pub trait Storage {
    /// Execute one parameterless statement through the counted database seam.
    fn execute(&self, sql: &str) -> Result<usize>;

    /// Execute a multi-statement script through the counted database seam.
    fn execute_batch(&self, sql: &str) -> Result<()>;

    /// Insert rows through the existing chunked, plural write path.
    fn insert_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize>;

    /// Replace a relation through the existing index-aware reload path.
    fn reload_rel(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize>;

    /// Retract specific rows by full-tuple identity in ONE statement (row-value
    /// `DELETE ... WHERE (cols) IN (VALUES ...)`), never a per-row loop. The
    /// insert twin of the reconcile/render step: applying a `RowDelta`'s
    /// retracted set incrementally instead of overwriting the whole relation.
    fn retract_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize>;

    /// Return the existing SQLite-backed structural and size statistics.
    fn rel_stats(&self, rel: &str) -> Result<serde_json::Value>;

    /// Flush one explicitly collected symbol batch.
    fn flush_syms(&self, sink: &mut SymSink) -> Result<usize>;

    /// Flush symbols queued by SQLite scalar functions.
    fn flush_pending_syms(&self) -> Result<usize>;

    /// Reset and inspect the existing per-tick counted-statement guard.
    fn tick_begin(&self);
    fn tick_end(&self) -> Option<(String, u32)>;

    fn is_autocommit(&self) -> bool;
    fn begin(&self) -> Result<()>;
    fn begin_immediate(&self) -> Result<()>;
    fn commit(&self) -> Result<()>;
    fn rollback(&self) -> Result<()>;
}

impl Storage for Db {
    fn execute(&self, sql: &str) -> Result<usize> {
        Db::exec(self, sql)
    }

    fn execute_batch(&self, sql: &str) -> Result<()> {
        Db::execute_batch(self, sql)
    }

    fn insert_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        Db::insert_rows(self, table, cols, rows)
    }

    fn reload_rel(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        Db::reload_rel(self, table, cols, rows)
    }

    fn retract_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let col_tuple = cols.join(", ");
        let one = format!("({})", vec!["?"; cols.len()].join(", "));
        let values = vec![one; rows.len()].join(", ");
        let sql = format!("DELETE FROM {table} WHERE ({col_tuple}) IN (VALUES {values})");
        let params: Vec<rusqlite::types::Value> = rows
            .iter()
            .flatten()
            .map(|cell| match cell {
                Value::Text(s) => rusqlite::types::Value::Text(s.clone()),
                Value::Int(n) => rusqlite::types::Value::Integer(*n),
                Value::Null => rusqlite::types::Value::Null,
            })
            .collect();
        let conn = self.conn();
        let mut stmt = conn.prepare(&sql)?;
        Ok(stmt.execute(rusqlite::params_from_iter(params))?)
    }

    fn rel_stats(&self, rel: &str) -> Result<serde_json::Value> {
        Db::rel_stats(self, rel)
    }

    fn flush_syms(&self, sink: &mut SymSink) -> Result<usize> {
        Db::flush_syms(self, sink)
    }

    fn flush_pending_syms(&self) -> Result<usize> {
        Db::flush_pending_syms(self)
    }

    fn tick_begin(&self) {
        Db::tick_begin(self)
    }

    fn tick_end(&self) -> Option<(String, u32)> {
        Db::tick_end(self)
    }

    fn is_autocommit(&self) -> bool {
        Db::is_autocommit(self)
    }

    fn begin(&self) -> Result<()> {
        Db::execute_batch(self, "BEGIN")
    }

    fn begin_immediate(&self) -> Result<()> {
        Db::execute_batch(self, "BEGIN IMMEDIATE")
    }

    fn commit(&self) -> Result<()> {
        Db::execute_batch(self, "COMMIT")
    }

    fn rollback(&self) -> Result<()> {
        Db::execute_batch(self, "ROLLBACK")
    }
}

#[cfg(test)]
mod tests {
    use super::Storage;
    use crate::ast::Value;
    use crate::{db, spine::SymSink};

    #[test]
    fn db_storage_delegates_bulk_reload_and_stats() {
        let db = db::open(None).unwrap();
        Storage::execute_batch(
            &db,
            "CREATE TABLE rel_storage_trait (id INTEGER PRIMARY KEY, value TEXT); \
             CREATE INDEX rel_storage_trait_value ON rel_storage_trait(value);",
        ).unwrap();

        let initial = vec![
            vec![Value::Int(1), Value::Text("one".into())],
            vec![Value::Int(2), Value::Text("two".into())],
        ];
        assert_eq!(Storage::insert_rows(
            &db, "rel_storage_trait", &["id", "value"], &initial,
        ).unwrap(), 2);

        let initial_stats = Storage::rel_stats(&db, "storage_trait").unwrap();
        assert_eq!(initial_stats["rows"], 2);

        let replacement = vec![vec![Value::Int(3), Value::Text("three".into())]];
        assert_eq!(Storage::reload_rel(
            &db, "rel_storage_trait", &["id", "value"], &replacement,
        ).unwrap(), 1);
        let stats = Storage::rel_stats(&db, "storage_trait").unwrap();
        assert_eq!(stats["rows"], 1);
        assert_eq!(stats["indexes"], serde_json::json!(["rel_storage_trait_value"]));
    }

    #[test]
    fn db_storage_transactions_preserve_caller_ownership() {
        let db = db::open(None).unwrap();
        Storage::execute(&db, "CREATE TABLE storage_tx (value INTEGER)").unwrap();
        assert!(Storage::is_autocommit(&db));

        Storage::begin_immediate(&db).unwrap();
        assert!(!Storage::is_autocommit(&db));
        Storage::execute(&db, "INSERT INTO storage_tx VALUES (1)").unwrap();
        Storage::rollback(&db).unwrap();
        assert!(Storage::is_autocommit(&db));

        Storage::begin(&db).unwrap();
        Storage::execute(&db, "INSERT INTO storage_tx VALUES (2)").unwrap();
        Storage::commit(&db).unwrap();
        assert!(Storage::is_autocommit(&db));
    }

    #[test]
    fn db_storage_flushes_symbol_batches() {
        let db = db::open(None).unwrap();
        Storage::execute(
            &db,
            "CREATE TABLE _strings (id INTEGER PRIMARY KEY, content TEXT NOT NULL, norm TEXT NOT NULL)",
        ).unwrap();
        let mut sink = SymSink::new();
        sink.sym("StorageTraitSymbol");
        sink.sym("StorageTraitSymbol");

        assert_eq!(Storage::flush_syms(&db, &mut sink).unwrap(), 1);
        assert_eq!(Storage::flush_pending_syms(&db).unwrap(), 0);
    }
}
