//! Layer 4 (subset) — `refs_at` RPC.
//!
//! Seed `_files`, `_paths`, `_refs` directly via `SprfStore` then drive
//! the RPC over the in-process router. Auto-views and LSP hover
//! enrichment are deferred follow-ups.

use v4::app::{build_in_process, RefsAtReq, SprfClient};
use v4::Coord;

#[tokio::test]
async fn refs_at_returns_seeded_ref_for_covering_byte() {
    let (state, client) = build_in_process(std::env::temp_dir());
    let store = state.sprf_store.clone();

    let file = store.intern_file(b"hello world\n", "/tmp/foo.txt");
    let _path = store.intern_path(0, 0, file, "/tmp/foo.txt");
    let r = store.intern_ref(Coord {
        repo: 0,
        rev: 0,
        fs: file,
        lo: 6,
        hi: 11,
    });

    let resp = client
        .refs_at(RefsAtReq {
            path: "/tmp/foo.txt".into(),
            byte: 7,
        })
        .await
        .unwrap();

    assert_eq!(resp.hits.len(), 1, "byte 7 sits inside [6,11)");
    let hit = &resp.hits[0];
    assert_eq!(hit.ref_id, r.0);
    assert_eq!(hit.coord.lo, 6);
    assert_eq!(hit.coord.hi, 11);
    assert_eq!(hit.coord.fs, file);
    assert_eq!(hit.coord.fs_path.as_deref(), Some("/tmp/foo.txt"));
}

#[tokio::test]
async fn refs_at_out_of_range_byte_is_empty() {
    let (state, client) = build_in_process(std::env::temp_dir());
    let store = state.sprf_store.clone();

    let file = store.intern_file(b"hello world\n", "/tmp/foo.txt");
    let _path = store.intern_path(0, 0, file, "/tmp/foo.txt");
    let _r = store.intern_ref(Coord {
        repo: 0,
        rev: 0,
        fs: file,
        lo: 6,
        hi: 11,
    });

    let resp = client
        .refs_at(RefsAtReq {
            path: "/tmp/foo.txt".into(),
            byte: 100,
        })
        .await
        .unwrap();

    assert!(
        resp.hits.is_empty(),
        "byte 100 falls outside every seeded ref"
    );
}

#[tokio::test]
async fn refs_at_unknown_path_is_empty() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let resp = client
        .refs_at(RefsAtReq {
            path: "/no/such/file".into(),
            byte: 0,
        })
        .await
        .unwrap();

    assert!(
        resp.hits.is_empty(),
        "unknown path resolves no FileId, no hits"
    );
}
