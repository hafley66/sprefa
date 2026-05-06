//! `SprfStore` boundary contract.
//!
//! Proves the v0 strings/refs/files invariants in the new layer:
//!   • intern_string is idempotent — same content → same StringId.
//!   • intern_file is content-addressable — two paths with identical
//!     bytes share one FileId, distinct bytes get distinct ids.
//!   • intern_ref dedups on (file, lo, hi).
//!   • each intern call appends exactly one row to its meta-table.
//!   • bind_capture stamps both legacy text and _REF/_STRING columns
//!     so existing op consumers keep working during the migration.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, MemFactStore};
use v4::store::{FILES_TABLE, REFS_TABLE, STRINGS_TABLE, SprfStore};
use v4::Cursor;

fn fresh_store() -> Arc<SprfStore> {
    let inner: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    SprfStore::new(inner)
}

#[test]
fn intern_string_dedups_and_writes_one_row() {
    let s = fresh_store();
    let a = s.intern_string("hello");
    let b = s.intern_string("hello");
    let c = s.intern_string("world");

    assert_eq!(a, b, "same content must hash to same StringId");
    assert_ne!(a, c, "different content must hash to different StringId");
    assert_eq!(s.inner().len(STRINGS_TABLE), 2, "one row per unique string");
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
    assert_eq!(s.inner().len(FILES_TABLE), 2);

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

    let r1 = s.intern_ref(f, 0, 3);
    let r2 = s.intern_ref(f, 0, 3);
    let r3 = s.intern_ref(f, 1, 3);
    assert_eq!(r1, r2);
    assert_ne!(r1, r3);
    assert_eq!(s.inner().len(REFS_TABLE), 2);
    assert_eq!(s.lookup_ref(r1), Some((f, 0, 3)));
}

#[test]
fn bind_capture_stamps_text_and_fk_columns() {
    let s = fresh_store();
    let f = s.intern_file(b"fn foo(){}", "/m.rs");

    let mut cur = Cursor::default();
    s.bind_capture(&mut cur, "NAME", f, 3, 6, "foo");

    assert_eq!(cur.get("NAME"), Some("foo"), "legacy text stamp preserved");
    let ref_id_str    = cur.get("NAME_REF").expect("_REF stamped");
    let string_id_str = cur.get("NAME_STRING").expect("_STRING stamped");
    let ref_id:    u64 = ref_id_str.parse().unwrap();
    let string_id: u64 = string_id_str.parse().unwrap();

    assert_eq!(s.lookup_ref(ref_id), Some((f, 3, 6)));
    assert_eq!(s.lookup_string(string_id).as_deref(), Some("foo"));
}
