//! `SprfStore` Layer 0b boundary contract.
//!
//! Validates the content-derived id model and the LRU/cold rehydrate
//! invariants. Sentinels are pre-inserted at id 0 for all three families.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, MemFactStore};
use v4::store::{FILES_TABLE, REFS_TABLE, STRINGS_TABLE, SprfStore};
use v4::{Coord, Cursor, Ref, StringId};

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
fn intern_ref_dedups_on_span_tuple() {
    let s = fresh_store();
    let f = s.intern_file(b"abc", "/x");

    let r1 = s.intern_ref(Coord { repo: 0, rev: 0, fs: f, lo: 0, hi: 3 });
    let r2 = s.intern_ref(Coord { repo: 0, rev: 0, fs: f, lo: 0, hi: 3 });
    let r3 = s.intern_ref(Coord { repo: 0, rev: 0, fs: f, lo: 1, hi: 3 });
    assert_eq!(r1, r2);
    assert_ne!(r1, r3);
    assert_eq!(s.inner().len(REFS_TABLE), 3, "sentinel + 2 unique coords");
    assert_eq!(
        s.coord_of(r1),
        Some(Coord { repo: 0, rev: 0, fs: f, lo: 0, hi: 3 }),
    );
}

#[test]
fn sentinels_pre_inserted() {
    let s = fresh_store();
    assert_eq!(s.lookup_string(StringId::EMPTY).as_deref(), Some(""));
    assert_eq!(s.coord_of(Ref::SYNTHETIC), Some(Coord::default()));
    let (hash, path) = s.lookup_file(0).expect("synthetic file resolves");
    assert_eq!(hash, [0u8; 32], "synthetic file_id has zero hash");
    assert_eq!(&*path, "\u{2205}");
}

#[test]
fn norm_columns_populated() {
    let s = fresh_store();
    let raw = "  Hello   World  ";
    let id = s.intern_string(raw);

    let id_str = id.0.to_string();
    let row = s.inner().rows_of(STRINGS_TABLE)
        .into_iter()
        .find(|r| r.get("id").as_deref() == Some(id_str.as_str()))
        .expect("row exists for interned id");

    assert_eq!(row.get("content").as_deref(), Some(raw));
    assert_eq!(row.get("norm_ws").as_deref(), Some("Hello World"));
    assert_eq!(row.get("norm_case").as_deref(), Some("  hello   world  "));
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
