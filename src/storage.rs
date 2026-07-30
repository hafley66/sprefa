//! Counted relational-storage seam.
//!
//! `Storage` is deliberately shaped like the small DB-API surface the engine
//! already uses. The `Db` implementation delegates to its existing counted and
//! plural methods, preserving SQLite behavior while giving callers a backend
//! boundary that does not expose the raw database connection.
//!
use anyhow::Result;
use std::cell::Cell;
use std::time::Instant;

use crate::ast::Value;
use crate::db::Db;
use crate::spine::SymSink;

pub(crate) mod call;

thread_local! {
    /// Cumulative (retract_ns, insert_ns) spent inside `Storage::retract_rows`/
    /// `insert_rows` on the current thread since the last [`take_write_ns`]
    /// call. Test-only: the corpus-scaling profiling harness
    /// (`tests/it/family_scaling_probe.rs`) uses this to attribute the
    /// render-write share of a one-file-edit tick's wall time separately from
    /// `FamilyRouter`'s derive/reconcile timings (`src/engine/family/
    /// router.rs`) — the two together cover the family flip's own cost;
    /// what's left over is extraction (populating `_call_owner`/
    /// `_call_raw_site`/`_call_resolution`), which lives outside this file's
    /// ownership and is not instrumented here.
    static WRITE_NS: Cell<(u128, u128)> = const { Cell::new((0, 0)) };
}

/// Read and reset the current thread's accumulated `(retract_ns, insert_ns)`.
/// Test-only.
pub fn take_write_ns() -> (u128, u128) {
    WRITE_NS.with(|cell| cell.replace((0, 0)))
}

fn add_retract_ns(elapsed_ns: u128) {
    WRITE_NS.with(|cell| {
        let (retract_ns, insert_ns) = cell.get();
        cell.set((retract_ns + elapsed_ns, insert_ns));
    });
}

fn add_insert_ns(elapsed_ns: u128) {
    WRITE_NS.with(|cell| {
        let (retract_ns, insert_ns) = cell.get();
        cell.set((retract_ns, insert_ns + elapsed_ns));
    });
}

pub trait Storage {
    /// Execute one parameterless statement through the counted database seam.
    fn execute(&self, sql: &str) -> Result<usize>;

    /// Execute a multi-statement script through the counted database seam.
    fn execute_batch(&self, sql: &str) -> Result<()>;

    /// Insert rows through the existing chunked, plural write path.
    fn insert_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize>;

    /// Replace a relation through the existing index-aware reload path.
    fn reload_rel(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize>;

    /// Retract specific rows by full-tuple identity via chunked, batched
    /// row-value `DELETE ... WHERE (cols) IN (VALUES ...)` statements (same
    /// `PARAM_BUDGET` chunking as `insert_rows`), never a per-row loop. The
    /// delete twin of the reconcile/render step: applying a `RowDelta`'s
    /// retracted set incrementally instead of overwriting the whole relation.
    fn retract_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize>;

    /// Return the existing SQLite-backed structural and size statistics.
    fn rel_stats(&self, rel: &str) -> Result<serde_json::Value>;

    /// Flush one explicitly collected symbol batch.
    fn flush_syms(&self, sink: &mut SymSink) -> Result<usize>;

    /// `flush_syms` with a caller-chosen N+1 counter key (see
    /// `Db::flush_syms_keyed`).
    fn flush_syms_keyed(&self, sink: &mut SymSink, bump_key: &str) -> Result<usize>;

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

impl Db {
    /// The unstinted `retract_rows` body — see [`Storage::retract_rows`] for
    /// the full doc. Split out so that trait method's timing wrapper (feeding
    /// [`take_write_ns`], the corpus-scaling profiling harness's render-write
    /// attribution) can wrap ONE call instead of threading `Instant::now()`
    /// through every early return.
    fn retract_rows_uncounted(
        &self,
        table: &str,
        cols: &[&str],
        rows: &[Vec<Value>],
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let col_tuple = cols.join(", ");
        let one_tuple = format!("({})", vec!["?"; cols.len()].join(", "));
        // Same chunk math as `Db::insert_rows`: SQLITE_MAX_VARIABLE_NUMBER caps
        // one statement's bound parameters, so `rows` is batched into chunks
        // of `chunk_rows` before each `DELETE ... VALUES` fires. Collect-then-
        // flush per chunk, never a per-row `DELETE`.
        let chunk_rows = (crate::db::PARAM_BUDGET / cols.len().max(1)).max(1);
        let conn = self.conn();
        let mut total_deleted = 0;
        for chunk in rows.chunks(chunk_rows) {
            let values = vec![one_tuple.clone(); chunk.len()].join(", ");
            let sql = format!("DELETE FROM {table} WHERE ({col_tuple}) IN (VALUES {values})");
            let params: Vec<rusqlite::types::Value> = chunk
                .iter()
                .flatten()
                .map(|cell| match cell {
                    Value::Text(s) => rusqlite::types::Value::Text(s.clone()),
                    Value::Int(n) => rusqlite::types::Value::Integer(*n),
                    Value::Null => rusqlite::types::Value::Null,
                })
                .collect();
            let mut stmt = conn.prepare(&sql)?;
            total_deleted += stmt.execute(rusqlite::params_from_iter(params))?;
        }
        Ok(total_deleted)
    }
}

impl Storage for Db {
    fn execute(&self, sql: &str) -> Result<usize> {
        Db::exec(self, sql)
    }

    fn execute_batch(&self, sql: &str) -> Result<()> {
        Db::execute_batch(self, sql)
    }

    fn insert_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        let started = Instant::now();
        let result = Db::insert_rows(self, table, cols, rows);
        add_insert_ns(started.elapsed().as_nanos());
        result
    }

    fn reload_rel(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        Db::reload_rel(self, table, cols, rows)
    }

    fn retract_rows(&self, table: &str, cols: &[&str], rows: &[Vec<Value>]) -> Result<usize> {
        let started = Instant::now();
        let result = self.retract_rows_uncounted(table, cols, rows);
        add_retract_ns(started.elapsed().as_nanos());
        result
    }

    fn rel_stats(&self, rel: &str) -> Result<serde_json::Value> {
        Db::rel_stats(self, rel)
    }

    fn flush_syms(&self, sink: &mut SymSink) -> Result<usize> {
        Db::flush_syms(self, sink)
    }

    fn flush_syms_keyed(&self, sink: &mut SymSink, bump_key: &str) -> Result<usize> {
        Db::flush_syms_keyed(self, sink, bump_key)
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
        )
        .unwrap();

        let initial = vec![
            vec![Value::Int(1), Value::Text("one".into())],
            vec![Value::Int(2), Value::Text("two".into())],
        ];
        assert_eq!(
            Storage::insert_rows(&db, "rel_storage_trait", &["id", "value"], &initial,).unwrap(),
            2
        );

        let initial_stats = Storage::rel_stats(&db, "storage_trait").unwrap();
        assert_eq!(initial_stats["rows"], 2);

        let replacement = vec![vec![Value::Int(3), Value::Text("three".into())]];
        assert_eq!(
            Storage::reload_rel(&db, "rel_storage_trait", &["id", "value"], &replacement,).unwrap(),
            1
        );
        let stats = Storage::rel_stats(&db, "storage_trait").unwrap();
        assert_eq!(stats["rows"], 1);
        assert_eq!(
            stats["indexes"],
            serde_json::json!(["rel_storage_trait_value"])
        );
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
    fn retract_rows_chunks_by_param_budget() {
        let db = db::open(None).unwrap();
        Storage::execute(&db, "CREATE TABLE retract_chunk_budget (a INTEGER, b TEXT)").unwrap();

        // 2 cols -> 16000 rows per statement; 33k rows = 3 chunks, each well
        // under SQLITE_MAX_VARIABLE_NUMBER, so the DELETE never blows the
        // bound-parameter limit the way one giant IN (VALUES ...) would.
        let rows: Vec<Vec<Value>> = (0..33_000)
            .map(|row_index| vec![Value::Int(row_index), Value::Text(format!("r{row_index}"))])
            .collect();
        assert_eq!(
            Storage::insert_rows(&db, "retract_chunk_budget", &["a", "b"], &rows).unwrap(),
            33_000
        );

        let deleted =
            Storage::retract_rows(&db, "retract_chunk_budget", &["a", "b"], &rows).unwrap();
        assert_eq!(deleted, 33_000);

        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM retract_chunk_budget", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn retract_rows_empty_input_is_a_no_op() {
        let db = db::open(None).unwrap();
        Storage::execute(&db, "CREATE TABLE retract_empty_noop (a INTEGER)").unwrap();
        assert_eq!(
            Storage::retract_rows(&db, "retract_empty_noop", &["a"], &[]).unwrap(),
            0
        );
    }

    #[test]
    fn retract_rows_single_row_is_deleted() {
        let db = db::open(None).unwrap();
        Storage::execute(&db, "CREATE TABLE retract_single_row (a INTEGER, b TEXT)").unwrap();
        let rows = vec![vec![Value::Int(1), Value::Text("one".into())]];
        Storage::insert_rows(&db, "retract_single_row", &["a", "b"], &rows).unwrap();

        let deleted = Storage::retract_rows(&db, "retract_single_row", &["a", "b"], &rows).unwrap();
        assert_eq!(deleted, 1);
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM retract_single_row", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    /// Exactly `chunk_rows` (= `PARAM_BUDGET / ncol`) rows must fit in ONE
    /// chunked `DELETE ... VALUES` statement — the boundary below `budget + 1`
    /// pushes into a second chunk.
    #[test]
    fn retract_rows_exactly_one_chunk_at_the_budget_boundary() {
        let db = db::open(None).unwrap();
        Storage::execute(&db, "CREATE TABLE retract_exact_budget (a INTEGER, b TEXT)").unwrap();

        let ncol = 2;
        let chunk_rows = crate::db::PARAM_BUDGET / ncol;
        let rows: Vec<Vec<Value>> = (0..chunk_rows as i64)
            .map(|row_index| vec![Value::Int(row_index), Value::Text(format!("r{row_index}"))])
            .collect();
        Storage::insert_rows(&db, "retract_exact_budget", &["a", "b"], &rows).unwrap();

        let deleted =
            Storage::retract_rows(&db, "retract_exact_budget", &["a", "b"], &rows).unwrap();
        assert_eq!(
            deleted, chunk_rows,
            "exactly-budget rows must all delete in the single chunk"
        );
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM retract_exact_budget", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    /// One row past the per-chunk budget must still fully delete — the extra
    /// row forces a second chunk, proving the chunk boundary doesn't drop it.
    #[test]
    fn retract_rows_budget_plus_one_spans_two_chunks() {
        let db = db::open(None).unwrap();
        Storage::execute(
            &db,
            "CREATE TABLE retract_budget_plus_one (a INTEGER, b TEXT)",
        )
        .unwrap();

        let ncol = 2;
        let chunk_rows = crate::db::PARAM_BUDGET / ncol;
        let rows: Vec<Vec<Value>> = (0..(chunk_rows + 1) as i64)
            .map(|row_index| vec![Value::Int(row_index), Value::Text(format!("r{row_index}"))])
            .collect();
        Storage::insert_rows(&db, "retract_budget_plus_one", &["a", "b"], &rows).unwrap();

        let deleted =
            Storage::retract_rows(&db, "retract_budget_plus_one", &["a", "b"], &rows).unwrap();
        assert_eq!(
            deleted,
            chunk_rows + 1,
            "budget+1 rows must fully delete across two chunks"
        );
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM retract_budget_plus_one", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    /// A wide 8-column tuple must shrink `chunk_rows` proportionally (budget is
    /// spent on PARAMS, not rows): `2 * (PARAM_BUDGET / 8) - 1` rows is one row
    /// short of what a per-row-fixed (column-count-blind) chunker would need
    /// two full chunks for — exercising the "per-param not per-row" math the
    /// P0 fix (4eb5694a) landed.
    #[test]
    fn retract_rows_wide_tuple_chunks_by_param_not_row_count() {
        let db = db::open(None).unwrap();
        Storage::execute(
            &db,
            "CREATE TABLE retract_wide_tuple (\
                c0 INTEGER, c1 INTEGER, c2 INTEGER, c3 INTEGER, \
                c4 INTEGER, c5 INTEGER, c6 INTEGER, c7 INTEGER)",
        )
        .unwrap();
        let cols = ["c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7"];
        let ncol = cols.len();
        let chunk_rows = crate::db::PARAM_BUDGET / ncol;
        let row_count = 2 * chunk_rows - 1;
        let rows: Vec<Vec<Value>> = (0..row_count as i64)
            .map(|row_index| {
                (0..ncol as i64)
                    .map(|col_index| Value::Int(row_index * 10 + col_index))
                    .collect()
            })
            .collect();
        Storage::insert_rows(&db, "retract_wide_tuple", &cols, &rows).unwrap();

        let deleted = Storage::retract_rows(&db, "retract_wide_tuple", &cols, &rows).unwrap();
        assert_eq!(
            deleted, row_count,
            "2*chunk_rows-1 wide-tuple rows must all delete across 2 chunks"
        );
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM retract_wide_tuple", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    /// Pinned semantic: `retract_rows` deletes by full-tuple VALUE identity
    /// (`WHERE (cols) IN (VALUES ...)`), not by rowid — a table holding the
    /// SAME tuple twice (no uniqueness constraint on these columns) loses BOTH
    /// physical copies from a `rows` argument naming that tuple once.
    #[test]
    fn retract_rows_duplicate_row_in_table_removes_all_copies() {
        let db = db::open(None).unwrap();
        Storage::execute(&db, "CREATE TABLE retract_dup_in_table (a INTEGER, b TEXT)").unwrap();

        let duplicated_row = vec![Value::Int(1), Value::Text("dup".into())];
        let other_row = vec![Value::Int(2), Value::Text("other".into())];
        let seed_rows = vec![
            duplicated_row.clone(),
            duplicated_row.clone(),
            other_row.clone(),
        ];
        Storage::insert_rows(&db, "retract_dup_in_table", &["a", "b"], &seed_rows).unwrap();

        let deleted =
            Storage::retract_rows(&db, "retract_dup_in_table", &["a", "b"], &[duplicated_row])
                .unwrap();
        assert_eq!(
            deleted, 2,
            "pinned: a full-tuple DELETE removes every physical copy sharing that tuple, not just one"
        );
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM retract_dup_in_table", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1, "the untouched other_row must survive");
    }

    #[test]
    fn retract_rows_of_absent_row_is_a_no_op() {
        let db = db::open(None).unwrap();
        Storage::execute(&db, "CREATE TABLE retract_absent_row (a INTEGER, b TEXT)").unwrap();
        let present_row = vec![Value::Int(1), Value::Text("present".into())];
        Storage::insert_rows(
            &db,
            "retract_absent_row",
            &["a", "b"],
            &vec![present_row.clone()],
        )
        .unwrap();

        let absent_row = vec![Value::Int(99), Value::Text("never-inserted".into())];
        let deleted =
            Storage::retract_rows(&db, "retract_absent_row", &["a", "b"], &[absent_row]).unwrap();
        assert_eq!(
            deleted, 0,
            "retracting a row that was never present must be a silent no-op"
        );
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM retract_absent_row", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1, "the unrelated present_row must survive untouched");
    }

    #[test]
    fn db_storage_flushes_symbol_batches() {
        let db = db::open(None).unwrap();
        Storage::execute(
            &db,
            "CREATE TABLE _strings (id INTEGER PRIMARY KEY, content TEXT NOT NULL)",
        )
        .unwrap();
        let mut sink = SymSink::new();
        sink.sym("StorageTraitSymbol");
        sink.sym("StorageTraitSymbol");

        assert_eq!(Storage::flush_syms(&db, &mut sink).unwrap(), 1);
        assert_eq!(Storage::flush_pending_syms(&db).unwrap(), 0);
    }
}
