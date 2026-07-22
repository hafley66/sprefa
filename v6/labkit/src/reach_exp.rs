//! Reach engines: maintain the live call graph, answer the blast-radius (all-pairs
//! transitive closure) each tick. ram-reach and sqlite-reach both RECOMPUTE the
//! closure from scratch each tick — the honest baseline whose bad scaling is the
//! motivation for an incremental engine (dd, or the store's semi-naive cascade).

use crate::reach::{dec, initial_edges, reach_digest, MUL};
use crate::{Complexity, Experiment};
use rusqlite::{functions::FunctionFlags, Connection};
use std::collections::{HashMap, HashSet};

// ---- ram-reach: recompute all-pairs TC each tick (the reference) --------------
#[derive(Default)]
pub struct RamReach {
    weight: HashMap<i64, i64>,
    last_digest: i64,
    last_card: u64,
    recomputes: u64,
}

impl RamReach {
    fn recompute(&mut self) {
        let live: HashSet<i64> = self.weight.iter().filter(|(_, &w)| w > 0).map(|(&k, _)| k).collect();
        let (d, c) = reach_digest(&live);
        self.last_digest = d;
        self.last_card = c;
        self.recomputes += 1;
    }
}

impl Experiment for RamReach {
    fn name(&self) -> &'static str {
        "ram-reach"
    }
    fn complexity(&self) -> Complexity {
        Complexity { time: "O(V·E)/tick", space: "O(facts) resident" }
    }
    fn rationale(&self) -> &'static str {
        "reference blast-radius: hold edges resident, recompute all-pairs reachability from scratch each edit via BFS. The trusted oracle for the closure, and the naive-recompute baseline."
    }
    fn reset(&mut self) {
        *self = RamReach::default();
    }
    fn setup(&mut self, base: usize) {
        for k in initial_edges(base) {
            *self.weight.entry(k).or_insert(0) += 1;
        }
        self.recompute();
    }
    fn tick(&mut self, adds: &[i64], removes: &[i64]) {
        for &k in adds {
            *self.weight.entry(k).or_insert(0) += 1;
        }
        for &k in removes {
            if let Some(w) = self.weight.get_mut(&k) {
                *w -= 1;
                if *w <= 0 {
                    self.weight.remove(&k);
                }
            }
        }
        self.recompute();
    }
    fn digest(&self) -> i64 {
        self.last_digest
    }
    fn live(&self) -> u64 {
        self.last_card // reachable-pair count (the answer's cardinality)
    }
    fn recompute_units(&self) -> u64 {
        self.recomputes
    }
}

// ---- sqlite-reach: recursive-CTE recompute each tick --------------------------
pub struct SqliteReach {
    conn: Connection,
    writes: u64,
    recomputes: u64,
    last: i64,
    card: u64,
}

impl Default for SqliteReach {
    fn default() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA soft_heap_limit=4294967296;
             CREATE TABLE fact(key INTEGER PRIMARY KEY, weight INTEGER NOT NULL);",
        )
        .unwrap();
        conn.create_aggregate_function("xorpair", 2, FunctionFlags::SQLITE_UTF8, XorPair).unwrap();
        Self { conn, writes: 0, recomputes: 0, last: 0, card: 0 }
    }
}

impl SqliteReach {
    fn apply(&mut self, deltas: &[(i64, i64)]) {
        if deltas.is_empty() {
            return;
        }
        let mut json = String::from("[");
        for (i, (k, dw)) in deltas.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!("[{k},{dw}]"));
        }
        json.push(']');
        let tx = self.conn.transaction().unwrap();
        tx.execute(
            "INSERT INTO fact(key,weight)
             SELECT json_extract(value,'$[0]'), sum(json_extract(value,'$[1]')) FROM json_each(?1) GROUP BY 1
             ON CONFLICT(key) DO UPDATE SET weight=weight+excluded.weight",
            [&json],
        )
        .unwrap();
        tx.execute("DELETE FROM fact WHERE weight<=0", []).unwrap();
        tx.commit().unwrap();
        self.writes += 2;
    }

    /// Recompute all-pairs reach via a recursive CTE, keyed by the edge encoding: an
    /// edge u->v is one row key=u*MUL+v, so u's out-edges are the key-range
    /// [u*MUL, (u+1)*MUL) — a PK range scan, no extra index.
    fn recompute(&mut self) {
        let sql = format!(
            "WITH RECURSIVE r(src,cur) AS (
                 SELECT key/{MUL}, key%{MUL} FROM fact
                 UNION
                 SELECT r.src, f.key%{MUL} FROM r
                   JOIN fact f ON f.key >= r.cur*{MUL} AND f.key < (r.cur+1)*{MUL}
             )
             SELECT COALESCE(xorpair(src,cur),0), count(*) FROM r"
        );
        let (d, c): (i64, i64) =
            self.conn.query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        self.last = d;
        self.card = c as u64;
        self.recomputes += 1;
    }
}

impl Experiment for SqliteReach {
    fn name(&self) -> &'static str {
        "sqlite-reach"
    }
    fn complexity(&self) -> Complexity {
        Complexity { time: "O(reach)/tick recompute", space: "O(working set)" }
    }
    fn rationale(&self) -> &'static str {
        "blast-radius on SQLite: edges in a table, recompute the closure each edit with a recursive CTE (edge encoded so u's out-edges are a PK range scan). Zero-dependency baseline."
    }
    fn reset(&mut self) {
        *self = SqliteReach::default();
    }
    fn setup(&mut self, base: usize) {
        let init: Vec<(i64, i64)> = initial_edges(base).into_iter().map(|k| (k, 1)).collect();
        for chunk in init.chunks(50_000) {
            self.apply(chunk);
        }
        self.recompute();
    }
    fn tick(&mut self, adds: &[i64], removes: &[i64]) {
        let mut deltas: Vec<(i64, i64)> = Vec::new();
        deltas.extend(adds.iter().map(|&k| (k, 1)));
        deltas.extend(removes.iter().map(|&k| (k, -1)));
        self.apply(&deltas);
        self.recompute();
    }
    fn digest(&self) -> i64 {
        self.last
    }
    fn live(&self) -> u64 {
        self.card
    }
    fn recompute_units(&self) -> u64 {
        self.recomputes
    }
    fn writes(&self) -> u64 {
        self.writes
    }
    fn plan_snapshot(&self) -> Option<String> {
        let _ = dec(0);
        Some("    recursive CTE over fact; step joins by PK key-range [cur*MUL,(cur+1)*MUL)\n".into())
    }
}

struct XorPair;
impl rusqlite::functions::Aggregate<i64, i64> for XorPair {
    fn init(&self, _: &mut rusqlite::functions::Context<'_>) -> rusqlite::Result<i64> {
        Ok(0)
    }
    fn step(&self, ctx: &mut rusqlite::functions::Context<'_>, acc: &mut i64) -> rusqlite::Result<()> {
        let src: i64 = ctx.get(0)?;
        let cur: i64 = ctx.get(1)?;
        *acc ^= crate::mix(src * MUL + cur);
        Ok(())
    }
    fn finalize(&self, _: &mut rusqlite::functions::Context<'_>, acc: Option<i64>) -> rusqlite::Result<i64> {
        Ok(acc.unwrap_or(0))
    }
}
