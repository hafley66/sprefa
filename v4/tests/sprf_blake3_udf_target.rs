//! RED TEST — Step 6 of the rules-as-tables patch series.
//!
//! Spec: every `SqliteFactStore` connection registers a deterministic
//! scalar UDF `sprf_blake3_id` on open. The UDF computes the same
//! blake3 hash that Rust uses for `_id` derivation, so fused SQL can
//! call it from within `INSERT INTO <table>_facts SELECT
//! sprf_blake3_id('<table>', col_id, ...) ... FROM ...` without a
//! Rust round-trip.
//!
//! Properties:
//!   - Deterministic (SQLite can fold into expressions).
//!   - Variadic (`-1` arg count in rusqlite create_scalar_function).
//!   - Returns a 32-byte BLOB (raw blake3 output, NOT hex).
//!   - Bytewise identical to the Rust routine the fuser would have
//!     computed in-Rust per row.

use rusqlite::Connection;

#[test]
fn udf_registered_on_sqlite_fact_store_open() {
    // Opening a SqliteFactStore must register sprf_blake3_id on the
    // underlying connection. Calling it should succeed.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("facts.db");
    let store = v3_effect_runtime_v2_fact_store::SqliteFactStore::<v4::Cursor>::open_file(&path)
        .expect("open SqliteFactStore");
    let conn = store.conn_for_test();

    let blob: Vec<u8> = conn
        .query_row(
            "SELECT sprf_blake3_id('hits', 1, 2, 3)",
            [],
            |r| r.get(0),
        )
        .expect("sprf_blake3_id must be registered and callable");

    assert_eq!(blob.len(), 32, "must return raw 32-byte BLOB, got {} bytes", blob.len());
}

#[test]
fn udf_matches_rust_blake3_bytewise() {
    // The UDF output must be byte-identical to what `v4`'s in-Rust
    // blake3 routine produces for the same input. Otherwise fused SQL
    // and Rust-streamed paths would emit different _id values for the
    // same row content.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("facts.db");
    let store = v3_effect_runtime_v2_fact_store::SqliteFactStore::<v4::Cursor>::open_file(&path)
        .expect("open SqliteFactStore");
    let conn = store.conn_for_test();

    let table = "hits";
    let cols: [i64; 3] = [42, 17, 99];

    let sql_blob: Vec<u8> = conn
        .query_row(
            "SELECT sprf_blake3_id(?, ?, ?, ?)",
            rusqlite::params![table, cols[0], cols[1], cols[2]],
            |r| r.get(0),
        )
        .unwrap();

    let rust_blob = v4::compile::lower::blake3_id_for_test(table, &cols);

    assert_eq!(
        sql_blob, rust_blob,
        "SQL UDF and Rust routine must produce identical bytes"
    );
}

#[test]
fn udf_is_deterministic_flag_set() {
    // SQLite needs the deterministic flag to fold `sprf_blake3_id(a, b)`
    // into expression-level optimizations. Without it, the planner
    // cannot reuse the result across an `INSERT ... SELECT` pass.
    //
    // Verifiable via repeat call yielding identical bytes (necessary
    // but not sufficient; the registration code must set the flag).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("facts.db");
    let store = v3_effect_runtime_v2_fact_store::SqliteFactStore::<v4::Cursor>::open_file(&path)
        .expect("open SqliteFactStore");
    let conn = store.conn_for_test();

    let a: Vec<u8> = conn
        .query_row("SELECT sprf_blake3_id('t', 1, 2)", [], |r| r.get(0))
        .unwrap();
    let b: Vec<u8> = conn
        .query_row("SELECT sprf_blake3_id('t', 1, 2)", [], |r| r.get(0))
        .unwrap();
    assert_eq!(a, b, "deterministic UDF must produce stable output");

    // Inspect the function table via pragma_function_list (SQLite 3.34+).
    // The `flags` column has bit 0x800 set when SQLITE_DETERMINISTIC.
    let has_deterministic: bool = conn
        .query_row(
            "SELECT (flags & 0x800) != 0 FROM pragma_function_list WHERE name='sprf_blake3_id' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(false);
    assert!(
        has_deterministic,
        "sprf_blake3_id must be registered with SQLITE_DETERMINISTIC"
    );
}

#[test]
fn udf_used_in_insert_select_fused_path() {
    // End-to-end shape: a fused INSERT...SELECT using sprf_blake3_id
    // produces the same _id values the Rust-streamed path would.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("facts.db");
    let conn = Connection::open(&path).unwrap();
    // The fact-store open call registers sprf_blake3_id. Simulate.
    v3_effect_runtime_v2_fact_store::register_sprf_udfs(&conn)
        .expect("UDF registration helper must exist");

    // We mirror the fuser's actual emission: INSERT OR IGNORE. The
    // deterministic UDF causes the duplicate (FS=1, LO=100) row to
    // share an _id with the first one; OR IGNORE silently drops it,
    // so the final count is the number of distinct (FS, LO) tuples.
    conn.execute_batch(
        r#"
        CREATE TABLE hits_facts (
          _id BLOB PRIMARY KEY,
          FS  INTEGER NOT NULL,
          LO  INTEGER NOT NULL
        ) WITHOUT ROWID;
        INSERT OR IGNORE INTO hits_facts (_id, FS, LO)
          SELECT sprf_blake3_id('hits', FS, LO), FS, LO
          FROM (SELECT 1 AS FS, 100 AS LO
                UNION ALL SELECT 1, 200
                UNION ALL SELECT 1, 100);   -- duplicate by content
        "#,
    )
    .unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM hits_facts", [], |r| r.get(0))
        .unwrap();
    // Two distinct (FS, LO) tuples in the source, so OR IGNORE keeps
    // two rows. Three would mean the UDF wasn't deterministic.
    assert_eq!(count, 2, "duplicate (FS,LO) must collapse to one row via deterministic UDF");
}
