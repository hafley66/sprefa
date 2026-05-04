// Smoke test: SqliteStore round-trips writes + IN-batch reads through Store trait.
//
// Two tables: files(file_id, path, content_hash) and refs(file_id, lo, hi).
// Insert a handful of rows, commit, read refs WHERE file_id IN (a, b),
// verify projected cols come back grouped correctly.

use std::sync::Arc;
use v4::*;

fn cur(pairs: &[(&str, &str)]) -> Cursor {
    let mut c = Cursor::default();
    for (k, v) in pairs { c.set(*k, *v); }
    c
}

#[tokio::test]
async fn sqlite_in_clause_round_trip() {
    let store: Arc<dyn Store> = SqliteStore::open_memory();

    store.ensure_schema("files", &["file_id", "path", "content_hash"]);
    store.ensure_schema("refs",  &["file_id", "lo", "hi"]);

    store.insert_many("files", vec![
        cur(&[("file_id", "1"), ("path", "a.rs"), ("content_hash", "aaaa")]),
        cur(&[("file_id", "2"), ("path", "b.rs"), ("content_hash", "bbbb")]),
        cur(&[("file_id", "3"), ("path", "c.rs"), ("content_hash", "cccc")]),
    ], 0).await;

    store.insert_many("refs", vec![
        cur(&[("file_id", "1"), ("lo",  "10"), ("hi",  "20")]),
        cur(&[("file_id", "1"), ("lo",  "30"), ("hi",  "40")]),
        cur(&[("file_id", "2"), ("lo", "100"), ("hi", "120")]),
        cur(&[("file_id", "3"), ("lo", "200"), ("hi", "210")]),
    ], 0).await;

    store.commit(0).await;

    // snapshot: full table
    let all_files = store.snapshot("files").await;
    assert_eq!(all_files.len(), 3);

    // read_in: fetch refs for file_id IN (1, 2)
    let rows = store.read_in(
        "refs",
        "file_id",
        vec!["1".into(), "2".into()],
        vec!["lo".into(), "hi".into()],
    ).await;
    assert_eq!(rows.len(), 3, "expected 2 rows for file 1 + 1 row for file 2");

    // every returned cursor carries file_id, lo, hi
    for c in &rows {
        assert!(c.get("file_id").is_some());
        assert!(c.get("lo").is_some());
        assert!(c.get("hi").is_some());
    }

    // empty IN list returns nothing
    let empty = store.read_in("refs", "file_id", vec![], vec!["lo".into()]).await;
    assert!(empty.is_empty());
}

#[tokio::test]
async fn mem_store_read_in_parity() {
    // MemStore.read_in scans HashMap; should match SqliteStore behavior shape.
    let mem: Arc<dyn Store> = MemStore::new();
    mem.ensure_schema("refs", &["file_id", "lo", "hi"]); // no-op for mem
    mem.insert_many("refs", vec![
        cur(&[("file_id", "1"), ("lo", "10"), ("hi", "20")]),
        cur(&[("file_id", "2"), ("lo", "30"), ("hi", "40")]),
    ], 0).await;

    let rows = mem.read_in(
        "refs",
        "file_id",
        vec!["1".into()],
        vec!["lo".into(), "hi".into()],
    ).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("file_id"), Some("1"));
    assert_eq!(rows[0].get("lo"), Some("10"));
}
