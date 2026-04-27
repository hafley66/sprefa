//! L2 sqlite-backed persistence for [`OpCache`] (sprefa-upb Phase D).
//!
//! Schema:
//! ```sql
//! CREATE TABLE op_cache (
//!     key BLOB PRIMARY KEY,
//!     value BLOB NOT NULL,
//!     created_at INTEGER NOT NULL DEFAULT (unixepoch())
//! ) WITHOUT ROWID;
//! ```
//!
//! Key = `[u8; 32]` blake3 composite from `compose_key(batch, op)`.
//! Value = `bincode::serialize(&Vec<Cursor>)`. Slots are dropped on
//! L2 (intentional sharp edge — see [`crate::_0_cursor::Cursor`]).
//!
//! Forward-compat notes:
//! - `created_at` lets a future watcher (Phase F) stamp invalidation
//!   timestamps or join against a sibling `cache_inputs` table.
//! - `WITHOUT ROWID` is a primary-key-by-blob optimization; the table
//!   is independent of `sprf_meta` so no migration collision exists.
//!
//! Failure mode: any sqlite or bincode error returns `None` from
//! [`DiskCache::get`] / silently drops on insert. Treat L2 as a
//! best-effort accelerator; correctness comes from L1 + recompute.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};

use crate::_0_cursor::Cursor;

pub struct DiskCache {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for DiskCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiskCache").finish_non_exhaustive()
    }
}

impl DiskCache {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS op_cache (\n\
                 key BLOB PRIMARY KEY,\n\
                 value BLOB NOT NULL,\n\
                 created_at INTEGER NOT NULL DEFAULT (unixepoch())\n\
             ) WITHOUT ROWID;",
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Read a cached batch. Returns `None` on miss, db error, or
    /// deserialize error.
    pub fn get(&self, key: &[u8; 32]) -> Option<Arc<[Cursor]>> {
        let conn = self.conn.lock().ok()?;
        let value: Option<Vec<u8>> = conn
            .query_row(
                "SELECT value FROM op_cache WHERE key = ?",
                params![&key[..]],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .ok()
            .flatten();
        let bytes = value?;
        let cursors: Vec<Cursor> = bincode::deserialize(&bytes).ok()?;
        Some(Arc::<[Cursor]>::from(cursors))
    }

    /// Write-through. Errors are dropped; callers see eventual recompute.
    pub fn insert(&self, key: [u8; 32], val: &[Cursor]) {
        let Ok(bytes) = bincode::serialize(&val.to_vec()) else { return };
        let Ok(conn) = self.conn.lock() else { return };
        let _ = conn.execute(
            "INSERT OR REPLACE INTO op_cache (key, value) VALUES (?, ?)",
            params![&key[..], &bytes],
        );
    }

    /// Phase F: bulk DELETE keyed eviction. Errors are dropped per the
    /// best-effort contract; on next miss the L1 recompute repopulates.
    pub fn delete_many(&self, keys: &[[u8; 32]]) {
        if keys.is_empty() { return; }
        let Ok(mut conn) = self.conn.lock() else { return };
        let Ok(tx) = conn.transaction() else { return };
        {
            let Ok(mut stmt) = tx.prepare("DELETE FROM op_cache WHERE key = ?")
            else { return };
            for k in keys {
                let _ = stmt.execute(params![&k[..]]);
            }
        }
        let _ = tx.commit();
    }

    /// Total persisted entries (test helper / telemetry).
    pub fn len(&self) -> usize {
        let Ok(conn) = self.conn.lock() else { return 0 };
        conn.query_row("SELECT COUNT(*) FROM op_cache", [], |row| row.get::<_, i64>(0))
            .map(|n| n as usize)
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_cursor::{Capture, CaptureKind, Cursor};
    use std::path::Path;
    use std::sync::Arc;

    fn tempdb(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("sprefa_disk_cache_{name}_{pid}_{nonce}.db"));
        let _ = std::fs::remove_file(&p);
        p
    }

    fn sample_cursor() -> Cursor {
        let mut c = Cursor::default();
        c.content = Arc::from(&b"hello world"[..]);
        c.byte_range = 0..11;
        c.repo = Arc::from("myorg/myrepo");
        c.rev = Arc::from("HEAD");
        c.fs = Some(Arc::from(Path::new("src/lib.rs")));
        c.content_hash = Some(Arc::new([7u8; 32]));
        c.last_bound = Some(Arc::from("X"));
        c.captures.push(Capture {
            name: Arc::from("X"),
            byte_range: 0..5,
            kind: CaptureKind::SpanBacked,
        });
        c.captures.push(Capture {
            name: Arc::from("Y"),
            byte_range: 0..0,
            kind: CaptureKind::Synthesized { value: Arc::from("synth-val") },
        });
        c
    }

    #[test]
    fn roundtrip_insert_get() {
        let path = tempdb("roundtrip");
        let cache = DiskCache::open(&path).unwrap();
        let key = [3u8; 32];
        let cursors = vec![sample_cursor()];
        assert!(cache.get(&key).is_none());
        cache.insert(key, &cursors);
        let hit = cache.get(&key).expect("hit");
        assert_eq!(hit.len(), 1);
        let h = &hit[0];
        let s = &cursors[0];
        assert_eq!(&*h.content, &*s.content);
        assert_eq!(h.byte_range, s.byte_range);
        assert_eq!(&*h.repo, &*s.repo);
        assert_eq!(&*h.rev, &*s.rev);
        assert_eq!(h.fs.as_deref(), s.fs.as_deref());
        assert_eq!(h.content_hash.as_deref(), s.content_hash.as_deref());
        assert_eq!(h.last_bound.as_deref(), s.last_bound.as_deref());
        assert_eq!(h.captures.len(), s.captures.len());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cross_process_simulation_drop_reopen() {
        let path = tempdb("crossproc");
        let key = [9u8; 32];
        let cursors = vec![sample_cursor()];
        {
            let cache = DiskCache::open(&path).unwrap();
            cache.insert(key, &cursors);
            assert_eq!(cache.len(), 1);
        } // drop closes the connection
        let cache = DiskCache::open(&path).unwrap();
        let hit = cache.get(&key).expect("hit after reopen");
        assert_eq!(hit.len(), 1);
        assert_eq!(&*hit[0].content, b"hello world");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn miss_returns_none() {
        let path = tempdb("miss");
        let cache = DiskCache::open(&path).unwrap();
        assert!(cache.get(&[0u8; 32]).is_none());
        let _ = std::fs::remove_file(&path);
    }
}
