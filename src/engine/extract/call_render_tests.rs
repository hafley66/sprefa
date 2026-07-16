//! Tier T2 render-flip tests for `Engine::flip_call_rels_via_router`.
//!
//! These tests live in their own file (included under `#[cfg(test)]` from
//! `src/engine/extract/call.rs`) so the component-level contract — cold
//! `reload_rel`, warm `retract_rows` + `insert_rows`, one transaction, memo in
//! `self.call_router` — is pinned without touching the integration test tree.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::ast::Value;
use crate::db;
use crate::engine::Engine;
use crate::engine::family::{call_input_rels, call_owner_delta_rels};
use crate::spine::StringId;
use crate::storage::call::{CallFamilyWrite, CallStore};
use crate::storage::Storage;

/// Minimal Rust: `beta` calls `alpha`, `gamma` calls `beta` and `alpha`, so
/// both `call_site` and resolved `call_edge` are non-empty after extraction.
const RUST_SRC: &str = "\
fn alpha() {}
fn beta() {
    alpha();
}
fn gamma() {
    beta();
    alpha();
}
";

/// V1 with a retarget: gamma's second call becomes `beta()` instead of
/// `alpha()`. Line count is identical to `RUST_SRC`, so the owner-delta fast
/// path accepts it and produces a mixed delta for the affected families.
const V1_RETARGET: &str = "\
fn alpha() {}
fn beta() {
    alpha();
}
fn gamma() {
    beta();
    beta();
}
";

/// V1 plus an extra function `delta()` that calls `alpha()`. Adding it at the
/// end leaves the spans of `alpha`, `beta`, and `gamma` unchanged, so the new
/// rows are pure inserts for every affected family.
const V1_PLUS_DELTA: &str = "\
fn alpha() {}
fn beta() {
    alpha();
}
fn gamma() {
    beta();
    alpha();
}
fn delta() {
    alpha();
}
";

fn fresh_engine(root: &PathBuf) -> Engine {
    let mut engine = Engine::new(db::open(None).unwrap(), root.clone());
    engine.ensure_meta().unwrap();
    engine.declare_builtins().unwrap();
    engine
        .db
        .conn()
        .execute(
            "INSERT INTO _file (repo, path, rev, hash, mtime, size) \
             VALUES ('', 'lib.rs', 'WORK', '', 0, 0)",
            [],
        )
        .unwrap();
    engine
}

fn engine_at(dir: &PathBuf, hash: &str) -> Engine {
    let mut engine = Engine::new(db::open(None).unwrap(), dir.clone());
    engine.ensure_meta().unwrap();
    engine.declare_builtins().unwrap();
    engine
        .db
        .conn()
        .execute(
            "INSERT INTO _file (repo, path, rev, hash, mtime, size) \
             VALUES ('', 'lib.rs', 'WORK', ?1, 0, 0)",
            [hash],
        )
        .unwrap();
    engine
}

fn make_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sprf-render-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn site_rows(engine: &Engine) -> Vec<[i64; 5]> {
    let mut stmt = engine
        .db
        .conn()
        .prepare("SELECT repo, caller, callee, file, line FROM rel_call_site")
        .unwrap();
    let mut rows: Vec<[i64; 5]> = stmt
        .query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?]))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    rows.sort();
    rows
}

fn as_int(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        _ => panic!("expected Int value in memo row"),
    }
}

/// Assert that every public call rel holds exactly the rows currently memoized
/// in the engine's persistent router.
fn assert_all_call_rels_match_memo(engine: &Engine) {
    let router = engine.call_router.borrow();
    let router = router.as_ref().expect("router memo should be populated");

    let memo_as_sorted_i64 = |name: &str| {
        let mut rows: Vec<Vec<i64>> = router
            .rows(name)
            .unwrap_or(&[])
            .iter()
            .map(|r| r.iter().map(as_int).collect())
            .collect();
        rows.sort();
        rows
    };

    let rel_as_sorted_i64 = |sql: &str, ncols: usize| {
        let mut stmt = engine.db.conn().prepare(sql).unwrap();
        let mut rows: Vec<Vec<i64>> = stmt
            .query_map([], |row| {
                let mut v = Vec::with_capacity(ncols);
                for i in 0..ncols {
                    v.push(row.get(i)?);
                }
                Ok(v)
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        rows.sort();
        rows
    };

    let cases: &[(&str, &str, usize)] = &[
        ("call_site", "SELECT repo, caller, callee, file, line FROM rel_call_site", 5),
        ("call_edge", "SELECT caller, callee, kind FROM rel_call_edge", 3),
        ("call_kind", "SELECT fn, kind FROM rel_call_kind", 2),
        (
            "call_edge_rev",
            "SELECT caller, callee, kind, rev FROM rel_call_edge_rev",
            4,
        ),
        (
            "call_def",
            "SELECT repo, sym, kind, file, line, \"end\" FROM rel_call_def",
            6,
        ),
        (
            "call_def_rev",
            "SELECT repo, sym, kind, file, line, \"end\", rev FROM rel_call_def_rev",
            7,
        ),
        ("call_name", "SELECT sym, name FROM rel_call_name", 2),
    ];

    for (name, sql, ncols) in cases {
        assert_eq!(
            memo_as_sorted_i64(name),
            rel_as_sorted_i64(sql, *ncols),
            "public rel {name} diverged from router memo"
        );
    }
}

#[test]
fn render_cold_family_on_fresh_database() {
    let dir = make_dir("cold-fresh");
    fs::write(dir.join("lib.rs"), RUST_SRC).unwrap();

    let mut engine = fresh_engine(&dir);
    // Create the call-family schema without writing any owned rows and without
    // populating the in-process router memo. This is the "fresh DB" condition:
    // every family has no memo and an empty derivation.
    engine
        .db
        .persist_call_family(CallFamilyWrite {
            owners: &[],
            def_buckets: &[],
            defs: &[],
            scip_dependency_digest: [0; 32],
            module_dependency_digest: [0; 32],
        })
        .unwrap();

    let rerun = engine.flip_call_rels_via_router(&call_input_rels()).unwrap();
    assert_eq!(
        rerun,
        vec![
            "call_def",
            "call_def_rev",
            "call_edge",
            "call_edge_rev",
            "call_kind",
            "call_name",
            "call_site",
        ],
        "cold flip must derive every hosted family in registry order"
    );
    assert!(
        site_rows(&engine).is_empty(),
        "fresh DB cold flip must leave call_site empty"
    );
    assert_all_call_rels_match_memo(&engine);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn render_cold_reload_removes_stale_rows() {
    let dir = make_dir("cold-stale");
    fs::write(dir.join("lib.rs"), RUST_SRC).unwrap();

    let mut engine = fresh_engine(&dir);
    // Establish real baseline rows and the router memo.
    engine.refresh_call_rels().unwrap();
    // Simulate a fresh process (or memo loss): the in-process memo is gone but
    // the public rels still hold the prior render. A stale row inserted now is
    // exactly the hazard `reload_rel` exists to fix.
    engine.call_router.borrow_mut().take();

    let bogus_callee = StringId::of("bogus_stale_callee").sqlite();
    Storage::insert_rows(
        &engine.db,
        "rel_call_site",
        &["repo", "caller", "callee", "file", "line"],
        &[vec![
            Value::Int(0),
            Value::Int(0),
            Value::Int(bogus_callee),
            Value::Int(0),
            Value::Int(9999),
        ]],
    )
    .unwrap();
    assert!(
        site_rows(&engine).iter().any(|r| r[2] == bogus_callee),
        "fixture: bogus stale row must be seeded"
    );

    // Memo is None again, so every family cold-reloads via `reload_rel`,
    // deleting the bogus row rather than merging around it.
    engine.flip_call_rels_via_router(&call_input_rels()).unwrap();
    assert!(
        !site_rows(&engine).iter().any(|r| r[2] == bogus_callee),
        "cold reload must remove stale rows a prior process left behind"
    );
    assert_all_call_rels_match_memo(&engine);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn render_warm_family_empty_delta_issues_no_writes() {
    let dir = make_dir("warm-empty");
    fs::write(dir.join("lib.rs"), RUST_SRC).unwrap();

    let mut engine = fresh_engine(&dir);
    engine.refresh_call_rels().unwrap();
    let before = site_rows(&engine);
    let data_version_before: i64 = engine
        .db
        .conn()
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();

    // Re-pass the full input set: every family reruns, but because the owned
    // tables are unchanged the deltas are empty and no write call should land.
    let rerun = engine.flip_call_rels_via_router(&call_input_rels()).unwrap();
    assert_eq!(
        rerun,
        vec![
            "call_def",
            "call_def_rev",
            "call_edge",
            "call_edge_rev",
            "call_kind",
            "call_name",
            "call_site",
        ],
        "warm empty-delta flip must still report every family as rerun"
    );
    assert_eq!(site_rows(&engine), before, "rel_call_site must be untouched");
    let data_version_after: i64 = engine
        .db
        .conn()
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        data_version_before, data_version_after,
        "empty-delta warm flip must issue zero writes (data_version unchanged)"
    );
    assert_all_call_rels_match_memo(&engine);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn render_warm_family_insert_only_delta() {
    let dir = make_dir("warm-insert");
    fs::write(dir.join("lib.rs"), RUST_SRC).unwrap();

    let mut engine = engine_at(&dir, "v1");
    engine.refresh_call_rels().unwrap();
    let site_before = site_rows(&engine);

    // Add a new function and call site at the end; existing def spans are
    // unchanged, so every affected family sees a pure-insert delta.
    fs::write(dir.join("lib.rs"), V1_PLUS_DELTA).unwrap();
    engine
        .db
        .conn()
        .execute("UPDATE _file SET hash = 'v1-plus' WHERE path = 'lib.rs'", [])
        .unwrap();
    engine.refresh_call_rels().unwrap();

    let site_after = site_rows(&engine);
    assert!(
        site_after.len() > site_before.len(),
        "insert-only delta must add call_site rows"
    );
    assert_all_call_rels_match_memo(&engine);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn render_warm_family_retract_only_delta() {
    let dir = make_dir("warm-retract");
    fs::write(dir.join("lib.rs"), V1_PLUS_DELTA).unwrap();

    let mut engine = engine_at(&dir, "v1-plus");
    engine.refresh_call_rels().unwrap();
    let site_before = site_rows(&engine);
    assert!(
        !site_before.is_empty(),
        "fixture: v1-plus must produce call_site rows"
    );

    // Remove the extra function and its call site; every affected family sees
    // a pure-retract delta.
    fs::write(dir.join("lib.rs"), RUST_SRC).unwrap();
    engine
        .db
        .conn()
        .execute("UPDATE _file SET hash = 'v1' WHERE path = 'lib.rs'", [])
        .unwrap();
    engine.refresh_call_rels().unwrap();

    let site_after = site_rows(&engine);
    assert!(
        site_after.len() < site_before.len(),
        "retract-only delta must remove call_site rows"
    );
    assert_all_call_rels_match_memo(&engine);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn render_warm_family_mixed_delta() {
    let dir = make_dir("warm-mixed");
    fs::write(dir.join("lib.rs"), RUST_SRC).unwrap();

    let mut engine = engine_at(&dir, "v1");
    engine.refresh_call_rels().unwrap();
    let site_before = site_rows(&engine);

    // Retarget gamma's second call: one site/edge retracts, another inserts.
    fs::write(dir.join("lib.rs"), V1_RETARGET).unwrap();
    engine
        .db
        .conn()
        .execute("UPDATE _file SET hash = 'v2' WHERE path = 'lib.rs'", [])
        .unwrap();
    engine.refresh_call_rels().unwrap();

    let site_after = site_rows(&engine);
    assert_ne!(
        site_before, site_after,
        "mixed delta must change call_site"
    );
    assert_all_call_rels_match_memo(&engine);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn render_flip_inside_caller_owned_transaction() {
    let dir = make_dir("warm-tx");
    fs::write(dir.join("lib.rs"), RUST_SRC).unwrap();

    let mut engine = engine_at(&dir, "v1");
    engine.refresh_call_rels().unwrap();

    // Mutate _call_resolution so call_edge/call_edge_rev have real work to do
    // inside the caller-owned transaction.
    engine
        .db
        .conn()
        .execute("DELETE FROM _call_resolution WHERE site_id IN (SELECT site_id FROM _call_resolution LIMIT 1)", [])
        .unwrap();

    engine.db.conn().execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(!engine.db.is_autocommit(), "fixture: caller must own a transaction");
    let edge_before = {
        let mut stmt = engine.db.conn().prepare("SELECT caller, callee, kind FROM rel_call_edge").unwrap();
        let mut rows: Vec<[i64; 3]> = stmt.query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        rows.sort();
        rows
    };

    let mut changed = HashSet::new();
    changed.insert("_call_resolution");
    let rerun = engine.flip_call_rels_via_router(&changed).unwrap();
    assert_eq!(
        rerun,
        vec!["call_edge", "call_edge_rev"],
        "resolution-only change reruns the two resolution readers"
    );
    assert!(
        !engine.db.is_autocommit(),
        "flip must not issue a nested COMMIT; outer transaction still owns it"
    );

    engine.db.conn().execute_batch("COMMIT").unwrap();
    assert!(engine.db.is_autocommit(), "outer transaction must commit cleanly");

    let edge_after = {
        let mut stmt = engine.db.conn().prepare("SELECT caller, callee, kind FROM rel_call_edge").unwrap();
        let mut rows: Vec<[i64; 3]> = stmt.query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        rows.sort();
        rows
    };
    assert_ne!(
        edge_before, edge_after,
        "render writes made inside the caller transaction must commit"
    );
    assert_all_call_rels_match_memo(&engine);

    let _ = fs::remove_dir_all(&dir);
}
