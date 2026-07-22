//! CascadeZset — the store's proven Z-set retraction cascade
//! (`sprefa-store::cascade`), ported to labkit's sync rusqlite. This is NOT a new
//! algorithm: same frontier -> hits -> next fixpoint, same set-based SQL, same
//! transition guard (a node enters the frontier exactly once), same CROSS JOIN to pin
//! the join order so every step is SCAN <small> -> SEARCH <big> USING PRIMARY KEY.
//! It is the "feldera in sqlite" the store already paid for; labkit runs it under the
//! gun against an independent RAM oracle.
//!
//! weight = number of supports (derivations). Retraction is a Z-set subtraction: a row
//! dies only when its LAST support is gone (weight reaches 0) — a row supported two ways
//! survives losing one. That is why this is not naive reachability: a child dies when its
//! last parent dies, never its first.
//!
//! Each round is a fixed handful of set-based statements over the whole frontier, so the
//! statement count is O(rounds = DAG depth), never O(rows). That is the delta-proportional
//! property: work scales with the wavefront, not the corpus.

use rusqlite::{functions::FunctionFlags, Connection};

const CHUNK: usize = 4000;

pub struct CascadeZset {
    conn: Connection,
    path: std::path::PathBuf,
    stmts: u64,
    rounds: u64,
}

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Drop for CascadeZset {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("db-wal"));
        let _ = std::fs::remove_file(self.path.with_extension("db-shm"));
    }
}

impl Default for CascadeZset {
    fn default() -> Self {
        Self::new()
    }
}

impl CascadeZset {
    pub fn new() -> Self {
        let id = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("labkit_czset_{}_{id}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        // Same shape as store::cascade::create_schema. cx_row is a ROWID table clustered
        // on `key` (INTEGER PRIMARY KEY = rowid alias, zero PK storage). cx_dep is a
        // 2-column WITHOUT ROWID key, parent-prefix-ordered for the delta traversal.
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA soft_heap_limit=4294967296;
             CREATE TABLE cx_row (key INTEGER PRIMARY KEY, weight INTEGER NOT NULL DEFAULT 1);
             CREATE TABLE cx_dep (parent_key INTEGER NOT NULL, child_key INTEGER NOT NULL,
                 PRIMARY KEY (parent_key, child_key)) WITHOUT ROWID;
             CREATE TABLE cx_frontier (key INTEGER PRIMARY KEY);
             CREATE TABLE cx_next     (key INTEGER PRIMARY KEY);
             CREATE TABLE cx_hits (key INTEGER PRIMARY KEY, dec INTEGER NOT NULL);",
        )
        .unwrap();
        conn.create_aggregate_function("xorhash", 1, FunctionFlags::SQLITE_UTF8, XorHash)
            .unwrap();
        Self { conn, path, stmts: 0, rounds: 0 }
    }

    fn exec(&mut self, sql: &str) {
        self.stmts += 1;
        self.conn.execute_batch(sql).unwrap();
    }

    /// Batch-insert `(key, weight)` rows. One transaction = one WAL commit.
    pub fn insert_rows(&mut self, rows: &[(i64, i64)]) {
        let tx = self.conn.transaction().unwrap();
        for chunk in rows.chunks(CHUNK) {
            let vals: Vec<String> = chunk.iter().map(|(k, w)| format!("({k},{w})")).collect();
            tx.execute_batch(&format!("INSERT INTO cx_row(key,weight) VALUES {}", vals.join(",")))
                .unwrap();
        }
        tx.commit().unwrap();
    }

    /// Batch-insert dependency edges `(parent_key, child_key)`.
    pub fn insert_deps(&mut self, edges: &[(i64, i64)]) {
        let tx = self.conn.transaction().unwrap();
        for chunk in edges.chunks(CHUNK) {
            let vals: Vec<String> = chunk.iter().map(|(p, c)| format!("({p},{c})")).collect();
            tx.execute_batch(&format!(
                "INSERT INTO cx_dep(parent_key,child_key) VALUES {}",
                vals.join(",")
            ))
            .unwrap();
        }
        tx.commit().unwrap();
    }

    /// Retract `seeds` (each loses one unit of weight) and cascade. Returns the number of
    /// rounds (= depth reached). The WHOLE cascade is ONE transaction (one WAL commit for
    /// every round); every statement is driven from the small working set into the big
    /// tables via PRIMARY KEY, so work scales with the wavefront, not the corpus.
    pub fn retract(&mut self, seeds: &[i64]) -> u64 {
        let seed_in = {
            let v: Vec<String> = seeds.iter().map(|k| k.to_string()).collect();
            format!("({})", v.join(","))
        };
        self.rounds = 0;
        self.exec("DELETE FROM cx_frontier");
        self.exec("DELETE FROM cx_next");
        self.exec(&format!("UPDATE cx_row SET weight = weight - 1 WHERE key IN {seed_in}"));
        self.exec(&format!(
            "INSERT INTO cx_frontier SELECT key FROM cx_row WHERE key IN {seed_in} AND weight <= 0"
        ));

        loop {
            let n: i64 = self
                .conn
                .query_row("SELECT count(*) FROM cx_frontier", [], |r| r.get(0))
                .unwrap();
            self.stmts += 1;
            if n == 0 {
                break;
            }
            self.rounds += 1;

            // 1. hits = frontier's children + how many supports each loses now.
            self.exec("DELETE FROM cx_hits");
            self.exec(
                "INSERT INTO cx_hits(key,dec) \
                 SELECT d.child_key, count(*) \
                 FROM cx_frontier f CROSS JOIN cx_dep d ON d.parent_key = f.key \
                 GROUP BY d.child_key",
            );
            // 2. decrement each hit child by its lost-support count (indexed by rowid).
            self.exec(
                "UPDATE cx_row SET weight = weight - \
                    (SELECT dec FROM cx_hits h WHERE h.key = cx_row.key) \
                 WHERE key IN (SELECT key FROM cx_hits)",
            );
            // 3. next frontier = hits that CROSSED zero THIS round (dead now, alive before).
            //    The transition guard means a node enters the frontier exactly once.
            self.exec("DELETE FROM cx_next");
            self.exec(
                "INSERT INTO cx_next(key) \
                 SELECT h.key FROM cx_hits h CROSS JOIN cx_row r ON r.key = h.key \
                 WHERE r.weight <= 0 AND r.weight + h.dec > 0",
            );
            // 4. frontier <- next. Dead rows STAY in cx_row (weight <= 0).
            self.exec("DELETE FROM cx_frontier");
            self.exec("INSERT INTO cx_frontier SELECT key FROM cx_next");
        }
        self.rounds
    }

    /// (digest, count) over the SURVIVORS (weight > 0) — the equivalence key vs the oracle.
    pub fn survivors(&self) -> (i64, u64) {
        self.conn
            .query_row(
                "SELECT COALESCE(xorhash(key),0), count(*) FROM cx_row WHERE weight > 0",
                [],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)),
            )
            .unwrap()
    }

    pub fn statements(&self) -> u64 {
        self.stmts
    }
    pub fn rounds(&self) -> u64 {
        self.rounds
    }
    /// On-disk file size in MB (page_count * page_size) — where the data actually lives.
    pub fn db_size_mb(&self) -> f64 {
        let pages: i64 = self.conn.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap_or(0);
        let ps: i64 = self.conn.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap_or(0);
        (pages * ps) as f64 / (1024.0 * 1024.0)
    }
}

/// The independent RAM oracle: the SAME Z-set cascade in plain Rust. Given initial
/// weights, dep edges (parent -> child), and seeds, returns the surviving (digest, count).
/// This is what a correct engine must match — it never touches SQLite.
pub fn cascade_oracle(
    weights: &[(i64, i64)],
    deps: &[(i64, i64)],
    seeds: &[i64],
) -> (i64, u64) {
    use std::collections::HashMap;
    let mut w: HashMap<i64, i64> = weights.iter().copied().collect();
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    for &(p, c) in deps {
        children.entry(p).or_default().push(c);
    }
    let mut frontier: Vec<i64> = Vec::new();
    for &s in seeds {
        if let Some(x) = w.get_mut(&s) {
            *x -= 1;
            if *x <= 0 {
                frontier.push(s);
            }
        }
    }
    while !frontier.is_empty() {
        // hits: child -> count of dead-parent supports lost this round
        let mut hits: HashMap<i64, i64> = HashMap::new();
        for &f in &frontier {
            if let Some(cs) = children.get(&f) {
                for &c in cs {
                    *hits.entry(c).or_insert(0) += 1;
                }
            }
        }
        let mut next: Vec<i64> = Vec::new();
        for (c, dec) in hits {
            let before = *w.get(&c).unwrap_or(&0);
            let after = before - dec;
            w.insert(c, after);
            if after <= 0 && before > 0 {
                next.push(c); // crossed zero this round (transition guard)
            }
        }
        frontier = next;
    }
    let mut digest = 0i64;
    let mut count = 0u64;
    for (&k, &wt) in &w {
        if wt > 0 {
            digest ^= mix(k);
            count += 1;
        }
    }
    (digest, count)
}

fn mix(k: i64) -> i64 {
    let mut z = (k as u64).wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    (z ^ (z >> 31)) as i64
}

struct XorHash;
impl rusqlite::functions::Aggregate<i64, i64> for XorHash {
    fn init(&self, _: &mut rusqlite::functions::Context<'_>) -> rusqlite::Result<i64> {
        Ok(0)
    }
    fn step(&self, ctx: &mut rusqlite::functions::Context<'_>, acc: &mut i64) -> rusqlite::Result<()> {
        let k: i64 = ctx.get(0)?;
        *acc ^= mix(k);
        Ok(())
    }
    fn finalize(&self, _: &mut rusqlite::functions::Context<'_>, acc: Option<i64>) -> rusqlite::Result<i64> {
        Ok(acc.unwrap_or(0))
    }
}
