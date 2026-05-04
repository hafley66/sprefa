//! `SqliteMutationStore<T>` — durable counterpart to `MutationStore<T>`.
//!
//! Same put/peek/take surface, backed by sqlite. Keyed by `NextKey`
//! (32-byte BLOB). Result blob produced via `Codec::encode`.
//!
//! Phase F.4: combined with `SqliteQueue`, this is what gives v4 the
//! redux-persist guarantee — close the file mid-flight, reopen, and
//! every parked row whose mutation completed already finds its result
//! still here. Re-dispatch is unnecessary.
//!
//! Connection is shared (`Arc<Mutex<Connection>>`); usually the same
//! connection as `SqliteQueue` so all stores commit atomically with
//! the relational fact store on the consumer side.

use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use super::codec::Codec;
use super::next::Next;
use super::next_key::NextKey;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sprf_v2_mutation_store (
  key   BLOB PRIMARY KEY,
  blob  BLOB NOT NULL
);
";

pub struct SqliteMutationStore<T: Next + Codec> {
    conn:    Arc<Mutex<Connection>>,
    _marker: PhantomData<fn() -> T>,
}

impl<T: Next + Codec> SqliteMutationStore<T> {
    pub fn open(conn: Arc<Mutex<Connection>>) -> Self {
        {
            let c = conn.lock().unwrap();
            c.execute_batch(SCHEMA).expect("mutation_store schema");
        }
        Self { conn, _marker: PhantomData }
    }

    pub fn put(&self, key: NextKey, value: Arc<T>) {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO sprf_v2_mutation_store (key, blob) VALUES (?1, ?2)",
            params![key.0.to_vec(), value.encode()],
        ).expect("mutation_store put");
    }

    pub fn peek(&self, key: NextKey) -> Option<Arc<T>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT blob FROM sprf_v2_mutation_store WHERE key = ?1",
            params![key.0.to_vec()],
            |r| {
                let blob: Vec<u8> = r.get(0)?;
                Ok(Arc::new(T::decode(&blob)))
            },
        ).optional().expect("mutation_store peek")
    }

    pub fn take(&self, key: NextKey) -> Option<Arc<T>> {
        let conn = self.conn.lock().unwrap();
        let v = conn.query_row(
            "SELECT blob FROM sprf_v2_mutation_store WHERE key = ?1",
            params![key.0.to_vec()],
            |r| {
                let blob: Vec<u8> = r.get(0)?;
                Ok(Arc::new(T::decode(&blob)))
            },
        ).optional().expect("mutation_store take/select");
        if v.is_some() {
            conn.execute(
                "DELETE FROM sprf_v2_mutation_store WHERE key = ?1",
                params![key.0.to_vec()],
            ).expect("mutation_store delete");
        }
        v
    }

    pub fn len(&self) -> usize {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sprf_v2_mutation_store",
            [],
            |r| r.get(0),
        ).expect("mutation_store len");
        n as usize
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }
}
