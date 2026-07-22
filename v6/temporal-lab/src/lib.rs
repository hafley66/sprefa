//! Append-only bitemporal fact table on SQLite.
//!
//!   fact(key, tt_from, tt_to, weight)   PK (key, tt_from)   partial index on live
//!
//! - valid-time lives in the KEY (the coordinate model: a key encodes which rev a
//!   fact is about; a cross-rev fact encodes two). The engine never interprets it.
//! - transaction-time lives in the INTERVAL: tt_from..tt_to, the revision counter.
//! - retract = weight hits 0 -> SET tt_to = R. NEVER DELETE. That one choice makes
//!   it durable (append-only), gives history for free, and makes as-of a filter.
//! - compaction = drop closed intervals older than a retention horizon. The live set
//!   (tt_to IS NULL) is never touched, so it stays correct and size stays bounded.
//!
//! All writes are set-based (one txn = one revision, JSON-batched delta, no N+1).

use rusqlite::{functions::FunctionFlags, Connection};

/// splitmix64 — an order-independent per-key hash for the XOR digest.
fn mix(k: i64) -> i64 {
    let mut z = (k as u64).wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    (z ^ (z >> 31)) as i64
}

pub struct TemporalStore {
    pub conn: Connection,
    pub rev: i64,
}

impl TemporalStore {
    /// `path=None` = in-memory (correctness runs); `Some(p)` = a WAL file (scale/RSS).
    pub fn open(path: Option<&str>) -> rusqlite::Result<Self> {
        let conn = match path {
            Some(p) => Connection::open(p)?,
            None => Connection::open_in_memory()?,
        };
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS fact(
                 key     INTEGER NOT NULL,
                 tt_from INTEGER NOT NULL,
                 tt_to   INTEGER,
                 weight  INTEGER NOT NULL,
                 PRIMARY KEY(key, tt_from)
             ) WITHOUT ROWID;
             CREATE INDEX IF NOT EXISTS ix_live ON fact(key) WHERE tt_to IS NULL;
             CREATE TEMP TABLE d(key INTEGER PRIMARY KEY, dw INTEGER);",
        )?;
        // xorhash(key): the live-set digest, computed in SQL over the on-disk rows so
        // the check never pulls the fact set into RAM.
        conn.create_aggregate_function(
            "xorhash",
            1,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            XorHash,
        )?;
        Ok(Self { conn, rev: 0 })
    }

    /// Apply one revision's net per-key weight deltas as a single transaction.
    /// `deltas` = (key, dw) where dw is the net support change this revision.
    pub fn commit(&mut self, deltas: &[(i64, i64)]) -> rusqlite::Result<i64> {
        self.rev += 1;
        let r = self.rev;
        // one JSON array of [key,dw] pairs -> one bulk insert (no per-row write).
        let mut json = String::from("[");
        for (i, (k, dw)) in deltas.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!("[{k},{dw}]"));
        }
        json.push(']');

        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM d", [])?;
        // GROUP BY nets multiple deltas for the same key in one revision (and keeps
        // d.key unique).
        tx.execute(
            "INSERT INTO d(key,dw)
             SELECT json_extract(value,'$[0]'), sum(json_extract(value,'$[1]'))
             FROM json_each(?1) GROUP BY 1",
            [&json],
        )?;
        // birth: keys with net-positive intent and no live interval get a fresh one.
        tx.execute(
            "INSERT INTO fact(key,tt_from,tt_to,weight)
             SELECT d.key, ?1, NULL, 0 FROM d
             LEFT JOIN fact f ON f.key=d.key AND f.tt_to IS NULL
             WHERE f.key IS NULL AND d.dw>0",
            [r],
        )?;
        // apply the deltas to the live intervals. The `key IN (SELECT key FROM d)`
        // form drives from d (EXPLAIN: SEARCH fact USING PRIMARY KEY, d via IN) so it
        // is O(Δ·log n); UPDATE..FROM here instead SCANs the whole live index — the
        // difference is O(Δ) vs O(live) per revision, i.e. linear vs quadratic total.
        tx.execute(
            "UPDATE fact SET weight = weight + (SELECT dw FROM d WHERE d.key=fact.key)
             WHERE tt_to IS NULL AND key IN (SELECT key FROM d)",
            [],
        )?;
        // retract: close any live interval whose support hit 0 — restricted to the
        // keys this revision touched (a key not in d cannot have changed weight), so
        // this never scans the whole live index.
        tx.execute(
            "UPDATE fact SET tt_to=?1
             WHERE tt_to IS NULL AND weight<=0 AND key IN (SELECT key FROM d)",
            [r],
        )?;
        tx.commit()?;
        Ok(r)
    }

    /// Digest of the set live right now (tt_to IS NULL).
    pub fn live_digest(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COALESCE(xorhash(key),0) FROM fact WHERE tt_to IS NULL", [], |r| r.get(0))
    }

    /// Digest of the set that was live AS OF transaction-time `tt` — same query shape,
    /// any point in the past. This is the time-travel read.
    pub fn live_at_digest(&self, tt: i64) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COALESCE(xorhash(key),0) FROM fact
             WHERE tt_from<=?1 AND (tt_to IS NULL OR tt_to>?1)",
            [tt],
            |r| r.get(0),
        )
    }

    /// Digest of the set live now whose key falls in [key_lo, key_hi) — i.e. a
    /// valid-time slice (the key encodes valid-time). Combine with live_at for a
    /// full bitemporal query (a world, as known at a time).
    pub fn live_world_at(&self, key_lo: i64, key_hi: i64, tt: i64) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COALESCE(xorhash(key),0) FROM fact
             WHERE key>=?1 AND key<?2 AND tt_from<=?3 AND (tt_to IS NULL OR tt_to>?3)",
            [key_lo, key_hi, tt],
            |r| r.get(0),
        )
    }

    pub fn live_count(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT count(*) FROM fact WHERE tt_to IS NULL", [], |r| r.get(0))
    }

    pub fn row_count(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT count(*) FROM fact", [], |r| r.get(0))
    }

    /// Compaction: drop closed intervals that ended at/before `horizon`. The live set
    /// is untouched (tt_to IS NULL never matches), and any as-of query at tt>=horizon
    /// is unaffected (a dropped interval ended before horizon, so it never contained
    /// tt). History strictly before the horizon is intentionally forgotten.
    pub fn compact(&self, horizon: i64) -> rusqlite::Result<usize> {
        self.conn
            .execute("DELETE FROM fact WHERE tt_to IS NOT NULL AND tt_to<=?1", [horizon])
    }

    /// Physically reclaim freed pages after a compaction.
    pub fn vacuum(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch("VACUUM")
    }
}

/// Reference oracle: an in-RAM weighted multiset. Proven correct against a
/// from-scratch batch recompute in frp-lab; here it is the trusted live-set truth
/// the SQLite engine must match at every revision.
#[derive(Default)]
pub struct RamOracle {
    pub weight: std::collections::HashMap<i64, i64>,
}

impl RamOracle {
    pub fn commit(&mut self, deltas: &[(i64, i64)]) {
        for &(k, dw) in deltas {
            let w = self.weight.entry(k).or_insert(0);
            *w += dw;
            if *w <= 0 {
                self.weight.remove(&k);
            }
        }
    }
    pub fn live_digest(&self) -> i64 {
        self.weight.keys().fold(0i64, |a, &k| a ^ mix(k))
    }
    pub fn live_keys_sorted(&self) -> Vec<i64> {
        let mut v: Vec<i64> = self.weight.keys().copied().collect();
        v.sort_unstable();
        v
    }
}

/// The XOR-of-splitmix aggregate registered into SQLite.
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

pub fn peak_rss_mb() -> f64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    ru.ru_maxrss as f64 / (1024.0 * 1024.0) // darwin: bytes
}
