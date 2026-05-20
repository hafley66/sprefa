//! Phase 5: `MemoDepsCache::by_path` reverse index.
//!
//! Covers the acceptance scenarios from `v4/plans/lsp-diags-to-claude-code-full.md`
//! §10 Phase 5 (lines after 2026-05-20 revision):
//!   1. record-and-lookup
//!   2. diff-replace drops orphans
//!   3. cold-load seeds by_path (in-memory FactStore path)
//!   4. canonicalization symmetry (macOS /var vs /private/var)
//!   5. empty source_path (SQL mount) NOT in by_path; still in forward
//!   6. singular `record_memo_dep` write-through

use std::sync::Arc;

use effect_runtime::v2::{FactStore, MemFactStore};
use v4::runtime_graph::{MemoDepsCache, RuntimeGraph, MEMO_DEPS_TABLE};
use v4::source_clock::SourceId;
use v4::store::SprfStore;
use v4::Cursor;

fn mem_graph() -> RuntimeGraph {
    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    RuntimeGraph::new(store, facts)
}

fn deprow(path: &str) -> (SourceId, u64, String, String) {
    (SourceId::for_file(path), 1, "content_hex".into(), path.into())
}

#[test]
fn record_and_lookup_by_path() {
    let tmp = tempfile::tempdir().unwrap();
    let p1 = tmp.path().join("a.rs");
    let p2 = tmp.path().join("b.rs");
    std::fs::write(&p1, "fn a() {}\n").unwrap();
    std::fs::write(&p2, "fn b() {}\n").unwrap();

    let g = mem_graph();
    g.record_memo_deps(
        "owner_hex_1",
        "in_key_1",
        &[
            deprow(p1.to_str().unwrap()),
            deprow(p2.to_str().unwrap()),
        ],
    );

    assert!(g.memo_dep_contains_path(&p1));
    assert!(g.memo_dep_contains_path(&p2));
    let owners1 = g.memo_dep_owners_for_path(&p1);
    assert_eq!(owners1, vec![("owner_hex_1".into(), "in_key_1".into())]);
    let owners2 = g.memo_dep_owners_for_path(&p2);
    assert_eq!(owners2, vec![("owner_hex_1".into(), "in_key_1".into())]);
}

#[test]
fn diff_replace_drops_orphans_from_by_path() {
    let tmp = tempfile::tempdir().unwrap();
    let p1 = tmp.path().join("a.rs");
    let p2 = tmp.path().join("b.rs");
    let p3 = tmp.path().join("c.rs");
    for p in [&p1, &p2, &p3] {
        std::fs::write(p, "fn x() {}\n").unwrap();
    }

    let g = mem_graph();
    g.record_memo_deps(
        "O",
        "K",
        &[deprow(p1.to_str().unwrap()), deprow(p2.to_str().unwrap())],
    );
    assert!(g.memo_dep_contains_path(&p1));
    assert!(g.memo_dep_contains_path(&p2));

    // Re-record with {p2, p3}: p1 must drop from by_path.
    g.record_memo_deps(
        "O",
        "K",
        &[deprow(p2.to_str().unwrap()), deprow(p3.to_str().unwrap())],
    );
    assert!(!g.memo_dep_contains_path(&p1), "orphaned p1 must be removed");
    assert!(g.memo_dep_contains_path(&p2));
    assert!(g.memo_dep_contains_path(&p3));

    let owners1 = g.memo_dep_owners_for_path(&p1);
    assert!(owners1.is_empty(), "owners_of_path(p1) must be empty after orphan prune");
}

#[test]
fn cold_load_from_factstore_seeds_by_path() {
    let tmp = tempfile::tempdir().unwrap();
    let p1 = tmp.path().join("preloaded.rs");
    std::fs::write(&p1, "fn x() {}\n").unwrap();

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    facts.declare(
        MEMO_DEPS_TABLE,
        &[
            "owner_op_id",
            "in_key",
            "source_id",
            "gen_seen",
            "content_id",
            "source_path",
        ],
    );
    let mut row = Cursor::default();
    row.set("owner_op_id", "preload_owner");
    row.set("in_key", "preload_ik");
    row.set("source_id", SourceId::for_file(p1.to_str().unwrap()).hex());
    row.set("gen_seen", "1");
    row.set("content_id", "cid");
    row.set("source_path", p1.to_str().unwrap());
    facts.insert(MEMO_DEPS_TABLE, Arc::new(row));

    let store = SprfStore::new(facts.clone());
    let g = RuntimeGraph::new(store, facts);
    // The lookup forces cache load; by_path must have been seeded.
    let owners = g.memo_dep_owners_for_path(&p1);
    assert_eq!(
        owners,
        vec![("preload_owner".into(), "preload_ik".into())],
        "cold-loaded MEMO_DEPS row must populate by_path",
    );
    assert!(g.memo_dep_contains_path(&p1));
}

#[test]
fn canonicalization_resolves_macos_var_symlink() {
    // tempfile::tempdir() returns under /var/folders/... on macOS; dunce
    // resolves to /private/var/folders/... . Record under /var, look up
    // under /private/var. Both must canonicalize to the same key.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("target.rs");
    std::fs::write(&p, "fn x() {}\n").unwrap();

    let g = mem_graph();
    g.record_memo_deps("O", "K", &[deprow(p.to_str().unwrap())]);

    // canonical form (may differ from the path tempdir returned on macOS).
    let canon = dunce::canonicalize(&p).unwrap();
    assert!(g.memo_dep_contains_path(&canon));
    let owners = g.memo_dep_owners_for_path(&canon);
    assert_eq!(owners, vec![("O".into(), "K".into())]);

    // The un-canonicalized form must also hit (the method canonicalizes
    // internally).
    assert!(g.memo_dep_contains_path(&p));
}

#[test]
fn empty_source_path_stays_in_forward_not_in_by_path() {
    // SQL-mount call sites at mounted_query.rs:430-431,514-515 pass
    // empty `source_path`. The dirty-sweep still finds them via
    // `memo_dep_owners_for_source`, but `by_path` must NOT key an empty
    // PathBuf.
    let g = mem_graph();
    let sid = SourceId::for_file("table_only");
    g.record_memo_deps(
        "sql_owner",
        "sql_ik",
        &[(sid, 1, "cid".into(), String::new())],
    );

    // Forward still finds it via source-id sweep.
    let owners = g.memo_dep_owners_for_source(sid);
    assert_eq!(owners, vec![("sql_owner".into(), "sql_ik".into())]);

    // by_path has no empty-PathBuf entry.
    let empty = std::path::Path::new("");
    assert!(!g.memo_dep_contains_path(empty));
}

#[test]
fn singular_record_memo_dep_writes_through() {
    let tmp = tempfile::tempdir().unwrap();
    let p1 = tmp.path().join("s1.rs");
    let p2 = tmp.path().join("s2.rs");
    std::fs::write(&p1, "fn x() {}\n").unwrap();
    std::fs::write(&p2, "fn y() {}\n").unwrap();

    let g = mem_graph();
    let sid1 = SourceId::for_file(p1.to_str().unwrap());
    let sid2 = SourceId::for_file(p2.to_str().unwrap());
    g.record_memo_dep("O", "K", sid1, 1, "c1", p1.to_str().unwrap());
    g.record_memo_dep("O", "K", sid2, 1, "c2", p2.to_str().unwrap());

    assert!(g.memo_dep_contains_path(&p1));
    assert!(g.memo_dep_contains_path(&p2));
    let owners = g.memo_dep_owners_for_path(&p1);
    assert_eq!(owners, vec![("O".into(), "K".into())]);
}

#[test]
fn memo_deps_cache_public_surface_compiles() {
    // Smoke check: `MemoDepsCache::new()` and its accessors are public.
    // If this fails to compile, Phase 6 cannot construct or inspect the
    // cache shape in tests.
    let c = MemoDepsCache::new();
    assert!(c.forward.is_empty());
    assert!(c.by_path.is_empty());
    assert!(!c.contains_path(std::path::Path::new("/nope")));
    assert!(c.owners_of_path(std::path::Path::new("/nope")).is_empty());
}
