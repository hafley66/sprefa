//! SqliteReachDRed — the CYCLE-SAFE sqlite-dd: incremental single-source reachability on
//! disk that survives cycles, via Delete-and-Rederive (Gupta-Mumick-Subrahmanian).
//!
//! The counting model (reach_inc) fails on a cycle: members mutually support each other
//! and never hit weight 0 even after the root is cut. DRed sidesteps counts entirely with
//! a boolean `reached` flag and two phases on delete:
//!
//!   over-delete: from the targets of deleted edges, walk FORWARD and tentatively unreach
//!                the whole reached cone (cycles included) — an over-approximation.
//!   rederive:    any cone node still reachable from a node OUTSIDE the cone (or a root)
//!                is re-marked reached and propagated forward. A dead cycle has no such
//!                in-edge, so it stays unreached. Correct on any graph.
//!
//! Adds are monotonic (a forward cascade), cycle-safe already. A round = del_batch then
//! add_batch, each correct for its own edge set. DRed does more work than the true delta
//! (that is its known cost), but it is the general-graph engine dd's `iterate` gives free.

use rusqlite::{functions::FunctionFlags, Connection};

pub struct SqliteReachDRed {
    conn: Connection,
    path: std::path::PathBuf,
    stmts: u64,
    rounds: u64,
}

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Drop for SqliteReachDRed {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("db-wal"));
        let _ = std::fs::remove_file(self.path.with_extension("db-shm"));
    }
}

impl Default for SqliteReachDRed {
    fn default() -> Self {
        Self::new()
    }
}

impl SqliteReachDRed {
    pub fn new() -> Self {
        let id = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("labkit_dred_{}_{id}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            // store's feldera-sqlite tuning: WAL + synchronous=NORMAL + temp_store=MEMORY,
            // plus a real page cache and mmap so the cone traversal reads node/edge from
            // RAM instead of the disk file. The churny working tables are TEMP so, with
            // temp_store=MEMORY, they live in RAM and are never WAL-logged per round.
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA temp_store=MEMORY;
             PRAGMA cache_size=-262144;      -- 256 MB page cache (default was ~2 MB)
             PRAGMA mmap_size=1073741824;    -- 1 GB mmap for reads
             CREATE TABLE node(key INTEGER PRIMARY KEY, reached INTEGER NOT NULL DEFAULT 0,
                 is_root INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE edge(u INTEGER NOT NULL, v INTEGER NOT NULL, PRIMARY KEY(u,v)) WITHOUT ROWID;
             CREATE INDEX ix_ev ON edge(v);           -- reverse lookups for rederive
             CREATE INDEX ix_reached ON node(reached);
             CREATE TEMP TABLE cx_frontier(key INTEGER PRIMARY KEY);
             CREATE TEMP TABLE cx_next(key INTEGER PRIMARY KEY);
             CREATE TEMP TABLE cx_cone(key INTEGER PRIMARY KEY);
             CREATE TEMP TABLE cx_batch(u INTEGER, v INTEGER);",
        )
        .unwrap();
        conn.create_aggregate_function("xorhash", 1, FunctionFlags::SQLITE_UTF8, XorHash).unwrap();
        Self { conn, path, stmts: 0, rounds: 0 }
    }

    fn exec(tx: &rusqlite::Transaction, sql: &str, stmts: &mut u64) {
        *stmts += 1;
        tx.execute_batch(sql).unwrap();
    }
    fn count(tx: &rusqlite::Transaction, sql: &str, stmts: &mut u64) -> i64 {
        *stmts += 1;
        tx.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn load_batch(tx: &rusqlite::Transaction, edges: &[(i64, i64)], stmts: &mut u64) {
        Self::exec(tx, "DELETE FROM cx_batch", stmts);
        for chunk in edges.chunks(4000) {
            let vals: Vec<String> = chunk.iter().map(|(u, v)| format!("({u},{v})")).collect();
            Self::exec(tx, &format!("INSERT INTO cx_batch(u,v) VALUES {}", vals.join(",")), stmts);
        }
    }

    pub fn setup(&mut self, roots: &[i64], edges: &[(i64, i64)]) {
        let tx = self.conn.transaction().unwrap();
        // node rows for every endpoint + roots
        Self::load_batch(&tx, edges, &mut self.stmts);
        Self::exec(&tx,
            "INSERT OR IGNORE INTO node(key) SELECT u FROM cx_batch;
             INSERT OR IGNORE INTO node(key) SELECT v FROM cx_batch;", &mut self.stmts);
        let rvals: Vec<String> = roots.iter().map(|r| format!("({r})")).collect();
        if !rvals.is_empty() {
            Self::exec(&tx, &format!("INSERT OR IGNORE INTO node(key) VALUES {}", rvals.join(",")), &mut self.stmts);
            let rin: Vec<String> = roots.iter().map(|r| r.to_string()).collect();
            Self::exec(&tx, &format!("UPDATE node SET reached=1, is_root=1 WHERE key IN ({})", rin.join(",")), &mut self.stmts);
        }
        for chunk in edges.chunks(4000) {
            let vals: Vec<String> = chunk.iter().map(|(u, v)| format!("({u},{v})")).collect();
            Self::exec(&tx, &format!("INSERT OR IGNORE INTO edge(u,v) VALUES {}", vals.join(",")), &mut self.stmts);
        }
        // initial reachability = forward cascade from roots
        Self::exec(&tx, "DELETE FROM cx_frontier; DELETE FROM cx_next;", &mut self.stmts);
        let rin: Vec<String> = roots.iter().map(|r| format!("({r})")).collect();
        Self::exec(&tx, &format!("INSERT OR IGNORE INTO cx_frontier(key) VALUES {}", rin.join(",")), &mut self.stmts);
        Self::forward_reach(&tx, &mut self.stmts, &mut self.rounds);
        tx.commit().unwrap();
    }

    /// Forward cascade: mark every node reachable from the current frontier as reached=1.
    /// Assumes frontier already reached. Cycle-safe (monotonic; stops when no new node).
    fn forward_reach(tx: &rusqlite::Transaction, stmts: &mut u64, rounds: &mut u64) {
        loop {
            Self::exec(tx,
                "DELETE FROM cx_next;
                 INSERT INTO cx_next(key)
                 SELECT DISTINCT e.v FROM cx_frontier f CROSS JOIN edge e ON e.u=f.key
                   CROSS JOIN node w ON w.key=e.v WHERE w.reached=0;", stmts);
            if Self::count(tx, "SELECT count(*) FROM cx_next", stmts) == 0 {
                break;
            }
            *rounds += 1;
            Self::exec(tx, "UPDATE node SET reached=1 WHERE key IN (SELECT key FROM cx_next);", stmts);
            Self::exec(tx, "DELETE FROM cx_frontier; INSERT INTO cx_frontier SELECT key FROM cx_next;", stmts);
        }
    }

    pub fn add_batch(&mut self, adds: &[(i64, i64)]) {
        if adds.is_empty() {
            return;
        }
        let tx = self.conn.transaction().unwrap();
        Self::load_batch(&tx, adds, &mut self.stmts);
        Self::exec(&tx,
            "INSERT OR IGNORE INTO node(key) SELECT u FROM cx_batch;
             INSERT OR IGNORE INTO node(key) SELECT v FROM cx_batch;", &mut self.stmts);
        for chunk in adds.chunks(4000) {
            let vals: Vec<String> = chunk.iter().map(|(u, v)| format!("({u},{v})")).collect();
            Self::exec(&tx, &format!("INSERT OR IGNORE INTO edge(u,v) VALUES {}", vals.join(",")), &mut self.stmts);
        }
        // seed = targets of added edges whose source is reached but target is not.
        Self::exec(&tx,
            "DELETE FROM cx_frontier; DELETE FROM cx_next;
             INSERT OR IGNORE INTO cx_frontier(key)
             SELECT DISTINCT b.v FROM cx_batch b JOIN node nu ON nu.key=b.u JOIN node nv ON nv.key=b.v
             WHERE nu.reached=1 AND nv.reached=0;", &mut self.stmts);
        Self::exec(&tx, "UPDATE node SET reached=1 WHERE key IN (SELECT key FROM cx_frontier);", &mut self.stmts);
        Self::forward_reach(&tx, &mut self.stmts, &mut self.rounds);
        tx.commit().unwrap();
    }

    pub fn del_batch(&mut self, dels: &[(i64, i64)]) {
        if dels.is_empty() {
            return;
        }
        let tx = self.conn.transaction().unwrap();
        Self::load_batch(&tx, dels, &mut self.stmts);
        // 1. apply deletions to the edge table
        Self::exec(&tx,
            "DELETE FROM edge WHERE (u,v) IN (SELECT u,v FROM cx_batch);", &mut self.stmts);
        // 2. over-delete: seed = deleted-edge targets that were reached (non-root) whose
        //    source was reached — they may have lost a support.
        Self::exec(&tx,
            "DELETE FROM cx_frontier; DELETE FROM cx_next; DELETE FROM cx_cone;
             INSERT OR IGNORE INTO cx_frontier(key)
             SELECT DISTINCT b.v FROM cx_batch b JOIN node nu ON nu.key=b.u JOIN node nv ON nv.key=b.v
             WHERE nu.reached=1 AND nv.reached=1 AND nv.is_root=0;", &mut self.stmts);
        Self::exec(&tx, "UPDATE node SET reached=0 WHERE key IN (SELECT key FROM cx_frontier);", &mut self.stmts);
        Self::exec(&tx, "INSERT OR IGNORE INTO cx_cone(key) SELECT key FROM cx_frontier;", &mut self.stmts);
        // walk the reached forward cone, unreaching it (roots excepted).
        loop {
            Self::exec(&tx,
                "DELETE FROM cx_next;
                 INSERT INTO cx_next(key)
                 SELECT DISTINCT e.v FROM cx_frontier f CROSS JOIN edge e ON e.u=f.key
                   CROSS JOIN node w ON w.key=e.v WHERE w.reached=1 AND w.is_root=0;", &mut self.stmts);
            if Self::count(&tx, "SELECT count(*) FROM cx_next", &mut self.stmts) == 0 {
                break;
            }
            self.rounds += 1;
            Self::exec(&tx, "UPDATE node SET reached=0 WHERE key IN (SELECT key FROM cx_next);", &mut self.stmts);
            Self::exec(&tx, "INSERT OR IGNORE INTO cx_cone(key) SELECT key FROM cx_next;", &mut self.stmts);
            Self::exec(&tx, "DELETE FROM cx_frontier; INSERT INTO cx_frontier SELECT key FROM cx_next;", &mut self.stmts);
        }
        // 3. rederive: cone nodes with an in-edge from a STILL-reached node come back,
        //    and propagate forward within the cone. A dead cycle has no such in-edge.
        Self::exec(&tx,
            "DELETE FROM cx_frontier; DELETE FROM cx_next;
             INSERT OR IGNORE INTO cx_frontier(key)
             SELECT DISTINCT c.key FROM cx_cone c JOIN edge e ON e.v=c.key JOIN node x ON x.key=e.u
             WHERE x.reached=1;", &mut self.stmts);
        Self::exec(&tx, "UPDATE node SET reached=1 WHERE key IN (SELECT key FROM cx_frontier);", &mut self.stmts);
        loop {
            Self::exec(&tx,
                "DELETE FROM cx_next;
                 INSERT INTO cx_next(key)
                 SELECT DISTINCT e.v FROM cx_frontier f CROSS JOIN edge e ON e.u=f.key
                   CROSS JOIN node w ON w.key=e.v CROSS JOIN cx_cone c ON c.key=e.v
                 WHERE w.reached=0;", &mut self.stmts);
            if Self::count(&tx, "SELECT count(*) FROM cx_next", &mut self.stmts) == 0 {
                break;
            }
            self.rounds += 1;
            Self::exec(&tx, "UPDATE node SET reached=1 WHERE key IN (SELECT key FROM cx_next);", &mut self.stmts);
            Self::exec(&tx, "DELETE FROM cx_frontier; INSERT INTO cx_frontier SELECT key FROM cx_next;", &mut self.stmts);
        }
        tx.commit().unwrap();
    }

    pub fn reachable(&self) -> (i64, u64) {
        self.conn
            .query_row(
                "SELECT COALESCE(xorhash(key),0), count(*) FROM node WHERE reached=1",
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
