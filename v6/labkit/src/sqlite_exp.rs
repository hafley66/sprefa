//! SqliteTemporal — the append-only bitemporal fact table with the store's proven
//! retraction techniques: set-based commit (JSON-batched delta), form-A index-driven
//! UPDATE (drives from d, O(Δ·log n), never scans the live index), close-only-touched
//! keys. Facts on disk, so RSS is O(working set), not O(facts). writes = SQL
//! statements per tick (flat = the N+1 win). total_rows includes closed history.

use crate::{Complexity, Experiment};
use rusqlite::{functions::FunctionFlags, Connection};

fn mixk(k: i64) -> i64 {
    let mut z = (k as u64).wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    (z ^ (z >> 31)) as i64
}

pub struct SqliteTemporal {
    conn: Connection,
    path: std::path::PathBuf,
    rev: i64,
    writes: u64,
}

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Default for SqliteTemporal {
    fn default() -> Self {
        // FILE-backed (the real design: single binary + SQLite file). Facts live on
        // disk, so RSS is O(working set), not O(facts).
        let id = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("labkit_temporal_{}_{id}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut s = Self { conn: Connection::open(&path).unwrap(), path, rev: 0, writes: 0 };
        s.init();
        s
    }
}

impl Drop for SqliteTemporal {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("db-wal"));
        let _ = std::fs::remove_file(self.path.with_extension("db-shm"));
    }
}

impl SqliteTemporal {
    fn init(&mut self) {
        // WAL + cap SQLite's own C heap (invisible to the Rust gun) at ~4GB.
        self.conn
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA soft_heap_limit=4294967296;
                 CREATE TABLE fact(key INTEGER NOT NULL, tt_from INTEGER NOT NULL,
                     tt_to INTEGER, weight INTEGER NOT NULL, PRIMARY KEY(key,tt_from)) WITHOUT ROWID;
                 CREATE INDEX ix_live ON fact(key) WHERE tt_to IS NULL;
                 CREATE TEMP TABLE d(key INTEGER PRIMARY KEY, dw INTEGER);",
            )
            .unwrap();
        self.conn
            .create_aggregate_function("xorhash", 1, FunctionFlags::SQLITE_UTF8, XorHash)
            .unwrap();
    }

    fn commit(&mut self, deltas: &[(i64, i64)]) {
        if deltas.is_empty() {
            return;
        }
        self.rev += 1;
        let r = self.rev;
        let mut json = String::from("[");
        for (i, (k, dw)) in deltas.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!("[{k},{dw}]"));
        }
        json.push(']');
        let tx = self.conn.transaction().unwrap();
        tx.execute("DELETE FROM d", []).unwrap();
        tx.execute(
            "INSERT INTO d(key,dw) SELECT json_extract(value,'$[0]'), sum(json_extract(value,'$[1]'))
             FROM json_each(?1) GROUP BY 1",
            [&json],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO fact(key,tt_from,tt_to,weight)
             SELECT d.key, ?1, NULL, 0 FROM d LEFT JOIN fact f ON f.key=d.key AND f.tt_to IS NULL
             WHERE f.key IS NULL AND d.dw>0",
            [r],
        )
        .unwrap();
        // form-A: correlated subquery + key IN d drives from d (index-probed), O(Δ·log n).
        tx.execute(
            "UPDATE fact SET weight = weight + (SELECT dw FROM d WHERE d.key=fact.key)
             WHERE tt_to IS NULL AND key IN (SELECT key FROM d)",
            [],
        )
        .unwrap();
        // retract = close, restricted to touched keys (never scans the live index).
        tx.execute(
            "UPDATE fact SET tt_to=?1 WHERE tt_to IS NULL AND weight<=0 AND key IN (SELECT key FROM d)",
            [r],
        )
        .unwrap();
        tx.commit().unwrap();
        self.writes += 5; // constant statements per revision regardless of Δ
    }
}

impl Experiment for SqliteTemporal {
    fn name(&self) -> &'static str {
        "sqlite-temporal"
    }
    fn complexity(&self) -> Complexity {
        Complexity { time: "O(Δ·log n)", space: "O(working set) RSS" }
    }
    fn rationale(&self) -> &'static str {
        "the real design: single binary + file-backed SQLite, append-only bitemporal, retract = close interval. Set-based commit (JSON delta -> index-driven UPDATE), facts on disk."
    }
    fn reset(&mut self) {
        *self = SqliteTemporal::default();
    }
    fn setup(&mut self, base: usize) {
        // seed base facts in chunks (set-based, no N+1).
        let mut k = 0i64;
        while (k as usize) < base {
            let end = ((k + 50_000) as usize).min(base) as i64;
            let batch: Vec<(i64, i64)> = (k..end).map(|x| (x, 1)).collect();
            self.commit(&batch);
            k = end;
        }
    }
    fn tick(&mut self, adds: &[i64], removes: &[i64]) {
        let mut deltas: Vec<(i64, i64)> = Vec::with_capacity(adds.len() + removes.len());
        deltas.extend(adds.iter().map(|&k| (k, 1)));
        deltas.extend(removes.iter().map(|&k| (k, -1)));
        self.commit(&deltas);
    }
    fn digest(&self) -> i64 {
        self.conn
            .query_row("SELECT COALESCE(xorhash(key),0) FROM fact WHERE tt_to IS NULL", [], |r| r.get(0))
            .unwrap()
    }
    fn live(&self) -> u64 {
        self.conn
            .query_row("SELECT count(*) FROM fact WHERE tt_to IS NULL", [], |r| r.get::<_, i64>(0))
            .unwrap() as u64
    }
    fn total_rows(&self) -> u64 {
        self.conn.query_row("SELECT count(*) FROM fact", [], |r| r.get::<_, i64>(0)).unwrap() as u64
    }
    fn writes(&self) -> u64 {
        self.writes
    }
    fn plan_snapshot(&self) -> Option<String> {
        let mut out = String::new();
        let q = "UPDATE fact SET weight = weight + (SELECT dw FROM d WHERE d.key=fact.key) \
                 WHERE tt_to IS NULL AND key IN (SELECT key FROM d)";
        let mut stmt = self.conn.prepare(&format!("EXPLAIN QUERY PLAN {q}")).ok()?;
        let rows = stmt
            .query_map([], |r| Ok(format!("    {}", r.get::<_, String>(3)?)))
            .ok()?;
        for row in rows.flatten() {
            out.push_str(&row);
            out.push('\n');
        }
        Some(out)
    }
}

struct XorHash;
impl rusqlite::functions::Aggregate<i64, i64> for XorHash {
    fn init(&self, _: &mut rusqlite::functions::Context<'_>) -> rusqlite::Result<i64> {
        Ok(0)
    }
    fn step(&self, ctx: &mut rusqlite::functions::Context<'_>, acc: &mut i64) -> rusqlite::Result<()> {
        *acc ^= mixk(ctx.get::<i64>(0)?);
        Ok(())
    }
    fn finalize(&self, _: &mut rusqlite::functions::Context<'_>, acc: Option<i64>) -> rusqlite::Result<i64> {
        Ok(acc.unwrap_or(0))
    }
}
