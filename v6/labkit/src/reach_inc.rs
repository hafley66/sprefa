//! SqliteReachInc — the "sqlite dd": INCREMENTAL single-source reachability on disk,
//! maintained under edge inserts AND deletes, no from-scratch recompute. This is the
//! full incremental recursive fixpoint the CAPABILITY-MAP calls gap #2, built from two
//! symmetric frontier cascades:
//!
//!   - ADD:    an edge into a reachable node can make its target newly reachable, which
//!             propagates forward — a forward semi-naive cascade (support count += ...).
//!   - DELETE: the store's `cascade.rs` retraction — a node loses a support; if it was
//!             its LAST, the node dies and the loss cascades forward (support count -= ...).
//!
//! weight(node) = (# reachable in-neighbors) + is_root. reachable iff weight > 0. This
//! counting model is SOUND ON A DAG (a cycle would mutually self-support and never die —
//! that needs full DRed over-delete/rederive; the head-to-head keeps the graph a DAG and
//! says so). Mirrors dd's own `examples/bfs.rs`: roots + edges, batched insert/delete
//! rounds. dd (resident) and a RAM BFS are the oracles.

use rusqlite::{functions::FunctionFlags, Connection};
use std::collections::{HashMap, HashSet, VecDeque};

pub struct SqliteReachInc {
    conn: Connection,
    path: std::path::PathBuf,
    stmts: u64,
    rounds: u64,
}

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Drop for SqliteReachInc {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("db-wal"));
        let _ = std::fs::remove_file(self.path.with_extension("db-shm"));
    }
}

impl SqliteReachInc {
    pub fn new() -> Self {
        let id = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("labkit_reachinc_{}_{id}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA soft_heap_limit=4294967296;
             CREATE TABLE node(key INTEGER PRIMARY KEY, weight INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE edge(u INTEGER NOT NULL, v INTEGER NOT NULL, PRIMARY KEY(u,v)) WITHOUT ROWID;
             CREATE TABLE cx_frontier(key INTEGER PRIMARY KEY);
             CREATE TABLE cx_next(key INTEGER PRIMARY KEY);
             CREATE TABLE cx_hits(key INTEGER PRIMARY KEY, dec INTEGER NOT NULL);",
        )
        .unwrap();
        conn.create_aggregate_function("xorhash", 1, FunctionFlags::SQLITE_UTF8, XorHash).unwrap();
        Self { conn, path, stmts: 0, rounds: 0 }
    }

    fn weight(&self, k: i64) -> i64 {
        self.conn
            .query_row("SELECT weight FROM node WHERE key=?1", [k], |r| r.get(0))
            .unwrap_or(0)
    }

    /// Seed nodes have already had their own weight updated + crossed the threshold;
    /// propagate the consequence forward. add=true increments children, add=false
    /// decrements. Each round = a fixed set of set-based statements over the frontier.
    fn run_cascade(tx: &rusqlite::Transaction, seeds: &[i64], add: bool, stmts: &mut u64, rounds: &mut u64) {
        tx.execute_batch("DELETE FROM cx_frontier; DELETE FROM cx_next;").unwrap();
        *stmts += 1;
        let vals: Vec<String> = seeds.iter().map(|k| format!("({k})")).collect();
        if !vals.is_empty() {
            tx.execute_batch(&format!("INSERT OR IGNORE INTO cx_frontier(key) VALUES {}", vals.join(",")))
                .unwrap();
            *stmts += 1;
        }
        loop {
            let n: i64 = tx.query_row("SELECT count(*) FROM cx_frontier", [], |r| r.get(0)).unwrap();
            *stmts += 1;
            if n == 0 {
                break;
            }
            *rounds += 1;
            // hits: each child + how many frontier parents touch it this round.
            tx.execute_batch(
                "DELETE FROM cx_hits;
                 INSERT INTO cx_hits(key,dec)
                 SELECT e.v, count(*) FROM cx_frontier f CROSS JOIN edge e ON e.u=f.key
                 GROUP BY e.v;",
            )
            .unwrap();
            *stmts += 2;
            if add {
                tx.execute_batch(
                    "UPDATE node SET weight = weight + (SELECT dec FROM cx_hits h WHERE h.key=node.key)
                     WHERE key IN (SELECT key FROM cx_hits);
                     DELETE FROM cx_next;
                     INSERT INTO cx_next(key)
                     SELECT h.key FROM cx_hits h CROSS JOIN node r ON r.key=h.key
                     WHERE r.weight > 0 AND r.weight - h.dec <= 0;",
                )
                .unwrap();
            } else {
                tx.execute_batch(
                    "UPDATE node SET weight = weight - (SELECT dec FROM cx_hits h WHERE h.key=node.key)
                     WHERE key IN (SELECT key FROM cx_hits);
                     DELETE FROM cx_next;
                     INSERT INTO cx_next(key)
                     SELECT h.key FROM cx_hits h CROSS JOIN node r ON r.key=h.key
                     WHERE r.weight <= 0 AND r.weight + h.dec > 0;",
                )
                .unwrap();
            }
            *stmts += 3;
            tx.execute_batch("DELETE FROM cx_frontier; INSERT INTO cx_frontier SELECT key FROM cx_next;").unwrap();
            *stmts += 2;
        }
    }

    /// Load roots (weight 1) + all initial edges, then compute reachability once by
    /// cascading forward from the roots.
    pub fn setup(&mut self, roots: &[i64], edges: &[(i64, i64)]) {
        let tx = self.conn.transaction().unwrap();
        // node rows for every endpoint + every root (weight 0), then roots += 1.
        let mut keys: HashSet<i64> = HashSet::new();
        for &(u, v) in edges {
            keys.insert(u);
            keys.insert(v);
        }
        for &r in roots {
            keys.insert(r);
        }
        for chunk in keys.iter().copied().collect::<Vec<_>>().chunks(4000) {
            let vals: Vec<String> = chunk.iter().map(|k| format!("({k},0)")).collect();
            tx.execute_batch(&format!("INSERT OR IGNORE INTO node(key,weight) VALUES {}", vals.join(",")))
                .unwrap();
        }
        for chunk in edges.chunks(4000) {
            let vals: Vec<String> = chunk.iter().map(|(u, v)| format!("({u},{v})")).collect();
            tx.execute_batch(&format!("INSERT OR IGNORE INTO edge(u,v) VALUES {}", vals.join(",")))
                .unwrap();
        }
        let rvals: Vec<String> = roots.iter().map(|r| r.to_string()).collect();
        tx.execute_batch(&format!("UPDATE node SET weight=weight+1 WHERE key IN ({})", rvals.join(",")))
            .unwrap();
        Self::run_cascade(&tx, roots, true, &mut self.stmts, &mut self.rounds);
        tx.commit().unwrap();
    }

    pub fn add_edge(&mut self, u: i64, v: i64) {
        let wu = self.weight(u);
        let tx = self.conn.transaction().unwrap();
        tx.execute_batch(&format!(
            "INSERT OR IGNORE INTO node(key,weight) VALUES ({u},0),({v},0);
             INSERT OR IGNORE INTO edge(u,v) VALUES ({u},{v});"
        ))
        .unwrap();
        self.stmts += 2;
        if wu > 0 {
            let wv_before: i64 = tx.query_row("SELECT weight FROM node WHERE key=?1", [v], |r| r.get(0)).unwrap();
            tx.execute(&format!("UPDATE node SET weight=weight+1 WHERE key={v}"), []).unwrap();
            self.stmts += 1;
            if wv_before <= 0 {
                Self::run_cascade(&tx, &[v], true, &mut self.stmts, &mut self.rounds);
            }
        }
        tx.commit().unwrap();
    }

    pub fn del_edge(&mut self, u: i64, v: i64) {
        let wu = self.weight(u);
        let tx = self.conn.transaction().unwrap();
        tx.execute(&format!("DELETE FROM edge WHERE u={u} AND v={v}"), []).unwrap();
        self.stmts += 1;
        if wu > 0 {
            let wv_before: i64 = tx.query_row("SELECT weight FROM node WHERE key=?1", [v], |r| r.get(0)).unwrap();
            tx.execute(&format!("UPDATE node SET weight=weight-1 WHERE key={v}"), []).unwrap();
            self.stmts += 1;
            if wv_before > 0 && wv_before - 1 <= 0 {
                Self::run_cascade(&tx, &[v], false, &mut self.stmts, &mut self.rounds);
            }
        }
        tx.commit().unwrap();
    }

    /// (digest, count) over reachable nodes (weight > 0) — the equivalence key.
    pub fn reachable(&self) -> (i64, u64) {
        self.conn
            .query_row(
                "SELECT COALESCE(xorhash(key),0), count(*) FROM node WHERE weight>0",
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
}

impl Default for SqliteReachInc {
    fn default() -> Self {
        Self::new()
    }
}

/// Independent RAM oracle: single-source reachable set (roots + everything reachable via
/// >=1 edge), by BFS over the CURRENT edge set. Never touches SQLite.
pub fn reach_oracle(roots: &[i64], edges: &HashSet<(i64, i64)>) -> (i64, u64) {
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for &(u, v) in edges {
        adj.entry(u).or_default().push(v);
    }
    let mut seen: HashSet<i64> = HashSet::new();
    let mut q: VecDeque<i64> = VecDeque::new();
    for &r in roots {
        if seen.insert(r) {
            q.push_back(r);
        }
    }
    while let Some(x) = q.pop_front() {
        if let Some(vs) = adj.get(&x) {
            for &w in vs {
                if seen.insert(w) {
                    q.push_back(w);
                }
            }
        }
    }
    let mut digest = 0i64;
    for &k in &seen {
        digest ^= mix(k);
    }
    (digest, seen.len() as u64)
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
