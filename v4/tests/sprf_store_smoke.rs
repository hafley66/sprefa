//! `SprfStore` Layer 0b boundary contract.
//!
//! Validates the content-derived id model and the LRU/cold rehydrate
//! invariants. Sentinels are pre-inserted at id 0 for all three families.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, MemFactStore};
use v4::store::{
    FILES_TABLE, PATHS_TABLE, REPOS_TABLE, REVS_TABLE, STRING_OBSERVATIONS_TABLE,
    STRINGS_TABLE, WHERE_BYTES_TABLE, SprfStore,
};
use v4::{Cursor, StringId, WhereBytes, WhereBytesId};

fn fresh_store() -> Arc<SprfStore> {
    let inner: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    SprfStore::new(inner)
}

fn fresh_store_with_caps(s: usize, f: usize, r: usize) -> Arc<SprfStore> {
    let inner: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    SprfStore::with_caps(inner, s, f, r)
}

#[test]
fn intern_string_dedups_and_writes_one_row() {
    let s = fresh_store();
    let a = s.intern_string("hello");
    let b = s.intern_string("hello");
    let c = s.intern_string("world");

    assert_eq!(a, b, "same content must hash to same StringId");
    assert_ne!(a, c, "different content must hash to different StringId");
    assert_eq!(s.inner().len(STRINGS_TABLE), 3, "sentinel + hello + world");
    assert_eq!(s.lookup_string(a).as_deref(), Some("hello"));
    assert_eq!(s.lookup_string(c).as_deref(), Some("world"));
}

#[test]
fn intern_file_is_content_addressable() {
    let s = fresh_store();
    let bytes = b"fn main() { println!(\"hi\") }";
    let id_a = s.intern_file(bytes, "/repo-a/src/main.rs");
    let id_b = s.intern_file(bytes, "/repo-b/elsewhere/main.rs");
    let id_c = s.intern_file(b"fn main() { println!(\"bye\") }", "/repo-a/src/main.rs");

    assert_eq!(
        id_a, id_b,
        "identical content under two paths must collapse to one FileId",
    );
    assert_ne!(id_a, id_c, "different content under same path → different FileId");
    assert_eq!(s.inner().len(FILES_TABLE), 3, "sentinel + 2 unique contents");

    let (hash, first_path) = s.lookup_file(id_a).expect("file_id resolves");
    assert_eq!(hash.len(), 32);
    assert_eq!(
        &*first_path, "/repo-a/src/main.rs",
        "first-seen path is sticky; second path doesn't overwrite",
    );
}

#[test]
fn intern_where_bytes_dedups_on_span_tuple() {
    let s = fresh_store();
    let f = s.intern_file(b"abc", "/x");
    let abc = s.intern_string("abc");
    let bc = s.intern_string("bc");

    let r1 = s.intern_where_bytes(WhereBytes { string: abc, repo: 0, rev: 0, file: f, lo: 0, hi: 3 });
    let r2 = s.intern_where_bytes(WhereBytes { string: abc, repo: 0, rev: 0, file: f, lo: 0, hi: 3 });
    let r3 = s.intern_where_bytes(WhereBytes { string: bc, repo: 0, rev: 0, file: f, lo: 1, hi: 3 });
    assert_eq!(r1, r2);
    assert_ne!(r1, r3);
    assert_eq!(s.inner().len(WHERE_BYTES_TABLE), 3, "sentinel + 2 unique byte ranges");
    assert_eq!(
        s.where_bytes_of(r1),
        Some(WhereBytes { string: abc, repo: 0, rev: 0, file: f, lo: 0, hi: 3 }),
    );
}

#[test]
fn where_bytes_id_includes_string_id() {
    let s = fresh_store();
    let f = s.intern_file(b"abcdef", "/x");
    let abc = s.intern_string("abc");
    let xyz = s.intern_string("xyz");

    let a = s.intern_where_bytes(WhereBytes { string: abc, repo: 0, rev: 0, file: f, lo: 0, hi: 3 });
    let b = s.intern_where_bytes(WhereBytes { string: xyz, repo: 0, rev: 0, file: f, lo: 0, hi: 3 });

    assert_ne!(a, b);
    assert_eq!(s.inner().len(WHERE_BYTES_TABLE), 3, "sentinel + 2 located strings sharing one span");
}

#[test]
fn sentinels_pre_inserted() {
    let s = fresh_store();
    assert_eq!(s.lookup_string(StringId::EMPTY).as_deref(), Some(""));
    assert_eq!(s.where_bytes_of(WhereBytesId::SYNTHETIC), Some(WhereBytes::default()));
    let (hash, path) = s.lookup_file(0).expect("synthetic file resolves");
    assert_eq!(hash, [0u8; 32], "synthetic file_id has zero hash");
    assert_eq!(&*path, "\u{2205}");
}

#[test]
fn norm_columns_populated() {
    let s = fresh_store();
    let raw = "ApiService/api_service/api-service";
    let id = s.intern_string(raw);

    let id_str = id.0.to_string();
    let row = s.inner().rows_of(STRINGS_TABLE)
        .into_iter()
        .find(|r| r.get("id").as_deref() == Some(id_str.as_str()))
        .expect("row exists for interned id");

    assert_eq!(row.get("content").as_deref(), Some(raw));
    assert_eq!(row.get("norm").as_deref(), Some("apiserviceapiserviceapiservice"));
}

#[test]
fn string_observations_are_role_rows() {
    let s = fresh_store();
    let file = s.intern_file(b"const ApiService = 1;", "/src/api.ts");
    let string = s.intern_string("ApiService");
    let where_bytes = s.intern_where_bytes(WhereBytes {
        string,
        repo: 0,
        rev: 0,
        file,
        lo: 6,
        hi: 16,
    });

    s.observe_string(string, "identifier", where_bytes, "", 0);
    s.observe_string(string, "identifier", where_bytes, "", 0);
    s.observe_string(string, "string_literal", where_bytes, "", 0);

    let rows = s.inner().rows_of(STRING_OBSERVATIONS_TABLE);
    assert_eq!(rows.len(), 3, "sentinel + 2 distinct role observations");
}

#[test]
fn content_stable_across_instances() {
    let a = fresh_store();
    let b = fresh_store();
    let id_a = a.intern_string("portable");
    let id_b = b.intern_string("portable");
    assert_eq!(id_a, id_b, "ids are content-derived; no sequential leak");
}

#[test]
fn intern_repo_dedups_and_seeds_sentinel() {
    let s = fresh_store();
    let a = s.intern_repo("sprefa", "git@github.com:hafley/sprefa");
    let b = s.intern_repo("sprefa", "git@github.com:hafley/sprefa");
    assert_ne!(a, 0, "real repo must not collide with sentinel");
    assert_eq!(a, b, "same (slug, remote) must hash to same RepoId");

    let (slug0, remote0) = s.lookup_repo(0).expect("sentinel repo resolves");
    assert_eq!(&*slug0, "");
    assert_eq!(&*remote0, "");

    let (slug, remote) = s.lookup_repo(a).expect("interned repo resolves");
    assert_eq!(&*slug, "sprefa");
    assert_eq!(&*remote, "git@github.com:hafley/sprefa");

    assert_eq!(s.inner().len(REPOS_TABLE), 2, "sentinel + one repo");
}

#[test]
fn intern_rev_dedups_per_repo() {
    let s = fresh_store();
    let r1 = s.intern_repo("sprefa", "git@github.com:hafley/sprefa");
    let r2 = s.intern_repo("other",  "git@github.com:hafley/other");
    assert_ne!(r1, r2);

    let oid = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
    let v1  = s.intern_rev(r1, oid, 100);
    let v1b = s.intern_rev(r1, oid, 100);
    let v2  = s.intern_rev(r2, oid, 100);

    assert_eq!(v1, v1b, "same (repo, oid) must dedup");
    assert_ne!(v1, v2, "same oid under different repos must not collide");

    let (got_repo, got_oid, got_ts) = s.lookup_rev(v1).expect("rev resolves");
    assert_eq!(got_repo, r1);
    assert_eq!(&*got_oid, oid);
    assert_eq!(got_ts, 100);

    assert_eq!(s.inner().len(REVS_TABLE), 3, "sentinel + two revs");
}

#[test]
fn intern_path_dedups_on_full_tuple() {
    let s = fresh_store();
    let repo = s.intern_repo("sprefa", "remote");
    let rev  = s.intern_rev(repo, "abc", 0);
    let file = s.intern_file(b"contents", "/a/b.rs");

    let p1  = s.intern_path(repo, rev, file, "/a/b.rs");
    let p1b = s.intern_path(repo, rev, file, "/a/b.rs");
    let p2  = s.intern_path(repo, rev, file, "/a/c.rs");
    let p3  = s.intern_path(repo, rev + 1, file, "/a/b.rs");

    assert_eq!(p1, p1b, "same (repo, rev, file, path) must dedup");
    assert_ne!(p1, p2, "differing path → different PathId");
    assert_ne!(p1, p3, "differing rev → different PathId");

    // Sentinel + 3 distinct rows.
    assert_eq!(s.inner().len(PATHS_TABLE), 4);
}

#[test]
fn find_where_bytes_covering_returns_covering_ids() {
    let s = fresh_store();
    let f = s.intern_file(b"some bytes for indexing", "/x.rs");

    let r1 = s.intern_where_bytes(WhereBytes { string: s.intern_string("some bytes"), repo: 0, rev: 0, file: f, lo:  0, hi: 10 });
    let r2 = s.intern_where_bytes(WhereBytes { string: s.intern_string("bytes for"), repo: 0, rev: 0, file: f, lo:  5, hi: 15 });
    let r3 = s.intern_where_bytes(WhereBytes { string: s.intern_string("indexing"), repo: 0, rev: 0, file: f, lo: 20, hi: 30 });

    // byte=7 is inside r1 (0..10) and r2 (5..15).
    let mut at_7 = s.find_where_bytes_covering(f, 7);
    at_7.sort();
    let mut want = vec![r1, r2];
    want.sort();
    assert_eq!(at_7, want);

    // byte=25 is inside r3 only.
    assert_eq!(s.find_where_bytes_covering(f, 25), vec![r3]);

    // byte=100 covers nothing.
    assert!(s.find_where_bytes_covering(f, 100).is_empty());

    // unknown file is empty.
    assert!(s.find_where_bytes_covering(0xdead_beef_dead_beef, 7).is_empty());
}

#[test]
fn find_file_by_path_first_match() {
    let s = fresh_store();
    let repo = s.intern_repo("sprefa", "remote");
    let rev  = s.intern_rev(repo, "abc", 0);
    let f    = s.intern_file(b"contents", "/a/b.rs");

    // Two _paths rows pointing at the same FileId.
    let _p1 = s.intern_path(repo, rev, f, "/a/b.rs");
    let _p2 = s.intern_path(repo, rev, f, "/a/c.rs");

    assert_eq!(s.find_file_by_path("/a/b.rs"), Some(f));
    assert_eq!(s.find_file_by_path("/a/c.rs"), Some(f));
    assert_eq!(s.find_file_by_path("/nope"), None);
}

#[test]
fn path_of_returns_first_seen() {
    let s = fresh_store();
    let f = s.intern_file(b"contents", "/first/seen.rs");
    // A second intern_file with a different path must NOT overwrite
    // the first-seen path on the _files row.
    let _ = s.intern_file(b"contents", "/second/seen.rs");

    let path = s.path_of(f).expect("path_of resolves first-seen path");
    assert_eq!(&*path, "/first/seen.rs");
}

#[test]
fn lru_eviction_invisible() {
    let s = fresh_store_with_caps(2, 2, 2);
    let words = ["one", "two", "three", "four", "five"];
    let ids: Vec<_> = words.iter().map(|w| s.intern_string(w)).collect();

    for (i, id) in ids.iter().enumerate() {
        let got = s.lookup_string(*id);
        assert_eq!(
            got.as_deref(),
            Some(words[i]),
            "cold-tier rehydrate after LRU eviction for {:?}", words[i],
        );
    }
}
