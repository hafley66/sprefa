//! `Store` over one SQLite connection. SQLite INTEGER is signed; a `Value` keeps its bits.

use crate::_0_id::Dict;
use crate::_2_ir::RelKind;
use crate::_2a_diag::Diag;
use crate::_3_plan::{Plan, Row};
use crate::_5_ends::{Cell, Change, Delta, Store, Table};
use crate::_7_store::_0_sql::{rel_sql, RelSql, DICT_INSERT, FIXED};
use rusqlite::types::ValueRef;
use rusqlite::Connection;

pub struct Sqlite {
    pub db: Connection,
    pub rels: Vec<RelSql>,
}

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
        let stored: Vec<(String, i64)> = tx
            .prepare("SELECT recipe, plan_hash FROM meta")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect()
            })
            .map_err(sql_error)?;
        match stored.as_slice() {
            [] => {
                tx.execute(
                    "INSERT INTO meta (recipe, plan_hash) VALUES (?1, ?2)",
                    rusqlite::params![recipe, plan.hash as i64],
                )
                .map_err(sql_error)?;
            }
            [(stored_recipe, stored_hash)]
                if stored_recipe == recipe && *stored_hash as u64 == plan.hash => {}
            [(stored_recipe, stored_hash)] => {
                return Err(Diag::Mismatch {
                    stored_recipe: stored_recipe.clone(),
                    stored_hash: *stored_hash as u64,
                    recipe: recipe.to_string(),
                    hash: plan.hash,
                })
            }
            many => return Err(Diag::Store(format!("meta holds {} rows", many.len()))),
        }
        let ddl: String = rels.iter().map(|rel| rel.create.as_str()).collect();
        tx.execute_batch(&ddl).map_err(sql_error)?;
        tx.commit().map_err(sql_error)?;
        Ok(Sqlite { db, rels })
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
            let mut statement = self.db.prepare(&rel.select).map_err(sql_error)?;
            let rows = statement
                .query_map([], |row| {
                    (0..rel.width)
                        .map(|slot| row.get::<_, i64>(slot).map(|value| value as u64))
                        .collect::<rusqlite::Result<Row>>()
                })
                .map_err(sql_error)?;
            for row in rows {
                changes.push(Change {
                    rel: rel.rel,
                    row: row.map_err(sql_error)?,
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
        tx.commit().map_err(sql_error)?;
        Ok(written)
    }

    /// One statement, read-only and returning columns; anything else (a write, BEGIN, ATTACH, a
    /// second statement) is `Diag::Store`.
    fn query(&self, sql: &str) -> Result<Table, Diag> {
        let mut statement = self.db.prepare(sql).map_err(sql_error)?;
        if !statement.readonly() || statement.column_count() == 0 {
            return Err(Diag::Store(format!(
                "query is not a read-only select: {sql}"
            )));
        }
        let columns: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(String::from)
            .collect();
        let width = columns.len();
        let rows = statement
            .query_map([], |row| {
                (0..width)
                    .map(|slot| row.get_ref(slot).map(cell))
                    .collect::<rusqlite::Result<Vec<Cell>>>()
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(sql_error)?;
        Ok(Table { columns, rows })
    }
}
