// The SQL seam. v6 law: async at the SQL seam only; in-memory row work above
// the seam stays plain sync. Each method is `async` outward and runs blocking
// rusqlite inward; none awaits inside a single statement, so a statement's
// effects are visible to the next statement in the same ordered batch.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use regex::Regex;
use rusqlite::functions::FunctionFlags;
use rusqlite::{params_from_iter, Connection, OptionalExtension, Row};
use std::sync::Arc;

use crate::types::{BoundaryError, BoundaryResult, QueryResult, ScalarValue, SqlStatement, Value};

/// rusqlite's own default is 16, which a program with more statements than
/// that would evict on every tick.
const DEFAULT_STATEMENT_CACHE: usize = 256;

pub type Error = rusqlite::Error;
pub type Result<T> = std::result::Result<T, Error>;

// SqlRunner is the trait analogue of ISqlSeam.runner. batch runs statements in
// order, one at a time, so a later statement observes this tick's earlier ones.

pub trait SqlRunner {
    fn execute(&self, statement: &SqlStatement) -> Result<QueryResult>;
    fn batch(&self, statements: &[SqlStatement]) -> Result<Vec<QueryResult>>;
    fn execute_multiple(&self, sql: &str) -> Result<()>;
    fn scalar(&self, sql: &str) -> Result<i64>;
}

pub struct SqliteSeam {
    conn: Connection,
    /// Distinct SQL texts compiled here, counted only when a caller asks:
    /// hashing every statement's text buys an ordinary fold nothing.
    prepared: RefCell<HashSet<String>>,
    counting: Cell<bool>,
    dispatches: Cell<u64>,
    cache_capacity: Cell<usize>,
}

impl SqliteSeam {
    pub fn in_memory() -> Result<Self> {
        // DL_DB_URL is the file-backed measurement door: the pragmas that are
        // no-ops on `:memory:` only have an effect on a real file.
        let conn = match std::env::var("DL_DB_URL") {
            Ok(url) if !url.is_empty() => Connection::open(url)?,
            _ => Connection::open_in_memory()?,
        };
        SqliteSeam::wrap(conn)
    }

    pub fn open(url: &str) -> Result<Self> {
        SqliteSeam::wrap(Connection::open(url)?)
    }

    fn wrap(conn: Connection) -> Result<Self> {
        install_scalars(&conn)?;
        apply_pragmas(&conn)?;
        Ok(SqliteSeam {
            conn,
            prepared: RefCell::new(HashSet::new()),
            counting: Cell::new(false),
            dispatches: Cell::new(0),
            cache_capacity: Cell::new(DEFAULT_STATEMENT_CACHE),
        })
    }

    /// Arms the prepare counters for a COUNT test; off on the fold path.
    pub fn count_prepares(&self) {
        self.counting.set(true);
    }

    /// The statement cache holds every text the IR fixes ahead of the fold, so
    /// no reusable statement is ever evicted and recompiled.
    pub fn size_statement_cache(&self, capacity: usize) {
        self.cache_capacity.set(capacity);
        self.conn.set_prepared_statement_cache_capacity(capacity);
    }

    pub fn statement_cache_capacity(&self) -> usize {
        self.cache_capacity.get()
    }

    /// A file-backed connection is the only one a durability pragma can move;
    /// `:memory:` has no journal to write.
    pub fn file_backed(&self) -> bool {
        is_file_backed(&self.conn)
    }

    /// The binding ceiling this build of SQLite enforces, read from the
    /// connection rather than assumed from a version note.
    pub fn variable_limit(&self) -> usize {
        self.conn
            .limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER)
            .unwrap_or(0)
            .max(0) as usize
    }

    pub fn distinct_sql_texts(&self) -> usize {
        self.prepared.borrow().len()
    }

    pub fn dispatches(&self) -> u64 {
        self.dispatches.get()
    }

    /// ONE SERVER, ONE DB: shared globals stay standing, a TABLE whose CREATE
    /// is the one already there keeps its rows, a moved shape is dropped.
    pub fn run_program_ddl(&self, ddl: &[String], queries: &[String]) -> Result<()> {
        for statement in ddl {
            let Some((kind, name)) = created_object(statement) else {
                continue;
            };
            if SHARED_GLOBALS.contains(&name) {
                continue;
            }
            if kind == "TABLE" && self.table_shape_stands(name, statement)? {
                continue;
            }
            self.execute_multiple(&format!("DROP {kind} IF EXISTS \"{name}\""))?;
        }
        for query in queries {
            self.execute_multiple(&format!("DROP VIEW IF EXISTS \"v_{query}\""))?;
        }
        let owned: Vec<String> = ddl.iter().map(|statement| idempotent(statement)).collect();
        self.run_ddl(&owned)
    }

    /// Whether the db already carries this exact table, whitespace-normalized:
    /// `sqlite_master.sql` is the CREATE as it was executed.
    fn table_shape_stands(&self, name: &str, statement: &str) -> Result<bool> {
        let standing: Option<String> = self
            .conn
            .query_row(
                "SELECT \"sql\" FROM \"sqlite_master\" WHERE \"type\" = 'table' AND \"name\" = ?1",
                rusqlite::params![name],
                |row| row.get(0),
            )
            .optional()?;
        Ok(standing.as_deref().map(normalized_ddl) == Some(normalized_ddl(statement)))
    }

    pub fn run_ddl(&self, ddl: &[String]) -> Result<()> {
        let _scope = crate::trace::Scope::phase("ddl");
        for statement in ddl {
            SqlRunner::execute_multiple(self, statement)?;
        }
        Ok(())
    }
}

// Every scalar the compile/registry.pl text_scalar rows can render that core
// SQLite does not ship. libsql carries `reverse` for the TypeScript door;
// rusqlite links plain SQLite, which does not.
fn install_scalars(conn: &Connection) -> Result<()> {
    install_reverse(conn)?;
    install_regexp(conn)
}

// Unicode scalar values, not bytes: the conformance oracle answers
// reverse('\u4e2d\u00e9') = '\u00e9\u4e2d'.
fn install_reverse(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "reverse",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let Ok(text) = ctx.get_raw(0).as_str() else {
                return Ok(None);
            };
            Ok(Some(text.chars().rev().collect::<String>()))
        },
    )
}

fn install_regexp(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let pattern: Arc<Regex> = ctx.get_or_create_aux(
                0,
                |value| -> std::result::Result<_, Box<dyn std::error::Error + Send + Sync>> {
                    Ok(Regex::new(value.as_str()?)?)
                },
            )?;
            let Ok(text) = ctx.get_raw(1).as_str() else {
                return Ok(false);
            };
            Ok(pattern.is_match(text))
        },
    )
}

/// page_size is the one pragma measured to move anything on `:memory:`, and it
/// has to precede the first table. The durability pragmas are file-only.
fn apply_pragmas(conn: &Connection) -> Result<()> {
    if std::env::var("DL_SEAM_PRAGMAS").as_deref() == Ok("0") {
        return Ok(());
    }
    conn.execute_batch("PRAGMA page_size=16384; PRAGMA temp_store=MEMORY;")?;
    if is_file_backed(conn) {
        conn.execute_batch(
            "PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA cache_size=-262144; PRAGMA mmap_size=1073741824;",
        )?;
    }
    Ok(())
}

fn is_file_backed(conn: &Connection) -> bool {
    conn.path()
        .map(|path| !path.is_empty() && path != ":memory:")
        .unwrap_or(false)
}

fn to_param(value: &ScalarValue) -> rusqlite::types::Value {
    match value {
        ScalarValue::Integer(v) => rusqlite::types::Value::Integer(*v),
        ScalarValue::Real(v) => rusqlite::types::Value::Real(*v),
        ScalarValue::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
        ScalarValue::Text(v) => rusqlite::types::Value::Text(v.clone()),
        ScalarValue::Bytes(v) => rusqlite::types::Value::Blob(v.clone()),
    }
}

fn row_to_values(row: &Row, columns: &[String]) -> Vec<Value> {
    let mut out = Vec::with_capacity(columns.len());
    for index in 0..columns.len() {
        let value = match row.get_ref(index) {
            Ok(rusqlite::types::ValueRef::Integer(v)) => Value::Integer(v),
            Ok(rusqlite::types::ValueRef::Real(v)) => Value::Real(v),
            Ok(rusqlite::types::ValueRef::Text(v)) => {
                Value::Text(String::from_utf8_lossy(v).into_owned())
            }
            Ok(rusqlite::types::ValueRef::Blob(v)) => Value::Bytes(v.to_vec()),
            Ok(rusqlite::types::ValueRef::Null) => Value::Text(String::new()),
            Err(_) => Value::Text(String::new()),
        };
        out.push(value);
    }
    out
}

/// Aggregate seam counters, read once at the end of a fold. Per-statement spans
/// would be tens of thousands of events for one rail run, which measures the
/// tracing layer more than the SQL.
#[derive(Default)]
pub struct SeamTally {
    pub statements: std::sync::atomic::AtomicU64,
    /// Statements that returned no rows AND changed no rows: the query ran and
    /// nothing came of it.
    pub inert: std::sync::atomic::AtomicU64,
    pub rows_out: std::sync::atomic::AtomicU64,
    pub rows_changed: std::sync::atomic::AtomicU64,
    pub nanos: std::sync::atomic::AtomicU64,
}

/// Inert statements by shape, so the report names WHICH query ran for nothing.
/// Behind a Mutex rather than atomics because it is keyed text; only populated
/// when DL_SEAM_SHAPES is set, so an ordinary fold pays one env read.
pub static INERT_SHAPES: std::sync::LazyLock<Mutex<BTreeMap<String, (u64, u64)>>> =
    std::sync::LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// The A/B door for the statement cache, measured on both workloads before the
/// default was set.
pub fn statement_cache_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| std::env::var("DL_SEAM_STMT_CACHE").as_deref() != Ok("0"))
}

pub fn seam_shapes_wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| std::env::var_os("DL_SEAM_SHAPES").is_some())
}

/// verb + the first table name, which is what identifies a plan's statement
/// without carrying its whole body or its bound values.
fn statement_shape(sql: &str) -> String {
    let mut words = sql.split_whitespace();
    let verb = words.next().unwrap_or("?").to_uppercase();
    let table = match verb.as_str() {
        "INSERT" | "REPLACE" => words.nth(1),
        "DELETE" => words.nth(1),
        "UPDATE" => words.next(),
        "SELECT" => sql
            .split_whitespace()
            .skip_while(|word| !word.eq_ignore_ascii_case("from"))
            .nth(1),
        _ => words.next(),
    };
    format!(
        "{verb} {}",
        table.unwrap_or("?").trim_matches(|c| c == '"' || c == '(')
    )
}

/// Statements in one execute_batch text. Semicolons inside quoted literals do
/// not end a statement, so the split cannot be a plain `split(';')`.
fn batch_statement_count(sql: &str) -> u64 {
    let mut count = 0u64;
    let mut in_quote: Option<char> = None;
    let mut has_content = false;
    for character in sql.chars() {
        match in_quote {
            Some(quote) => {
                if character == quote {
                    in_quote = None;
                }
            }
            None => match character {
                '\'' | '"' => {
                    in_quote = Some(character);
                    has_content = true;
                }
                ';' => {
                    if has_content {
                        count += 1;
                    }
                    has_content = false;
                }
                other if !other.is_whitespace() => has_content = true,
                _ => {}
            },
        }
    }
    if has_content {
        count += 1;
    }
    count
}

pub static SEAM_TALLY: SeamTally = SeamTally {
    statements: std::sync::atomic::AtomicU64::new(0),
    inert: std::sync::atomic::AtomicU64::new(0),
    rows_out: std::sync::atomic::AtomicU64::new(0),
    rows_changed: std::sync::atomic::AtomicU64::new(0),
    nanos: std::sync::atomic::AtomicU64::new(0),
};

/// One event carrying the whole fold's SQL shape.
pub fn report_seam_tally() {
    use std::sync::atomic::Ordering::Relaxed;
    let statements = SEAM_TALLY.statements.load(Relaxed);
    let inert = SEAM_TALLY.inert.load(Relaxed);
    tracing::info!(
        statements,
        inert,
        inert_pct = if statements == 0 {
            0
        } else {
            inert * 100 / statements
        },
        rows_out = SEAM_TALLY.rows_out.load(Relaxed),
        rows_changed = SEAM_TALLY.rows_changed.load(Relaxed),
        ms = SEAM_TALLY.nanos.load(Relaxed) / 1_000_000,
        "seam tally"
    );
    if seam_shapes_wanted() {
        let shapes = INERT_SHAPES.lock().expect("inert shapes");
        let mut rows: Vec<(&String, &(u64, u64))> = shapes.iter().collect();
        rows.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
        for (shape, (count, micros)) in rows.into_iter().take(20) {
            tracing::info!(shape = %shape, count, ms = micros / 1000, "seam shape");
        }
    }
}

impl SqlRunner for SqliteSeam {
    fn execute(&self, statement: &SqlStatement) -> Result<QueryResult> {
        let started = std::time::Instant::now();
        let label = crate::trace::current_label();
        let span = tracing::trace_span!(
            "sql",
            relation = %label.relation,
            verb = label.verb,
            shape = tracing::field::Empty,
            rows_out = tracing::field::Empty,
            rows_changed = tracing::field::Empty,
            prepare_us = tracing::field::Empty,
            step_us = tracing::field::Empty
        );
        let _entered = span.enter();
        if self.counting.get() {
            self.dispatches.set(self.dispatches.get() + 1);
            if !self.prepared.borrow().contains(&statement.sql) {
                self.prepared.borrow_mut().insert(statement.sql.clone());
            }
        }
        let mut cached;
        let mut fresh;
        let stmt: &mut rusqlite::Statement = if statement_cache_wanted() {
            cached = self.conn.prepare_cached(&statement.sql)?;
            &mut cached
        } else {
            fresh = self.conn.prepare(&statement.sql)?;
            &mut fresh
        };
        let prepare_nanos = started.elapsed().as_nanos() as u64;
        let column_count = stmt.column_count();
        let columns: Vec<String> = (0..column_count)
            .map(|index| stmt.column_name(index).unwrap_or_default().to_string())
            .collect();
        // Projection statements receive the full trigger row even when their
        // generated SQL binds only a prefix of that row.
        let params: Vec<rusqlite::types::Value> = statement
            .args
            .iter()
            .take(stmt.parameter_count())
            .map(to_param)
            .collect();
        let mut rows = stmt.query(params_from_iter(params.iter()))?;
        let mut out_rows = Vec::new();
        while let Some(row) = rows.next()? {
            out_rows.push(row_to_values(row, &columns));
        }
        let rows_affected = self.conn.changes() as i64;
        let elapsed_nanos = started.elapsed().as_nanos() as u64;
        crate::trace::record(
            label,
            elapsed_nanos,
            out_rows.len() as u64,
            rows_affected.max(0) as u64,
        );
        if !span.is_disabled() {
            span.record("shape", statement_shape(&statement.sql).as_str());
            span.record("rows_out", out_rows.len() as u64);
            span.record("rows_changed", rows_affected);
            span.record("prepare_us", prepare_nanos / 1_000);
            span.record("step_us", elapsed_nanos.saturating_sub(prepare_nanos) / 1_000);
        }
        {
            use std::sync::atomic::Ordering::Relaxed;
            SEAM_TALLY.statements.fetch_add(1, Relaxed);
            SEAM_TALLY
                .rows_out
                .fetch_add(out_rows.len() as u64, Relaxed);
            SEAM_TALLY
                .rows_changed
                .fetch_add(rows_affected.max(0) as u64, Relaxed);
            SEAM_TALLY
                .nanos
                .fetch_add(started.elapsed().as_nanos() as u64, Relaxed);
            if out_rows.is_empty() && rows_affected <= 0 {
                SEAM_TALLY.inert.fetch_add(1, Relaxed);
                if seam_shapes_wanted() {
                    let mut shapes = INERT_SHAPES.lock().expect("inert shapes");
                    let entry = shapes
                        .entry(statement_shape(&statement.sql))
                        .or_insert((0, 0));
                    entry.0 += 1;
                    entry.1 += started.elapsed().as_micros() as u64;
                }
            } else if seam_shapes_wanted() {
                let mut shapes = INERT_SHAPES.lock().expect("inert shapes");
                let entry = shapes
                    .entry(format!("(live) {}", statement_shape(&statement.sql)))
                    .or_insert((0, 0));
                entry.0 += 1;
                entry.1 += started.elapsed().as_micros() as u64;
            }
        }
        Ok(QueryResult {
            rows: out_rows,
            columns,
            rows_affected,
        })
    }

    fn batch(&self, statements: &[SqlStatement]) -> Result<Vec<QueryResult>> {
        statements
            .iter()
            .map(|statement| self.execute(statement))
            .collect()
    }

    fn execute_multiple(&self, sql: &str) -> Result<()> {
        let started = std::time::Instant::now();
        let label = crate::trace::current_label();
        let span = tracing::trace_span!("sql_batch", relation = %label.relation, verb = label.verb);
        let _entered = span.enter();
        self.conn.execute_batch(sql)?;
        // A batch is N statements, not one: every caller joins with ";\n", and
        // a tally that charged it 1 hid the per-rel clears entirely.
        SEAM_TALLY.statements.fetch_add(
            batch_statement_count(sql),
            std::sync::atomic::Ordering::Relaxed,
        );
        crate::trace::record(label, started.elapsed().as_nanos() as u64, 0, 0);
        Ok(())
    }

    fn scalar(&self, sql: &str) -> Result<i64> {
        let started = std::time::Instant::now();
        let label = crate::trace::current_label();
        let span = tracing::trace_span!("sql_scalar", relation = %label.relation, verb = label.verb);
        let _entered = span.enter();
        let value = self.conn.query_row(sql, [], |row| row.get(0)).optional()?;
        crate::trace::record(label, started.elapsed().as_nanos() as u64, 1, 0);
        Ok(value.unwrap_or(0))
    }
}

// result_rows maps raw seam rows through declared column types, mirroring
// result_rows in 1_incremental.ts. bool columns normalize 0/1 to Bool; float
// columns normalize -0 to 0 and validate finite; int columns stay integer.

pub fn result_rows(
    result: &QueryResult,
    columns: &[String],
    column_types: &[crate::types::RowColumnType],
) -> BoundaryResult<Vec<Vec<Value>>> {
    result
        .rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(index, _column)| {
                    let value = row
                        .get(index)
                        .cloned()
                        .unwrap_or(Value::Text(String::new()));
                    let ty = column_types.get(index).copied();
                    normalize_boundary_value(value, ty)
                })
                .collect()
        })
        .collect()
}

fn normalize_boundary_value(
    value: Value,
    ty: Option<crate::types::RowColumnType>,
) -> BoundaryResult<Value> {
    match (ty, value) {
        (Some(crate::types::RowColumnType::Bool), Value::Integer(v)) => Ok(Value::Bool(v != 0)),
        (Some(crate::types::RowColumnType::Float), Value::Real(v)) => {
            if !v.is_finite() {
                panic!("float column crossed SQLite with non-finite value");
            }
            if v == 0.0 {
                Ok(Value::Real(0.0))
            } else {
                Ok(Value::Real(v))
            }
        }
        (Some(crate::types::RowColumnType::Float), Value::Integer(v)) => Ok(Value::Real(v as f64)),
        // F3: the consumer gets Vec<T>, never the array TEXT the `__list_`
        // view aggregated and never the interned entity id.
        (Some(crate::types::RowColumnType::List), Value::Text(text)) => {
            match serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                Ok(items) => Ok(Value::List(items)),
                Err(error) => Err(BoundaryError::ListColumnNotAnArray {
                    text,
                    detail: error.to_string(),
                }),
            }
        }
        (Some(crate::types::RowColumnType::Bytes), Value::Bytes(bytes)) => Ok(Value::Bytes(bytes)),
        (Some(crate::types::RowColumnType::Bytes), _value) => Err(
            BoundaryError::BytesAtScalarSeam(crate::types::ScalarSeam::SqlParameter),
        ),
        (_, value) => Ok(value),
    }
}

pub struct RawCols {
    pub map: HashMap<String, usize>,
}

pub fn column_index(result: &QueryResult, name: &str) -> Option<usize> {
    result.columns.iter().position(|column| column == name)
}

/// The objects every program in the one db shares. `__str` is the global text
/// intern table other programs' INTEGER columns point into, so dropping it would
/// dangle their rows; `__meta` already carries a `program` column.
pub const SHARED_GLOBALS: &[&str] = &["__str", "__meta"];

/// `sqlite_master.sql` records the CREATE as executed, minus `IF NOT EXISTS`,
/// so the two texts are compared with their whitespace collapsed.
fn normalized_ddl(statement: &str) -> String {
    statement
        .replacen("IF NOT EXISTS ", "", 1)
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
        .trim_end_matches(';')
        .to_string()
}

/// The kind and name a `CREATE ...` statement makes, or None for anything else.
fn created_object(statement: &str) -> Option<(&'static str, &str)> {
    for (prefix, kind) in [
        ("CREATE TABLE ", "TABLE"),
        ("CREATE TEMP VIEW ", "VIEW"),
        ("CREATE VIEW ", "VIEW"),
        ("CREATE UNIQUE INDEX ", "INDEX"),
        ("CREATE INDEX ", "INDEX"),
    ] {
        let Some(tail) = statement.strip_prefix(prefix) else {
            continue;
        };
        let tail = tail.strip_prefix("IF NOT EXISTS ").unwrap_or(tail);
        let quoted = tail.strip_prefix('"')?;
        let end = quoted.find('"')?;
        return Some((kind, &quoted[..end]));
    }
    None
}

/// Every CREATE TABLE has to be idempotent: a shared global and a kept table
/// are both already standing when the next start runs the DDL.
fn idempotent(statement: &str) -> String {
    match created_object(statement) {
        Some(("TABLE", _)) if !statement.contains("IF NOT EXISTS") => {
            statement.replacen("CREATE TABLE ", "CREATE TABLE IF NOT EXISTS ", 1)
        }
        _ => statement.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLL_STATE: &str = "CREATE TABLE \"p_poll_state_etag\" \
        (\"__id\" INTEGER PRIMARY KEY, \"page_url\" INTEGER, \"etag\" INTEGER)";

    fn stored_etags(seam: &SqliteSeam) -> i64 {
        seam.scalar("SELECT COUNT(*) FROM \"p_poll_state_etag\"").expect("count")
    }

    /// TEST: issues/dl6-run-restart-loses-etags. Every restart dropped this
    /// program's tables, so the poll re-downloaded ~1 MB it already had.
    /// Sabotage receipt: deleting the `table_shape_stands` arm in
    /// `run_program_ddl` turns `restart` back to 0 rows.
    #[test]
    fn a_restart_keeps_a_table_whose_shape_did_not_move() {
        let home = tempfile::tempdir().expect("tempdir");
        let url = home.path().join("dl6.db").to_string_lossy().to_string();
        let ddl = vec![POLL_STATE.to_string()];

        let first = SqliteSeam::open(&url).expect("first start");
        first.run_program_ddl(&ddl, &[]).expect("first ddl");
        first
            .execute_multiple("INSERT INTO \"p_poll_state_etag\" VALUES (1, 10, 20)")
            .expect("one stored etag");
        assert_eq!(stored_etags(&first), 1);
        drop(first);

        let restart = SqliteSeam::open(&url).expect("restart");
        restart.run_program_ddl(&ddl, &[]).expect("restart ddl");
        assert_eq!(
            stored_etags(&restart),
            1,
            "a restart re-reads the stored ETag rather than the whole wire"
        );
        drop(restart);

        let moved = vec![format!("{POLL_STATE}").replace("\"etag\" INTEGER", "\"etag\" TEXT")];
        let reshaped = SqliteSeam::open(&url).expect("reshaped start");
        reshaped.run_program_ddl(&moved, &[]).expect("reshaped ddl");
        assert_eq!(
            stored_etags(&reshaped),
            0,
            "a table whose columns moved is dropped: the rows no longer fit"
        );
    }

    #[test]
    fn a_shared_global_is_never_dropped() {
        let home = tempfile::tempdir().expect("tempdir");
        let url = home.path().join("dl6.db").to_string_lossy().to_string();
        let ddl = vec!["CREATE TABLE \"__str\" (\"__id\" INTEGER PRIMARY KEY, \
                        \"content\" TEXT NOT NULL UNIQUE)"
            .to_string()];
        let first = SqliteSeam::open(&url).expect("first start");
        first.run_program_ddl(&ddl, &[]).expect("first ddl");
        first
            .execute_multiple("INSERT INTO \"__str\" VALUES (1, 'orgs/hafley66/repos')")
            .expect("one interned text")
            ;
        first.run_program_ddl(&ddl, &[]).expect("second ddl in the same process");
        assert_eq!(
            first.scalar("SELECT COUNT(*) FROM \"__str\"").expect("count"),
            1,
            "the interned text dictionary is shared by every program in the one db"
        );
    }

    // Pre-fix execute_multiple tallied 0 for any batch, so a per-rel tick and a
    // shared tick reported the same statement count (docs/failure-modes.md 75).
    #[test]
    fn a_batch_counts_every_statement_in_it() {
        assert_eq!(
            batch_statement_count(
                "DELETE FROM \"__delta_a\";\nDELETE FROM \"__next_frontier_a\""
            ),
            2
        );
        assert_eq!(
            batch_statement_count("INSERT INTO \"t\" VALUES ('a;b');\nDELETE FROM \"t\""),
            2,
            "a semicolon inside a literal does not end a statement"
        );
        assert_eq!(batch_statement_count("DELETE FROM \"t\";\n"), 1);
        assert_eq!(batch_statement_count("  \n ;; \n "), 0);
    }

    #[test]
    fn a_batch_reaches_the_seam_tally() {
        use std::sync::atomic::Ordering::Relaxed;
        let seam = SqliteSeam::in_memory().expect("seam");
        seam.run_ddl(&["CREATE TABLE \"t\" (\"v\" INTEGER)".to_string()])
            .expect("ddl");
        let before = SEAM_TALLY.statements.load(Relaxed);
        seam.execute_multiple("INSERT INTO \"t\" VALUES (1);\nDELETE FROM \"t\"")
            .expect("batch");
        assert_eq!(SEAM_TALLY.statements.load(Relaxed) - before, 2);
    }
}
