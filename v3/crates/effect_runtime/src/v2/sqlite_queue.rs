//! `SqliteQueue<N>` — durable `QueueBackend<N>` impl over rusqlite.
//!
//! Same trait surface as `MemQueue<N>` — the driver is unchanged.
//! Connection is shared (`Arc<Mutex<Connection>>`) so the consumer
//! crate can run queue mutations and relational fact writes in the
//! same transaction. Construction takes the connection; this crate
//! does not own the file or its path.
//!
//! Park subscriptions live in the row itself (`wake_kind`,
//! `wake_domain`, `wake_key`). `dispatch_park(domain, key)` is one
//! indexed `UPDATE ... SET wake_kind=IMMEDIATE` statement; the index
//! on `(wake_kind, wake_domain, wake_key)` keeps it O(matching).
//!
//! Storage layout:
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS sprf_v3_queue (
//!   id              INTEGER PRIMARY KEY AUTOINCREMENT,
//!   parent_id       INTEGER,
//!   batch_idx       INTEGER NOT NULL,
//!   path            BLOB    NOT NULL,
//!   pipe_hash       INTEGER NOT NULL,
//!   instance_id     INTEGER NOT NULL,
//!   depth           INTEGER NOT NULL,
//!   next_blob       BLOB    NOT NULL,
//!   next_hash       BLOB    NOT NULL,
//!   wake_kind       INTEGER NOT NULL,
//!   wake_tick       INTEGER,
//!   wake_domain     TEXT,
//!   wake_key        BLOB,
//!   drive_tick      INTEGER NOT NULL,
//!   enqueued_at_ns  INTEGER NOT NULL
//! );
//! ```

use std::borrow::Cow;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Row};

use super::codec::Codec;
use super::next::Next;
use super::next_key::NextKey;
use super::queue::{
    DriveTick, QueueBackend, QueueId, QueueRow,
};
use super::wake::Wake;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sprf_v3_queue (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  parent_id       INTEGER,
  batch_idx       INTEGER NOT NULL,
  path            BLOB    NOT NULL,
  pipe_hash       INTEGER NOT NULL,
  instance_id     INTEGER NOT NULL,
  depth           INTEGER NOT NULL,
  next_blob       BLOB    NOT NULL,
  next_hash       BLOB    NOT NULL,
  wake_kind       INTEGER NOT NULL,
  wake_tick       INTEGER,
  wake_domain     TEXT,
  wake_key        BLOB,
  drive_tick      INTEGER NOT NULL,
  enqueued_at_ns  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS sprf_v3_queue_parent      ON sprf_v3_queue(parent_id);
CREATE INDEX IF NOT EXISTS sprf_v3_queue_kind_id     ON sprf_v3_queue(wake_kind, id);
CREATE INDEX IF NOT EXISTS sprf_v3_queue_park_lookup ON sprf_v3_queue(wake_kind, wake_domain, wake_key);
";

const WAKE_KIND_IMMEDIATE: i64 = 0;
const WAKE_KIND_TICK:      i64 = 1;
const WAKE_KIND_KEY:       i64 = 2;

pub struct SqliteQueue<N: Next + Codec> {
    conn:    Arc<Mutex<Connection>>,
    _marker: PhantomData<fn() -> N>,
}

impl<N: Next + Codec> SqliteQueue<N> {
    pub fn open(conn: Arc<Mutex<Connection>>) -> Self {
        {
            let c = conn.lock().unwrap();
            // WAL improves concurrent read/write throughput; safe at
            // pre-1.0 since the file format is rebuilt anyway.
            let _ = c.pragma_update(None, "journal_mode", "WAL");
            c.execute_batch(SCHEMA).expect("queue schema");
        }
        Self { conn, _marker: PhantomData }
    }

    pub fn open_in_memory() -> Self {
        let c = Connection::open_in_memory().expect("open :memory:");
        Self::open(Arc::new(Mutex::new(c)))
    }

    pub fn open_file(path: &std::path::Path) -> Self {
        let c = Connection::open(path).expect("open sqlite file");
        Self::open(Arc::new(Mutex::new(c)))
    }

    pub fn connection(&self) -> Arc<Mutex<Connection>> { self.conn.clone() }

    /// Bulk insert in a single transaction. Used by `HybridQueue` to
    /// flush a batch of evicted parks to the cold tier with one fsync,
    /// not N. Returns the number of rows inserted.
    pub fn bulk_enqueue(&self, rows: Vec<QueueRow<N>>) -> usize {
        if rows.is_empty() { return 0; }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().expect("bulk_enqueue tx");
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO sprf_v3_queue (
                    parent_id, batch_idx, path, pipe_hash, instance_id, depth,
                    next_blob, next_hash, wake_kind, wake_tick, wake_domain,
                    wake_key, drive_tick, enqueued_at_ns
                 ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
            ).expect("prepare bulk insert");
            for row in &rows {
                let (wake_kind, wake_tick, wake_domain, wake_key) = match &row.wake {
                    Wake::Immediate         => (WAKE_KIND_IMMEDIATE, None,                    None,                None),
                    Wake::Tick { past_tick }=> (WAKE_KIND_TICK,      Some(*past_tick as i64), None,                None),
                    Wake::Key { domain, key } => (
                        WAKE_KIND_KEY,
                        None,
                        Some(domain.as_ref().to_string()),
                        Some(key.0.to_vec()),
                    ),
                };
                stmt.execute(params![
                    row.parent_id.map(|p| p as i64),
                    row.batch_idx as i64,
                    encode_path(&row.path),
                    row.pipe_hash as i64,
                    row.instance_id as i64,
                    row.depth as i64,
                    row.value.encode(),
                    row.value.content_hash().to_vec(),
                    wake_kind,
                    wake_tick,
                    wake_domain,
                    wake_key,
                    row.drive_tick as i64,
                    row.enqueued_at_ns as i64,
                ]).expect("bulk insert row");
            }
        }
        let n = rows.len();
        tx.commit().expect("bulk_enqueue commit");
        n
    }
}

fn encode_path(p: &[u32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(p.len() * 4);
    for s in p { b.extend_from_slice(&s.to_le_bytes()); }
    b
}

fn decode_path(b: &[u8]) -> Vec<u32> {
    let mut out = Vec::with_capacity(b.len() / 4);
    for chunk in b.chunks_exact(4) {
        let mut a = [0u8; 4];
        a.copy_from_slice(chunk);
        out.push(u32::from_le_bytes(a));
    }
    out
}

fn row_to_queue<N: Next + Codec>(r: &Row) -> rusqlite::Result<QueueRow<N>> {
    let id:             i64       = r.get("id")?;
    let parent_id:      Option<i64> = r.get("parent_id")?;
    let batch_idx:      i64       = r.get("batch_idx")?;
    let path_blob:      Vec<u8>   = r.get("path")?;
    let pipe_hash:      i64       = r.get("pipe_hash")?;
    let instance_id:    i64       = r.get("instance_id")?;
    let depth:          i64       = r.get("depth")?;
    let next_blob:      Vec<u8>   = r.get("next_blob")?;
    let wake_kind:      i64       = r.get("wake_kind")?;
    let wake_tick:      Option<i64> = r.get("wake_tick")?;
    let wake_domain:    Option<String> = r.get("wake_domain")?;
    let wake_key:       Option<Vec<u8>> = r.get("wake_key")?;
    let drive_tick:     i64       = r.get("drive_tick")?;
    let enqueued_at_ns: i64       = r.get("enqueued_at_ns")?;

    let wake = match wake_kind {
        x if x == WAKE_KIND_IMMEDIATE => Wake::Immediate,
        x if x == WAKE_KIND_TICK      => Wake::Tick { past_tick: wake_tick.unwrap() as u64 },
        x if x == WAKE_KIND_KEY       => {
            let mut k = [0u8; 32];
            k.copy_from_slice(&wake_key.unwrap());
            Wake::Key {
                domain: Cow::Owned(wake_domain.unwrap_or_default()),
                key:    NextKey(k),
            }
        }
        _ => unreachable!("unknown wake_kind {}", wake_kind),
    };

    Ok(QueueRow {
        id:             id as u64,
        parent_id:      parent_id.map(|p| p as u64),
        batch_idx:      batch_idx as u32,
        path:           decode_path(&path_blob),
        pipe_hash:      pipe_hash as u64,
        instance_id:    instance_id as u64,
        depth:          depth as u32,
        value:          Arc::new(N::decode(&next_blob)),
        wake,
        drive_tick:     drive_tick as u64,
        enqueued_at_ns: enqueued_at_ns as u64,
    })
}

impl<N: Next + Codec> QueueBackend<N> for SqliteQueue<N> {
    // invariant: do not stash row.value anywhere; Arc<N> drops on
    // return. Park-as-row's RSS guarantee depends on the carrier being
    // serialized into next_blob and then dropped. If a future
    // refactor introduces a side-cache keyed by id, it must hold a
    // weak ref or recompute from the blob.
    fn enqueue(&self, row: QueueRow<N>) -> QueueId {
        let conn = self.conn.lock().unwrap();
        let (wake_kind, wake_tick, wake_domain, wake_key) = match &row.wake {
            Wake::Immediate     => (WAKE_KIND_IMMEDIATE, None,                    None,                None),
            Wake::Tick{past_tick}=>(WAKE_KIND_TICK,      Some(*past_tick as i64), None,                None),
            Wake::Key { domain, key } => (
                WAKE_KIND_KEY,
                None,
                Some(domain.as_ref().to_string()),
                Some(key.0.to_vec()),
            ),
        };

        conn.execute(
            "INSERT INTO sprf_v3_queue (
                parent_id, batch_idx, path, pipe_hash, instance_id, depth,
                next_blob, next_hash, wake_kind, wake_tick, wake_domain,
                wake_key, drive_tick, enqueued_at_ns
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                row.parent_id.map(|p| p as i64),
                row.batch_idx as i64,
                encode_path(&row.path),
                row.pipe_hash as i64,
                row.instance_id as i64,
                row.depth as i64,
                row.value.encode(),
                row.value.content_hash().to_vec(),
                wake_kind,
                wake_tick,
                wake_domain,
                wake_key,
                row.drive_tick as i64,
                row.enqueued_at_ns as i64,
            ],
        ).expect("queue insert");
        conn.last_insert_rowid() as u64
    }

    fn pull_runnable(
        &self,
        global_tick: DriveTick,
    ) -> Option<QueueRow<N>> {
        let conn = self.conn.lock().unwrap();

        // 1. Immediate — oldest by id.
        let im = conn.query_row(
            "SELECT * FROM sprf_v3_queue WHERE wake_kind = ?1 ORDER BY id ASC LIMIT 1",
            params![WAKE_KIND_IMMEDIATE],
            row_to_queue::<N>,
        ).optional().expect("queue select immediate");
        if let Some(row) = im {
            conn.execute("DELETE FROM sprf_v3_queue WHERE id = ?1", params![row.id as i64])
                .expect("queue delete");
            return Some(row);
        }

        // 2. Tick — any past_tick < global_tick.
        let tk = conn.query_row(
            "SELECT * FROM sprf_v3_queue
             WHERE wake_kind = ?1 AND wake_tick < ?2
             ORDER BY id ASC LIMIT 1",
            params![WAKE_KIND_TICK, global_tick as i64],
            row_to_queue::<N>,
        ).optional().expect("queue select tick");
        if let Some(row) = tk {
            conn.execute("DELETE FROM sprf_v3_queue WHERE id = ?1", params![row.id as i64])
                .expect("queue delete");
            return Some(row);
        }

        // 3. Key rows are never directly runnable; dispatch_park flips
        //    them to Immediate first.

        None
    }

    fn depth(&self) -> u64 {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sprf_v3_queue",
            [],
            |r| r.get(0),
        ).expect("queue depth");
        n as u64
    }

    fn dispatch_park(&self, domain: &str, key: Option<NextKey>) -> u64 {
        let conn = self.conn.lock().unwrap();
        let n = match key {
            Some(k) => conn.execute(
                "UPDATE sprf_v3_queue
                    SET wake_kind = ?1, wake_domain = NULL, wake_key = NULL
                  WHERE wake_kind = ?2 AND wake_domain = ?3 AND wake_key = ?4",
                params![WAKE_KIND_IMMEDIATE, WAKE_KIND_KEY, domain, k.0.to_vec()],
            ).expect("dispatch_park update (keyed)"),
            None => conn.execute(
                "UPDATE sprf_v3_queue
                    SET wake_kind = ?1, wake_domain = NULL, wake_key = NULL
                  WHERE wake_kind = ?2 AND wake_domain = ?3",
                params![WAKE_KIND_IMMEDIATE, WAKE_KIND_KEY, domain],
            ).expect("dispatch_park update (domain)"),
        };
        n as u64
    }

    fn pull_runnable_batch(
        &self,
        global_tick: DriveTick,
        n:           usize,
    ) -> Vec<QueueRow<N>> {
        if n == 0 { return Vec::new(); }
        let conn = self.conn.lock().unwrap();

        // Hot path: WAKE_KIND_IMMEDIATE rows. SELECT up to n in id
        // order, take the (pipe_hash, depth)-homogeneous head prefix,
        // bulk DELETE matching ids in one statement.
        let mut stmt = conn.prepare_cached(
            "SELECT * FROM sprf_v3_queue
             WHERE wake_kind = ?1
             ORDER BY id ASC LIMIT ?2"
        ).expect("prepare batch select");
        let rows: Vec<QueueRow<N>> = stmt
            .query_map(params![WAKE_KIND_IMMEDIATE, n as i64], row_to_queue::<N>)
            .expect("batch query")
            .filter_map(Result::ok)
            .collect();

        if !rows.is_empty() {
            let head_pipe  = rows[0].pipe_hash;
            let head_depth = rows[0].depth;
            let prefix: Vec<QueueRow<N>> = rows.into_iter()
                .take_while(|r| r.pipe_hash == head_pipe && r.depth == head_depth)
                .collect();
            let id_list: Vec<String> = prefix.iter().map(|r| r.id.to_string()).collect();
            let sql = format!(
                "DELETE FROM sprf_v3_queue WHERE id IN ({})",
                id_list.join(",")
            );
            conn.execute(&sql, []).expect("bulk delete");
            return prefix;
        }

        drop(stmt);
        drop(conn);
        match self.pull_runnable(global_tick) {
            Some(r) => vec![r],
            None    => Vec::new(),
        }
    }
}
