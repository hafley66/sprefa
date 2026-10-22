//! SQLite is the relational store: one table per persisted product, INTEGER id columns, text once
//! in `dict`. Writes are one statement per table per frontier (`json_each` over the batch).

use crate::_0_id::Dict;
use rusqlite::Connection;
use std::collections::BTreeSet;

pub type Edge = (u64, u64);

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS dict (id INTEGER PRIMARY KEY, bytes BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS edge (caller INTEGER NOT NULL, callee INTEGER NOT NULL,
  PRIMARY KEY (caller, callee)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS reach (caller INTEGER NOT NULL, callee INTEGER NOT NULL,
  PRIMARY KEY (caller, callee)) WITHOUT ROWID;
CREATE VIEW IF NOT EXISTS reach_text AS
  SELECT a.bytes AS caller, b.bytes AS callee
  FROM reach r JOIN dict a ON a.id = r.caller JOIN dict b ON b.id = r.callee;
";

/// SQLite INTEGER is signed; an id keeps its bits.
fn signed(id: u64) -> i64 {
    id as i64
}

pub fn open(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(SCHEMA)
}

pub fn load_dict(db: &Connection, dict: &mut Dict) -> rusqlite::Result<()> {
    let mut statement = db.prepare("SELECT id, bytes FROM dict")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)? as u64, row.get::<_, Vec<u8>>(1)?))
    })?;
    for row in rows {
        let (id, bytes) = row?;
        dict.load(id, bytes.into_boxed_slice());
    }
    Ok(())
}

pub fn load_pairs(db: &Connection, table: &str) -> rusqlite::Result<BTreeSet<Edge>> {
    let mut statement = db.prepare(&format!("SELECT caller, callee FROM {table}"))?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64))
    })?;
    rows.collect()
}

fn pairs_json(pairs: &[Edge]) -> String {
    let rows: Vec<[i64; 2]> = pairs.iter().map(|(a, b)| [signed(*a), signed(*b)]).collect();
    serde_json::to_string(&rows).expect("ids serialize")
}

/// One frontier's changes, one transaction, one statement per table and sign.
pub struct Frontier<'a> {
    pub dict: Vec<(u64, &'a [u8])>,
    pub edge: &'a [(Edge, isize)],
    pub reach: &'a [(Edge, isize)],
}

pub fn write(db: &mut Connection, frontier: Frontier<'_>) -> rusqlite::Result<usize> {
    let tx = db.transaction()?;
    let mut written = 0;
    if !frontier.dict.is_empty() {
        let rows: Vec<serde_json::Value> = frontier
            .dict
            .iter()
            .map(|(id, bytes)| serde_json::json!([signed(*id), String::from_utf8_lossy(bytes)]))
            .collect();
        written += tx.execute(
            "INSERT INTO dict (id, bytes) SELECT value->>0, CAST(value->>1 AS BLOB) FROM json_each(?1)
             WHERE true ON CONFLICT (id) DO NOTHING",
            [serde_json::to_string(&rows).expect("dict rows serialize")],
        )?;
    }
    for (table, changes) in [("edge", frontier.edge), ("reach", frontier.reach)] {
        let added: Vec<Edge> = changes.iter().filter(|(_, d)| *d > 0).map(|(e, _)| *e).collect();
        let removed: Vec<Edge> = changes.iter().filter(|(_, d)| *d < 0).map(|(e, _)| *e).collect();
        if !removed.is_empty() {
            written += tx.execute(
                &format!(
                    "DELETE FROM {table} WHERE (caller, callee) IN
                     (SELECT value->>0, value->>1 FROM json_each(?1))"
                ),
                [pairs_json(&removed)],
            )?;
        }
        if !added.is_empty() {
            written += tx.execute(
                &format!(
                    "INSERT INTO {table} (caller, callee) SELECT value->>0, value->>1 FROM json_each(?1)"
                ),
                [pairs_json(&added)],
            )?;
        }
    }
    tx.commit()?;
    Ok(written)
}
