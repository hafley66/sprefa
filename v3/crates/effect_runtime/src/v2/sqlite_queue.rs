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
//!   expand_tick      INTEGER NOT NULL,
//!   enqueued_at_ns  INTEGER NOT NULL
//! );
//! ```

use std::borrow::Cow;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row, ToSql, Transaction};

use super::codec::Codec;
use super::next::Next;
use super::next_key::NextKey;
use super::queue::{
    ExpandTick, InstanceId, PendingSummary, PipeHash, QueueBackend, QueueId, QueueRow,
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
  expand_tick      INTEGER NOT NULL,
  enqueued_at_ns  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS sprf_v3_queue_parent      ON sprf_v3_queue(parent_id);
CREATE INDEX IF NOT EXISTS sprf_v3_queue_kind_id     ON sprf_v3_queue(wake_kind, id);
CREATE INDEX IF NOT EXISTS sprf_v3_queue_park_lookup ON sprf_v3_queue(wake_kind, wake_domain, wake_key);
CREATE INDEX IF NOT EXISTS sprf_v3_queue_barrier     ON sprf_v3_queue(pipe_hash, instance_id, depth, wake_kind);
";

const WAKE_KIND_IMMEDIATE: i64 = 0;
const WAKE_KIND_TICK: i64 = 1;
const WAKE_KIND_KEY: i64 = 2;
const BULK_ENQUEUE_MULTIROW: usize = 2_048;
const TRACE_TARGET: &str = "effect_runtime::sqlite_queue";

pub struct SqliteQueue<N: Next + Codec> {
    conn: Arc<Mutex<Connection>>,
    _marker: PhantomData<fn() -> N>,
}

fn delete_queue_rows(conn: &Connection, ids: impl IntoIterator<Item = QueueId>) {
    let id_list: Vec<String> = ids.into_iter().map(|id| id.to_string()).collect();
    if id_list.is_empty() {
        return;
    }
    let sql = format!(
        "DELETE FROM sprf_v3_queue WHERE id IN ({})",
        id_list.join(",")
    );
    conn.execute(&sql, []).expect("bulk delete queue rows");
}

struct QueueInsertRecord {
    parent_id: Option<i64>,
    batch_idx: i64,
    path: Vec<u8>,
    pipe_hash: i64,
    instance_id: i64,
    depth: i64,
    next_blob: Vec<u8>,
    next_hash: Vec<u8>,
    wake_kind: i64,
    wake_tick: Option<i64>,
    wake_domain: Option<String>,
    wake_key: Option<Vec<u8>>,
    expand_tick: i64,
    enqueued_at_ns: i64,
}

impl<N: Next + Codec> From<&QueueRow<N>> for QueueInsertRecord {
    fn from(row: &QueueRow<N>) -> Self {
        let (wake_kind, wake_tick, wake_domain, wake_key) = match &row.wake {
            Wake::Immediate => (WAKE_KIND_IMMEDIATE, None, None, None),
            Wake::Tick { past_tick } => (WAKE_KIND_TICK, Some(*past_tick as i64), None, None),
            Wake::Key { domain, key } => (
                WAKE_KIND_KEY,
                None,
                Some(domain.as_ref().to_string()),
                Some(key.0.to_vec()),
            ),
        };
        Self {
            parent_id: row.parent_id.map(|p| p as i64),
            batch_idx: row.batch_idx as i64,
            path: encode_path(&row.path),
            pipe_hash: row.pipe_hash as i64,
            instance_id: row.instance_id as i64,
            depth: row.depth as i64,
            next_blob: row.value.encode(),
            next_hash: row.value.content_hash().to_vec(),
            wake_kind,
            wake_tick,
            wake_domain,
            wake_key,
            expand_tick: row.expand_tick as i64,
            enqueued_at_ns: row.enqueued_at_ns as i64,
        }
    }
}

fn insert_queue_records(tx: &Transaction<'_>, records: &[QueueInsertRecord]) {
    if records.is_empty() {
        return;
    }
    let values = std::iter::repeat("(?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .take(records.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "INSERT INTO sprf_v3_queue (
            parent_id, batch_idx, path, pipe_hash, instance_id, depth,
            next_blob, next_hash, wake_kind, wake_tick, wake_domain,
            wake_key, expand_tick, enqueued_at_ns
         ) VALUES {values}"
    );

    let mut params: Vec<&dyn ToSql> = Vec::with_capacity(records.len() * 14);
    for row in records {
        params.push(&row.parent_id);
        params.push(&row.batch_idx);
        params.push(&row.path);
        params.push(&row.pipe_hash);
        params.push(&row.instance_id);
        params.push(&row.depth);
        params.push(&row.next_blob);
        params.push(&row.next_hash);
        params.push(&row.wake_kind);
        params.push(&row.wake_tick);
        params.push(&row.wake_domain);
        params.push(&row.wake_key);
        params.push(&row.expand_tick);
        params.push(&row.enqueued_at_ns);
    }
    tx.execute(&sql, params_from_iter(params))
        .expect("bulk insert queue records");
}

#[derive(Default)]
struct QueueInsertStats {
    rows: usize,
    chunks: usize,
    next_blob_bytes: usize,
    next_hash_bytes: usize,
    path_bytes: usize,
    wake_key_bytes: usize,
    record_time: Duration,
    insert_time: Duration,
    commit_time: Duration,
    total_time: Duration,
}

fn trace_queue_insert(stats: &QueueInsertStats) {
    let payload_bytes =
        stats.next_blob_bytes + stats.next_hash_bytes + stats.path_bytes + stats.wake_key_bytes;
    tracing::info!(
        target: TRACE_TARGET,
        rows = stats.rows,
        chunks = stats.chunks,
        rows_per_sec = per_sec(stats.rows, stats.total_time).round() as u64,
        next_blob_MB = stats.next_blob_bytes as f64 / (1024.0 * 1024.0),
        next_hash_MB = stats.next_hash_bytes as f64 / (1024.0 * 1024.0),
        path_MB = stats.path_bytes as f64 / (1024.0 * 1024.0),
        wake_key_MB = stats.wake_key_bytes as f64 / (1024.0 * 1024.0),
        payload_MB = payload_bytes as f64 / (1024.0 * 1024.0),
        payload_MB_per_sec = mb_per_sec(payload_bytes, stats.total_time).round() as u64,
        record_rows_per_sec = per_sec(stats.rows, stats.record_time).round() as u64,
        insert_rows_per_sec = per_sec(stats.rows, stats.insert_time).round() as u64,
        commit_rows_per_sec = per_sec(stats.rows, stats.commit_time).round() as u64,
        record_ms = stats.record_time.as_secs_f64() * 1000.0,
        insert_ms = stats.insert_time.as_secs_f64() * 1000.0,
        commit_ms = stats.commit_time.as_secs_f64() * 1000.0,
        total_ms = stats.total_time.as_secs_f64() * 1000.0,
        "queue_bulk_enqueue"
    );
}

fn trace_queue_pull(
    name: &'static str,
    requested: usize,
    returned: usize,
    select_time: Duration,
    delete_time: Duration,
    total_time: Duration,
) {
    tracing::debug!(
        target: TRACE_TARGET,
        name,
        requested,
        returned,
        rows_per_sec = per_sec(returned, total_time).round() as u64,
        select_rows_per_sec = per_sec(returned, select_time).round() as u64,
        delete_rows_per_sec = per_sec(returned, delete_time).round() as u64,
        select_ms = select_time.as_secs_f64() * 1000.0,
        delete_ms = delete_time.as_secs_f64() * 1000.0,
        total_ms = total_time.as_secs_f64() * 1000.0,
        "queue_pull"
    );
}

fn per_sec(count: usize, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs == 0.0 {
        0.0
    } else {
        count as f64 / secs
    }
}

fn mb_per_sec(bytes: usize, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs == 0.0 {
        0.0
    } else {
        bytes as f64 / (1024.0 * 1024.0) / secs
    }
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
        Self {
            conn,
            _marker: PhantomData,
        }
    }

    pub fn open_in_memory() -> Self {
        let c = Connection::open_in_memory().expect("open :memory:");
        Self::open(Arc::new(Mutex::new(c)))
    }

    pub fn open_file(path: &std::path::Path) -> Self {
        let c = Connection::open(path).expect("open sqlite file");
        Self::open(Arc::new(Mutex::new(c)))
    }

    pub fn connection(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    /// Bulk insert in a single transaction. Used by `HybridQueue` to
    /// flush a batch of evicted parks to the cold tier with one fsync,
    /// not N. Returns the number of rows inserted.
    pub fn bulk_enqueue(&self, rows: Vec<QueueRow<N>>) -> usize {
        if rows.is_empty() {
            return 0;
        }
        let total_t0 = Instant::now();
        let trace_on = tracing::enabled!(target: TRACE_TARGET, tracing::Level::INFO);
        let mut stats = QueueInsertStats {
            rows: rows.len(),
            ..QueueInsertStats::default()
        };
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().expect("bulk_enqueue tx");
        for chunk in rows.chunks(BULK_ENQUEUE_MULTIROW) {
            let record_t0 = Instant::now();
            let records: Vec<QueueInsertRecord> =
                chunk.iter().map(QueueInsertRecord::from).collect();
            if trace_on {
                stats.chunks += 1;
                stats.record_time += record_t0.elapsed();
                stats.next_blob_bytes +=
                    records.iter().map(|row| row.next_blob.len()).sum::<usize>();
                stats.next_hash_bytes +=
                    records.iter().map(|row| row.next_hash.len()).sum::<usize>();
                stats.path_bytes += records.iter().map(|row| row.path.len()).sum::<usize>();
                stats.wake_key_bytes += records
                    .iter()
                    .filter_map(|row| row.wake_key.as_ref().map(Vec::len))
                    .sum::<usize>();
            }
            let insert_t0 = Instant::now();
            insert_queue_records(&tx, &records);
            if trace_on {
                stats.insert_time += insert_t0.elapsed();
            }
        }
        let n = rows.len();
        let commit_t0 = Instant::now();
        tx.commit().expect("bulk_enqueue commit");
        if trace_on {
            stats.commit_time = commit_t0.elapsed();
            stats.total_time = total_t0.elapsed();
            trace_queue_insert(&stats);
        }
        n
    }
}

fn encode_path(p: &[u32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(p.len() * 4);
    for s in p {
        b.extend_from_slice(&s.to_le_bytes());
    }
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
    let id: i64 = r.get("id")?;
    let parent_id: Option<i64> = r.get("parent_id")?;
    let batch_idx: i64 = r.get("batch_idx")?;
    let path_blob: Vec<u8> = r.get("path")?;
    let pipe_hash: i64 = r.get("pipe_hash")?;
    let instance_id: i64 = r.get("instance_id")?;
    let depth: i64 = r.get("depth")?;
    let next_blob: Vec<u8> = r.get("next_blob")?;
    let wake_kind: i64 = r.get("wake_kind")?;
    let wake_tick: Option<i64> = r.get("wake_tick")?;
    let wake_domain: Option<String> = r.get("wake_domain")?;
    let wake_key: Option<Vec<u8>> = r.get("wake_key")?;
    let expand_tick: i64 = r.get("expand_tick")?;
    let enqueued_at_ns: i64 = r.get("enqueued_at_ns")?;

    let wake = match wake_kind {
        x if x == WAKE_KIND_IMMEDIATE => Wake::Immediate,
        x if x == WAKE_KIND_TICK => Wake::Tick {
            past_tick: wake_tick.unwrap() as u64,
        },
        x if x == WAKE_KIND_KEY => {
            let mut k = [0u8; 32];
            k.copy_from_slice(&wake_key.unwrap());
            Wake::Key {
                domain: Cow::Owned(wake_domain.unwrap_or_default()),
                key: NextKey(k),
            }
        }
        _ => unreachable!("unknown wake_kind {}", wake_kind),
    };

    Ok(QueueRow {
        id: id as u64,
        parent_id: parent_id.map(|p| p as u64),
        batch_idx: batch_idx as u32,
        path: decode_path(&path_blob),
        pipe_hash: pipe_hash as u64,
        instance_id: instance_id as u64,
        depth: depth as u32,
        value: Arc::new(N::decode(&next_blob)),
        wake,
        expand_tick: expand_tick as u64,
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
            Wake::Immediate => (WAKE_KIND_IMMEDIATE, None, None, None),
            Wake::Tick { past_tick } => (WAKE_KIND_TICK, Some(*past_tick as i64), None, None),
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
                wake_key, expand_tick, enqueued_at_ns
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
                row.expand_tick as i64,
                row.enqueued_at_ns as i64,
            ],
        )
        .expect("queue insert");
        conn.last_insert_rowid() as u64
    }

    fn enqueue_replacing_parked(&self, row: QueueRow<N>) -> QueueId {
        let Wake::Key { domain, key } = &row.wake else {
            return self.enqueue(row);
        };

        let conn = self.conn.lock().unwrap();
        let next_hash = row.value.content_hash().to_vec();
        conn.execute(
            "DELETE FROM sprf_v3_queue
              WHERE wake_kind = ?1
                AND wake_domain = ?2
                AND wake_key = ?3
                AND pipe_hash = ?4
                AND instance_id = ?5
                AND depth = ?6
                AND next_hash = ?7",
            params![
                WAKE_KIND_KEY,
                domain.as_ref(),
                key.0.to_vec(),
                row.pipe_hash as i64,
                row.instance_id as i64,
                row.depth as i64,
                next_hash,
            ],
        )
        .expect("queue replace parked delete");

        conn.execute(
            "INSERT INTO sprf_v3_queue (
                parent_id, batch_idx, path, pipe_hash, instance_id, depth,
                next_blob, next_hash, wake_kind, wake_tick, wake_domain,
                wake_key, expand_tick, enqueued_at_ns
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
                WAKE_KIND_KEY,
                Option::<i64>::None,
                domain.as_ref(),
                key.0.to_vec(),
                row.expand_tick as i64,
                row.enqueued_at_ns as i64,
            ],
        )
        .expect("queue replace parked insert");
        conn.last_insert_rowid() as u64
    }

    fn pull_runnable(&self, global_tick: ExpandTick) -> Option<QueueRow<N>> {
        let conn = self.conn.lock().unwrap();

        // 1. Immediate — oldest by id.
        let im = conn
            .query_row(
                "SELECT * FROM sprf_v3_queue WHERE wake_kind = ?1 ORDER BY id ASC LIMIT 1",
                params![WAKE_KIND_IMMEDIATE],
                row_to_queue::<N>,
            )
            .optional()
            .expect("queue select immediate");
        if let Some(row) = im {
            conn.execute(
                "DELETE FROM sprf_v3_queue WHERE id = ?1",
                params![row.id as i64],
            )
            .expect("queue delete");
            return Some(row);
        }

        // 2. Tick — any past_tick < global_tick.
        let tk = conn
            .query_row(
                "SELECT * FROM sprf_v3_queue
             WHERE wake_kind = ?1 AND wake_tick < ?2
             ORDER BY id ASC LIMIT 1",
                params![WAKE_KIND_TICK, global_tick as i64],
                row_to_queue::<N>,
            )
            .optional()
            .expect("queue select tick");
        if let Some(row) = tk {
            conn.execute(
                "DELETE FROM sprf_v3_queue WHERE id = ?1",
                params![row.id as i64],
            )
            .expect("queue delete");
            return Some(row);
        }

        // 3. Key rows are never directly runnable; dispatch_park flips
        //    them to Immediate first.

        None
    }

    fn depth(&self) -> u64 {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sprf_v3_queue", [], |r| r.get(0))
            .expect("queue depth");
        n as u64
    }

    fn dispatch_park(&self, domain: &str, key: Option<NextKey>) -> u64 {
        let conn = self.conn.lock().unwrap();
        let n = match key {
            Some(k) => conn
                .execute(
                    "UPDATE sprf_v3_queue
                    SET wake_kind = ?1, wake_domain = NULL, wake_key = NULL
                  WHERE wake_kind = ?2 AND wake_domain = ?3 AND wake_key = ?4",
                    params![WAKE_KIND_IMMEDIATE, WAKE_KIND_KEY, domain, k.0.to_vec()],
                )
                .expect("dispatch_park update (keyed)"),
            None => conn
                .execute(
                    "UPDATE sprf_v3_queue
                    SET wake_kind = ?1, wake_domain = NULL, wake_key = NULL
                  WHERE wake_kind = ?2 AND wake_domain = ?3",
                    params![WAKE_KIND_IMMEDIATE, WAKE_KIND_KEY, domain],
                )
                .expect("dispatch_park update (domain)"),
        };
        n as u64
    }

    fn has_parked_domain(&self, domain: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sprf_v3_queue
                    WHERE wake_kind = ?1 AND wake_domain = ?2
                    LIMIT 1
                )",
                params![WAKE_KIND_KEY, domain],
                |r| r.get(0),
            )
            .expect("queue parked domain exists");
        n != 0
    }

    fn pending_summary_before_or_at(
        &self,
        pipe_hash: PipeHash,
        instance_id: InstanceId,
        global_tick: ExpandTick,
        max_depth: u32,
    ) -> PendingSummary {
        let conn = self.conn.lock().unwrap();
        let runnable: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sprf_v3_queue
             WHERE pipe_hash = ?1
               AND instance_id = ?2
               AND depth <= ?3
               AND (
                    wake_kind = ?4
                    OR (wake_kind = ?5 AND wake_tick < ?6)
               )",
                params![
                    pipe_hash as i64,
                    instance_id as i64,
                    max_depth as i64,
                    WAKE_KIND_IMMEDIATE,
                    WAKE_KIND_TICK,
                    global_tick as i64,
                ],
                |r| r.get(0),
            )
            .expect("barrier runnable count");
        let parked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sprf_v3_queue
             WHERE pipe_hash = ?1
               AND instance_id = ?2
               AND depth <= ?3
               AND (
                    wake_kind = ?4
                    OR (wake_kind = ?5 AND wake_tick >= ?6)
               )",
                params![
                    pipe_hash as i64,
                    instance_id as i64,
                    max_depth as i64,
                    WAKE_KIND_KEY,
                    WAKE_KIND_TICK,
                    global_tick as i64,
                ],
                |r| r.get(0),
            )
            .expect("barrier parked count");
        PendingSummary {
            runnable: runnable as u64,
            parked: parked as u64,
        }
    }

    fn pull_runnable_batch_for(
        &self,
        pipe_hash: PipeHash,
        instance_id: InstanceId,
        global_tick: ExpandTick,
        n: usize,
    ) -> Vec<QueueRow<N>> {
        if n == 0 {
            return Vec::new();
        }
        let total_t0 = Instant::now();
        let trace_on = tracing::enabled!(target: TRACE_TARGET, tracing::Level::INFO);
        let conn = self.conn.lock().unwrap();
        let select_t0 = Instant::now();
        let first = conn
            .query_row(
                "SELECT * FROM sprf_v3_queue
             WHERE pipe_hash = ?1
               AND instance_id = ?2
               AND wake_kind = ?3
             ORDER BY id ASC
             LIMIT 1",
                params![pipe_hash as i64, instance_id as i64, WAKE_KIND_IMMEDIATE],
                row_to_queue::<N>,
            )
            .optional()
            .expect("query scoped immediate");

        if let Some(first) = first {
            let depth = first.depth;
            let mut stmt = conn
                .prepare(
                    "SELECT * FROM sprf_v3_queue
                 WHERE pipe_hash = ?1
                   AND instance_id = ?2
                   AND depth = ?3
                   AND wake_kind = ?4
                 ORDER BY id ASC
                 LIMIT ?5",
                )
                .expect("prepare scoped runnable batch");
            let rows = stmt
                .query_map(
                    params![
                        pipe_hash as i64,
                        instance_id as i64,
                        depth as i64,
                        WAKE_KIND_IMMEDIATE,
                        n as i64,
                    ],
                    row_to_queue::<N>,
                )
                .expect("query scoped runnable batch");
            let out: Vec<QueueRow<N>> = rows.map(|row| row.expect("scoped runnable row")).collect();
            let select_time = select_t0.elapsed();
            let delete_t0 = Instant::now();
            delete_queue_rows(&conn, out.iter().map(|row| row.id));
            if trace_on {
                trace_queue_pull(
                    "pull_runnable_batch_for_immediate",
                    n,
                    out.len(),
                    select_time,
                    delete_t0.elapsed(),
                    total_t0.elapsed(),
                );
            }
            return out;
        }

        let tick_select_t0 = Instant::now();
        let tick = conn
            .query_row(
                "SELECT * FROM sprf_v3_queue
             WHERE pipe_hash = ?1
               AND instance_id = ?2
               AND wake_kind = ?3
               AND wake_tick < ?4
             ORDER BY id ASC
             LIMIT 1",
                params![
                    pipe_hash as i64,
                    instance_id as i64,
                    WAKE_KIND_TICK,
                    global_tick as i64,
                ],
                row_to_queue::<N>,
            )
            .optional()
            .expect("query scoped tick");
        if let Some(row) = tick {
            let select_time = select_t0.elapsed() + tick_select_t0.elapsed();
            let delete_t0 = Instant::now();
            conn.execute(
                "DELETE FROM sprf_v3_queue WHERE id = ?1",
                params![row.id as i64],
            )
            .expect("delete scoped tick row");
            if trace_on {
                trace_queue_pull(
                    "pull_runnable_batch_for_tick",
                    n,
                    1,
                    select_time,
                    delete_t0.elapsed(),
                    total_t0.elapsed(),
                );
            }
            return vec![row];
        }

        if trace_on {
            trace_queue_pull(
                "pull_runnable_batch_for_empty",
                n,
                0,
                select_t0.elapsed(),
                Duration::ZERO,
                total_t0.elapsed(),
            );
        }
        Vec::new()
    }

    fn cascade_delete(&self, root: QueueId) -> u64 {
        let conn = self.conn.lock().unwrap();
        // Recursive CTE walks parent_id chain from root downward; one
        // DELETE statement removes the whole subtree. The
        // sprf_v3_queue_parent index makes the join O(matching).
        // Seed includes both `id = root` (root if still present) AND
        // `parent_id = root` (direct children, so descendants are found
        // even when root has already been popped). Recursion expands
        // grandchildren+ from there.
        let n = conn
            .execute(
                "WITH RECURSIVE doomed(id) AS (
                 SELECT id FROM sprf_v3_queue WHERE id = ?1 OR parent_id = ?1
                 UNION ALL
                 SELECT q.id FROM sprf_v3_queue q
                 JOIN doomed d ON q.parent_id = d.id
             )
             DELETE FROM sprf_v3_queue WHERE id IN (SELECT id FROM doomed)",
                params![root as i64],
            )
            .expect("cascade_delete");
        n as u64
    }

    fn pull_runnable_batch(&self, global_tick: ExpandTick, n: usize) -> Vec<QueueRow<N>> {
        if n == 0 {
            return Vec::new();
        }
        let total_t0 = Instant::now();
        let trace_on = tracing::enabled!(target: TRACE_TARGET, tracing::Level::INFO);
        let conn = self.conn.lock().unwrap();
        let select_t0 = Instant::now();

        // Hot path: WAKE_KIND_IMMEDIATE rows. SELECT up to n in id
        // order, take the (pipe_hash, depth)-homogeneous head prefix,
        // bulk DELETE matching ids in one statement.
        let mut stmt = conn
            .prepare_cached(
                "SELECT * FROM sprf_v3_queue
             WHERE wake_kind = ?1
             ORDER BY id ASC LIMIT ?2",
            )
            .expect("prepare batch select");
        let rows: Vec<QueueRow<N>> = stmt
            .query_map(params![WAKE_KIND_IMMEDIATE, n as i64], row_to_queue::<N>)
            .expect("batch query")
            .filter_map(Result::ok)
            .collect();

        if !rows.is_empty() {
            let head_pipe = rows[0].pipe_hash;
            let head_depth = rows[0].depth;
            let prefix: Vec<QueueRow<N>> = rows
                .into_iter()
                .take_while(|r| r.pipe_hash == head_pipe && r.depth == head_depth)
                .collect();
            let select_time = select_t0.elapsed();
            let delete_t0 = Instant::now();
            delete_queue_rows(&conn, prefix.iter().map(|row| row.id));
            if trace_on {
                trace_queue_pull(
                    "pull_runnable_batch_immediate",
                    n,
                    prefix.len(),
                    select_time,
                    delete_t0.elapsed(),
                    total_t0.elapsed(),
                );
            }
            return prefix;
        }

        drop(stmt);
        drop(conn);
        match self.pull_runnable(global_tick) {
            Some(r) => vec![r],
            None => Vec::new(),
        }
    }
}
