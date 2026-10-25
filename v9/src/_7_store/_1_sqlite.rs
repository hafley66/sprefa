//! `Store` over one SQLite connection. SQLite INTEGER is signed; a `Value` keeps its bits.

use crate::_0_id::Dict;
use crate::_2_ir::RelKind;
use crate::_2a_diag::Diag;
use crate::_3_plan::{Plan, RelId, Row};
use crate::_5_ends::{Cell, Change, Delta, Event, Store, Table};
use crate::_7_store::_0_sql::{
    rel_sql, RelSql, DICT_INSERT, FIXED, META_EVENT, META_EVENT_SET,
};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, ErrorCode};
use crate::_1_effects::now;
use std::time::Duration;

pub struct Sqlite {
    pub db: Connection,
    pub rels: Vec<RelSql>,
    /// Rows one `query` may return; `MAX_QUERY_ROWS` at open.
    pub max_rows: usize,
    /// Time one `query` may run; `MAX_QUERY_TIME` at open.
    pub max_time: Duration,
    /// `meta.event`: the last event written.
    pub event: Event,
}

/// Rows one `query` returns at most; protects the Core thread's memory and the wire body.
pub const MAX_QUERY_ROWS: usize = 1 << 18;
/// Time one `query` runs at most; protects the Core thread, which serves every client.
pub const MAX_QUERY_TIME: Duration = Duration::from_secs(2);
/// SQLite virtual machine steps between two checks of the query deadline.
const PROGRESS_STEPS: i32 = 1000;

fn sql_error(error: rusqlite::Error) -> Diag {
    Diag::Store(error.to_string())
}

impl Sqlite {
    /// Creates the schema when absent. The first open records `meta`; a later open under another
    /// recipe or plan hash is `Diag::Mismatch` and creates nothing.
    pub fn open(plan: &Plan, recipe: &str, mut db: Connection) -> Result<Sqlite, Diag> {
        let rels = rel_sql(plan)?;
        let tx = db.transaction().map_err(sql_error)?;
        tx.execute_batch(FIXED).map_err(sql_error)?;
        let has_event: bool = tx
            .query_row(
                "SELECT count(*) FROM pragma_table_info('meta') WHERE name = 'event'",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !has_event {
            tx.execute_batch(META_EVENT).map_err(sql_error)?;
        }
        let stored: Vec<(String, i64, i64)> = tx
            .prepare("SELECT recipe, plan_hash, event FROM meta")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                    .collect()
            })
            .map_err(sql_error)?;
        let event = match stored.as_slice() {
            [] => {
                tx.execute(
                    "INSERT INTO meta (recipe, plan_hash) VALUES (?1, ?2)",
                    rusqlite::params![recipe, plan.hash as i64],
                )
                .map_err(sql_error)?;
                0
            }
            [(stored_recipe, stored_hash, event)]
                if stored_recipe == recipe && *stored_hash as u64 == plan.hash =>
            {
                *event as Event
            }
            [(stored_recipe, stored_hash, _)] => {
                return Err(Diag::Mismatch {
                    stored_recipe: stored_recipe.clone(),
                    stored_hash: *stored_hash as u64,
                    recipe: recipe.to_string(),
                    hash: plan.hash,
                })
            }
            many => return Err(Diag::Store(format!("meta holds {} rows", many.len()))),
        };
        let ddl: String = rels.iter().map(|rel| rel.create.as_str()).collect();
        tx.execute_batch(&ddl).map_err(sql_error)?;
        tx.commit().map_err(sql_error)?;
        Ok(Sqlite {
            db,
            rels,
            max_rows: MAX_QUERY_ROWS,
            max_time: MAX_QUERY_TIME,
            event,
        })
    }

    /// Rows of `rel` from its `select` or `present` statement, `?1` bound when given.
    fn read(&self, rel: RelId, sql: &str, json: Option<String>) -> Result<Vec<Row>, Diag> {
        let Some(declared) = self.rels.get(rel as usize) else {
            return Err(Diag::Store(format!("no rel {rel}")));
        };
        let mut statement = self.db.prepare_cached(sql).map_err(sql_error)?;
        let read = |row: &rusqlite::Row<'_>| {
            (0..declared.width)
                .map(|slot| row.get::<_, i64>(slot).map(|value| value as u64))
                .collect::<rusqlite::Result<Row>>()
        };
        let rows = match json {
            Some(json) => statement.query_map([json], read),
            None => statement.query_map([], read),
        };
        rows.and_then(|rows| rows.collect()).map_err(sql_error)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn rows_json(rows: &[&Row]) -> String {
    let rows: Vec<Vec<i64>> = rows
        .iter()
        .map(|row| row.iter().map(|value| *value as i64).collect())
        .collect();
    serde_json::to_string(&rows).expect("ids serialize")
}

fn cell(value: ValueRef<'_>) -> Cell {
    match value {
        ValueRef::Null => Cell::Null,
        ValueRef::Integer(int) => Cell::Int(int),
        ValueRef::Real(real) => Cell::Real(real),
        ValueRef::Text(text) => Cell::Text(String::from_utf8_lossy(text).into_owned()),
        ValueRef::Blob(blob) => Cell::Blob(blob.to_vec()),
    }
}

impl Store for Sqlite {
    /// Loads all of `dict`; returns every Base and Answer row with diff +1, sorted.
    fn replay(&mut self, dict: &mut Dict) -> Result<Vec<Change>, Diag> {
        let entries: Vec<(i64, Vec<u8>)> = self
            .db
            .prepare("SELECT id, bytes FROM dict")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect()
            })
            .map_err(sql_error)?;
        for (id, bytes) in entries {
            dict.load(id as u64, bytes.into_boxed_slice());
        }
        let mut changes = Vec::new();
        for rel in &self.rels {
            if !matches!(rel.kind, RelKind::Base | RelKind::Answer { .. }) {
                continue;
            }
            for row in self.read(rel.rel, &rel.select, None)? {
                changes.push(Change {
                    rel: rel.rel,
                    row,
                    diff: 1,
                });
            }
        }
        changes.sort();
        Ok(changes)
    }

    /// Deletes before inserts per table. A delete that misses a row, an insert of a present row,
    /// or a row of the wrong width is `Diag::Store` and the transaction rolls back.
    fn write(&mut self, delta: &Delta, fresh: &[(u64, &[u8])]) -> Result<usize, Diag> {
        let mut signed: Vec<[Vec<&Row>; 2]> = self.rels.iter().map(|_| [vec![], vec![]]).collect();
        for change in &delta.changes {
            let Some(rel) = self.rels.get(change.rel as usize) else {
                return Err(Diag::Store(format!(
                    "event {}: no rel {}",
                    delta.event, change.rel
                )));
            };
            if change.row.len() != rel.width {
                return Err(Diag::Store(format!(
                    "event {}: rel {} row {:?} has width {}, expected {}",
                    delta.event,
                    change.rel,
                    change.row,
                    change.row.len(),
                    rel.width
                )));
            }
            match change.diff.signum() {
                -1 => signed[change.rel as usize][0].push(&change.row),
                1 => signed[change.rel as usize][1].push(&change.row),
                _ => {}
            }
        }
        let tx = self.db.transaction().map_err(sql_error)?;
        let mut written = 0;
        if !fresh.is_empty() {
            let entries: Vec<(i64, String)> = fresh
                .iter()
                .map(|(id, bytes)| (*id as i64, hex(bytes)))
                .collect();
            written += tx
                .prepare_cached(DICT_INSERT)
                .and_then(|mut statement| {
                    statement.execute([serde_json::to_string(&entries).expect("dict serializes")])
                })
                .map_err(sql_error)?;
        }
        for (rel, [removed, added]) in self.rels.iter().zip(&signed) {
            for (sql, rows) in [(&rel.delete, removed), (&rel.insert, added)] {
                if rows.is_empty() {
                    continue;
                }
                let count = tx
                    .prepare_cached(sql)
                    .and_then(|mut statement| statement.execute([rows_json(rows)]))
                    .map_err(sql_error)?;
                if count != rows.len() {
                    return Err(Diag::Store(format!(
                        "event {}: {sql} touched {count} of {} rows",
                        delta.event,
                        rows.len()
                    )));
                }
                written += count;
            }
        }
        tx.prepare_cached(META_EVENT_SET)
            .and_then(|mut statement| statement.execute([delta.event as i64]))
            .map_err(sql_error)?;
        tx.commit().map_err(sql_error)?;
        self.event = delta.event;
        Ok(written)
    }

    /// One statement, read-only and returning columns; anything else (a write, BEGIN, ATTACH, a
    /// second statement) is `Diag::Store`. More than `max_rows` rows is `Diag::QueryRows`; a run
    /// past `max_time` is interrupted and is `Diag::QueryTime`.
    fn query(&self, sql: &str) -> Result<Table, Diag> {
        let deadline = now() + self.max_time;
        self.db
            .progress_handler(PROGRESS_STEPS, Some(move || now() > deadline))
            .map_err(sql_error)?;
        let table = self.bounded_query(sql);
        self.db
            .progress_handler(0, None::<fn() -> bool>)
            .map_err(sql_error)?;
        table.map_err(|error| match error {
            rusqlite::Error::SqliteFailure(failure, _)
                if failure.code == ErrorCode::OperationInterrupted =>
            {
                Diag::QueryTime {
                    max_ms: self.max_time.as_millis() as u64,
                }
            }
            other => sql_error(other),
        })?
    }

    fn present(&self, rel: RelId, rows: &[&Row]) -> Result<Vec<Row>, Diag> {
        let Some(declared) = self.rels.get(rel as usize) else {
            return Err(Diag::Store(format!("no rel {rel}")));
        };
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        self.read(rel, &declared.present, Some(rows_json(rows)))
    }

    fn rows(&self, rel: RelId) -> Result<Vec<Row>, Diag> {
        let Some(declared) = self.rels.get(rel as usize) else {
            return Err(Diag::Store(format!("no rel {rel}")));
        };
        self.read(rel, &declared.select, None)
    }

    fn event(&self) -> Event {
        self.event
    }
}

impl Sqlite {
    /// `query` under the progress handler: SQLite errors stay `rusqlite::Error` so the caller
    /// can tell an interrupt; the row cap and the read-only check are already `Diag`.
    fn bounded_query(&self, sql: &str) -> rusqlite::Result<Result<Table, Diag>> {
        let mut statement = self.db.prepare(sql)?;
        if !statement.readonly() || statement.column_count() == 0 {
            return Ok(Err(Diag::Store(format!(
                "query is not a read-only select: {sql}"
            ))));
        }
        let columns: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(String::from)
            .collect();
        let width = columns.len();
        let mut rows = Vec::new();
        let mut cursor = statement.query([])?;
        // Bounded: returns once `rows` holds `max_rows` and another row comes.
        while let Some(row) = cursor.next()? {
            if rows.len() == self.max_rows {
                return Ok(Err(Diag::QueryRows {
                    max_rows: self.max_rows,
                }));
            }
            rows.push(
                (0..width)
                    .map(|slot| row.get_ref(slot).map(cell))
                    .collect::<rusqlite::Result<Vec<Cell>>>()?,
            );
        }
        Ok(Ok(Table { columns, rows }))
    }
}
