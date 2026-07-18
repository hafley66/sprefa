//! Cold-start staging (plan `2026-07-17-cold-start-staging.md`): on a blank
//! slate under the daemon poll loop, the first tick seeds one `_cold_node` per
//! used extract family and returns WITHOUT running the extract fan-out or the
//! derived rebuild. Each node then runs its wholesale family refresh; the
//! completion tick does the single blank-slate derived rebuild. These tests
//! drive the engine in-process (poll_loop toggled) — the daemon shell just wires
//! the same `Engine::run_cold_node` / `cold_nodes_complete` calls onto the queue.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cold_stage_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

/// A tiny Rust corpus: a call chain (top -> make -> helper) plus a struct, so
/// the module/type/call families all extract non-empty rows.
fn write_corpus(dir: &Path) {
    fs::write(
        dir.join("src/lib.rs"),
        r#"
pub struct Widget { pub id: u32 }
pub fn helper() -> u32 { 7 }
pub fn make() -> Widget { Widget { id: helper() } }
pub fn top() -> Widget { make() }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("src/other.rs"),
        r#"
pub fn side() -> u32 { 3 }
pub fn caller() -> u32 { side() }
"#,
    )
    .unwrap();
}

// Uses call_edge (via closure) and type_edge (via closure): the module (needed
// for resolution), type, and call families are all `used`, plus the two derived
// closure rels exercise the completion tick's derived rebuild.
const PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /./, line).
rel calls_reach(a: text, b: text).
calls_reach(a, b) <- closure(call_edge).
rel type_reach(a: text, b: text).
type_reach(a, b) <- closure(type_edge).
"#;

fn parse_prog() -> sprefa_v5::ast::Program {
    parse::parse(lex::lex(PROG).unwrap()).unwrap()
}

/// The extraction + derived rels compared for staged-vs-inline equivalence.
const COMPARE_RELS: &[&str] = &[
    "module_edge_rev",
    "call_edge",
    "call_edge_rev",
    "call_site",
    "call_def",
    "call_name",
    "type_edge",
    "type_entity",
    "calls_reach",
    "type_reach",
];

/// Drain every seeded cold node in canonical priority order (module before
/// type/call), the same order the single-flight daemon worker claims them.
fn drain_all(eng: &mut Engine, prog: &sprefa_v5::ast::Program, mut staged: Vec<(String, u32, i64)>) {
    staged.sort_by(|a, b| b.2.cmp(&a.2));
    for (family, shard, _) in &staged {
        eng.run_cold_node(prog, family, *shard).unwrap();
    }
}

/// Blank-slate boot under staging SEEDS nodes and does NOT extract inline nor
/// rebuild derived on the first tick.
#[test]
fn blank_slate_seeds_without_inline_extract_or_derived() {
    let d = sandbox("seed");
    write_corpus(&d);
    let prog = parse_prog();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.poll_loop = true;

    let report = eng.tick_report(&prog, true).unwrap();
    assert!(report.cold_pending, "seed tick reports cold_pending");
    assert!(!report.cold_staged.is_empty(), "seed tick handed back cold nodes");
    assert!(!report.is_settled(), "a cold-pending tick is never settled");

    // Extract families did NOT run inline:
    assert_eq!(eng.count_rows("call_edge").unwrap(), 0, "call_edge not extracted on the seed tick");
    assert_eq!(eng.count_rows("type_edge").unwrap(), 0, "type_edge not extracted on the seed tick");
    assert_eq!(eng.count_rows("module_edge_rev").unwrap(), 0, "module not extracted on the seed tick");
    // Derived layer NOT rebuilt:
    assert_eq!(eng.count_rows("calls_reach").unwrap(), 0, "derived not rebuilt on the seed tick");

    // Every seeded node is pending, one shard each (wholesale this arc).
    let rows = eng.query_sql("SELECT state, n_shards FROM _cold_node", &[]).unwrap();
    assert!(!rows.is_empty(), "nodes seeded");
    assert!(rows.iter().all(|r| r[0] == json!("pending")), "all nodes pending: {rows:?}");
    assert!(rows.iter().all(|r| r[1] == json!(1)), "wholesale => n_shards=1: {rows:?}");
    // module/type/call are among the seeded families.
    let staged: Vec<String> = report.cold_staged.iter().map(|j| j.0.clone()).collect();
    for want in ["module-rels", "type-rels", "call-rels"] {
        assert!(staged.contains(&want.to_string()), "expected {want} seeded, got {staged:?}");
    }
}

/// Draining all nodes then the completion tick produces a db row-count-equivalent
/// (per rel) to the inline cold path on the same corpus — the discriminating test.
#[test]
fn staged_drain_then_completion_matches_inline() {
    let prog = parse_prog();

    // Inline cold path (one-shot, poll_loop=false).
    let di = sandbox("equiv_inline");
    write_corpus(&di);
    let conn_i = db::open(Some(di.join("db").to_str().unwrap())).unwrap();
    let mut eng_inline = Engine::new(conn_i, di.clone());
    eng_inline.tick(&prog, true).unwrap();

    // Staged path (daemon, poll_loop=true): seed -> drain nodes -> completion tick.
    let ds = sandbox("equiv_staged");
    write_corpus(&ds);
    let conn_s = db::open(Some(ds.join("db").to_str().unwrap())).unwrap();
    let mut eng_staged = Engine::new(conn_s, ds.clone());
    eng_staged.poll_loop = true;

    let report = eng_staged.tick_report(&prog, true).unwrap();
    assert!(report.cold_pending, "staged seed tick is cold-pending");
    drain_all(&mut eng_staged, &prog, report.cold_staged.clone());
    assert!(eng_staged.cold_nodes_complete().unwrap(), "all nodes done after drain");
    // Completion tick: cold no longer in progress -> normal blank-slate rebuild.
    eng_staged.tick(&prog, true).unwrap();

    // Sanity: the staged path actually extracted + derived something.
    assert!(eng_staged.count_rows("call_edge").unwrap() > 0, "staged call graph is non-empty");
    assert!(eng_staged.count_rows("calls_reach").unwrap() > 0, "staged derived closure non-empty");

    for rel in COMPARE_RELS {
        let inline = eng_inline.count_rows(rel).unwrap();
        let staged = eng_staged.count_rows(rel).unwrap();
        assert_eq!(inline, staged, "rel `{rel}`: inline={inline} staged={staged}");
    }
}

/// Crash recovery: mark a subset done, boot (reopen the db), only the still-
/// pending nodes re-run — and the final warmed db still matches inline.
#[test]
fn crash_recovery_reruns_only_pending_nodes() {
    let prog = parse_prog();

    // Inline reference db for the final equivalence check.
    let di = sandbox("crash_inline");
    write_corpus(&di);
    let conn_i = db::open(Some(di.join("db").to_str().unwrap())).unwrap();
    let mut eng_inline = Engine::new(conn_i, di.clone());
    eng_inline.tick(&prog, true).unwrap();

    let ds = sandbox("crash_staged");
    write_corpus(&ds);
    let db_path = ds.join("db");

    // Boot 1: seed, then drain ONLY the highest-priority node (module), then
    // "crash" (drop the engine) with the rest still pending.
    let first_family;
    {
        let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
        let mut eng = Engine::new(conn, ds.clone());
        eng.poll_loop = true;
        let report = eng.tick_report(&prog, true).unwrap();
        let mut staged = report.cold_staged.clone();
        staged.sort_by(|a, b| b.2.cmp(&a.2));
        assert!(staged.len() >= 2, "need >=2 nodes to test partial drain: {staged:?}");
        first_family = staged[0].0.clone();
        eng.run_cold_node(&prog, &first_family, staged[0].1).unwrap();
        // engine dropped here — db (with the done node) persists on disk.
    }

    // Boot 2: reopen the same db. The tick sees cold-start in progress and
    // re-enqueues ONLY the still-pending nodes; the already-done family is not
    // in the re-enqueued set (not re-run).
    let conn2 = db::open(Some(db_path.to_str().unwrap())).unwrap();
    let mut eng2 = Engine::new(conn2, ds.clone());
    eng2.poll_loop = true;
    let report2 = eng2.tick_report(&prog, true).unwrap();
    assert!(report2.cold_pending, "resume tick is cold-pending");
    let reenqueued: Vec<String> = report2.cold_staged.iter().map(|j| j.0.clone()).collect();
    assert!(
        !reenqueued.contains(&first_family),
        "already-done `{first_family}` must not be re-run; re-enqueued={reenqueued:?}"
    );
    // The done node stays done in the table.
    let done_rows = eng2
        .query_sql("SELECT family FROM _cold_node WHERE state='done'", &[])
        .unwrap();
    assert_eq!(done_rows.len(), 1, "exactly the one drained node is done");
    assert_eq!(done_rows[0][0], json!(first_family));

    // Finish the remaining nodes + completion tick, then confirm equivalence.
    drain_all(&mut eng2, &prog, report2.cold_staged.clone());
    assert!(eng2.cold_nodes_complete().unwrap(), "all nodes done after resume drain");
    eng2.tick(&prog, true).unwrap();
    for rel in COMPARE_RELS {
        let inline = eng_inline.count_rows(rel).unwrap();
        let staged = eng2.count_rows(rel).unwrap();
        assert_eq!(inline, staged, "post-recovery rel `{rel}`: inline={inline} staged={staged}");
    }
}

/// A `--no-daemon` (one-shot) cold run is unchanged: the inline path runs, no
/// staging, and `_cold_node` is never even created.
#[test]
fn no_daemon_cold_run_stays_inline() {
    let d = sandbox("inline");
    write_corpus(&d);
    let prog = parse_prog();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone()); // poll_loop defaults to false

    let report = eng.tick_report(&prog, true).unwrap();
    assert!(!report.cold_pending, "one-shot tick never stages");
    assert!(report.cold_staged.is_empty(), "one-shot tick seeds nothing");

    // Extraction + derive happened inline in this one tick.
    assert!(eng.count_rows("call_edge").unwrap() > 0, "inline extracted the call graph");
    assert!(eng.count_rows("calls_reach").unwrap() > 0, "inline built the derived closure");

    // The staging table was never touched on the one-shot path.
    assert!(
        eng.query_sql("SELECT COUNT(*) FROM _cold_node", &[]).is_err(),
        "_cold_node must not exist after a one-shot run"
    );
}
