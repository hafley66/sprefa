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

/// A many-small-files corpus: `n` files each with a param + let binding so the
/// dataflow family emits df_node rows, plus a call chain, so module/type/call/
/// dataflow/scip all seed. With `n > COLD_CHUNK_MAX_FILES` (64) the dataflow
/// family splits into multiple byte/file-bounded chunks under the DEFAULT config
/// (no process-global env override — the file-count cap does the splitting).
fn write_many_files(dir: &Path, n: usize) {
    for idx in 0..n {
        let prev = if idx == 0 { "0".to_string() } else { format!("f{}(idx)", idx - 1) };
        fs::write(
            dir.join(format!("src/f{idx}.rs")),
            format!(
                "pub fn f{idx}(input: u32) -> u32 {{ let local = input; let seed = {prev}; local + seed }}\n"
            ),
        )
        .unwrap();
    }
}

/// A program exercising the chunked dataflow family alongside the wholesale
/// call/type families (module/type/call/dataflow/scip all seed).
const PROG_DF: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /pub fn /, line).
rel calls_reach(caller: text, callee: text).
calls_reach(caller, callee) <- closure(call_edge).
rel type_reach(sub: text, sup: text).
type_reach(sub, sup) <- closure(type_edge).
rel df_touch(node: text).
df_touch(node) <- df_node(node, _kind, _var, _fn, _file, _line).
"#;

const COMPARE_RELS_DF: &[&str] = &[
    "call_edge", "call_edge_rev", "call_site", "type_edge", "type_entity",
    "df_node", "df_node_repo", "df_edge", "df_param", "df_node_rev",
    "calls_reach", "type_reach", "df_touch",
];

fn parse_df() -> sprefa_v5::ast::Program {
    parse::parse(lex::lex(PROG_DF).unwrap()).unwrap()
}

/// Chunked drain: with >64 files the dataflow family splits into multiple
/// chunks (n_shards > 1); draining every chunk + the completion tick produces a
/// db row-count-equivalent (per rel) to the inline cold path. The discriminating
/// test for MB-bounded chunking equivalence (plan Addendum 2026-07-18).
#[test]
fn chunked_dataflow_drain_matches_inline() {
    let prog = parse_df();
    let n_files = 150; // > COLD_CHUNK_MAX_FILES (64) → >= 3 dataflow chunks

    // Inline cold path.
    let di = sandbox("chunk_inline");
    write_many_files(&di, n_files);
    let conn_i = db::open(Some(di.join("db").to_str().unwrap())).unwrap();
    let mut eng_inline = Engine::new(conn_i, di.clone());
    eng_inline.tick(&prog, true).unwrap();

    // Staged + chunked path.
    let ds = sandbox("chunk_staged");
    write_many_files(&ds, n_files);
    let conn_s = db::open(Some(ds.join("db").to_str().unwrap())).unwrap();
    let mut eng_staged = Engine::new(conn_s, ds.clone());
    eng_staged.poll_loop = true;

    let report = eng_staged.tick_report(&prog, true).unwrap();
    assert!(report.cold_pending, "chunk seed tick is cold-pending");

    // Dataflow split into multiple chunks; call/type stayed wholesale (1 shard).
    let df_shards = eng_staged
        .query_sql("SELECT DISTINCT n_shards FROM _cold_node WHERE family='dataflow-rels'", &[])
        .unwrap();
    assert_eq!(df_shards.len(), 1, "one n_shards value for dataflow");
    let n_df_shards = df_shards[0][0].as_i64().unwrap();
    assert!(n_df_shards >= 3, "dataflow chunked into >=3 slices, got {n_df_shards}");
    let call_shards = eng_staged
        .query_sql("SELECT n_shards FROM _cold_node WHERE family='call-rels'", &[])
        .unwrap();
    assert_eq!(call_shards[0][0], json!(1), "call stays wholesale (1 shard)");

    drain_all(&mut eng_staged, &prog, report.cold_staged.clone());
    assert!(eng_staged.cold_nodes_complete().unwrap(), "all nodes done after chunked drain");
    eng_staged.tick(&prog, true).unwrap();

    assert!(eng_staged.count_rows("df_node").unwrap() > 0, "chunked dataflow non-empty");
    for rel in COMPARE_RELS_DF {
        let inline = eng_inline.count_rows(rel).unwrap();
        let staged = eng_staged.count_rows(rel).unwrap();
        assert_eq!(inline, staged, "rel `{rel}`: inline={inline} staged={staged}");
    }
}

/// Crash mid-chunk: drain the wholesale families + a SUBSET of the dataflow
/// chunks, "crash" (drop engine), reopen. The resume re-enqueues ONLY the
/// still-pending dataflow chunks (drained chunks stay done, not re-run), and the
/// finished db still matches inline.
#[test]
fn crash_mid_chunk_reruns_only_pending_chunks() {
    let prog = parse_df();
    let n_files = 150;

    let di = sandbox("midchunk_inline");
    write_many_files(&di, n_files);
    let conn_i = db::open(Some(di.join("db").to_str().unwrap())).unwrap();
    let mut eng_inline = Engine::new(conn_i, di.clone());
    eng_inline.tick(&prog, true).unwrap();

    let ds = sandbox("midchunk_staged");
    write_many_files(&ds, n_files);
    let db_path = ds.join("db");

    // Boot 1: seed, drain everything EXCEPT the last dataflow chunk, then crash.
    let last_df_shard;
    {
        let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
        let mut eng = Engine::new(conn, ds.clone());
        eng.poll_loop = true;
        let report = eng.tick_report(&prog, true).unwrap();
        let mut staged = report.cold_staged.clone();
        staged.sort_by(|a, b| b.2.cmp(&a.2));
        // The highest dataflow shard index is drained LAST → leave it pending.
        last_df_shard = staged
            .iter()
            .filter(|(fam, _, _)| fam == "dataflow-rels")
            .map(|(_, shard, _)| *shard)
            .max()
            .expect("dataflow chunks seeded");
        for (family, shard, _) in &staged {
            if family == "dataflow-rels" && *shard == last_df_shard {
                continue; // leave one chunk pending
            }
            eng.run_cold_node(&prog, family, *shard).unwrap();
        }
        assert!(!eng.cold_nodes_complete().unwrap(), "not complete: one chunk left");
        // engine dropped → db persists with the drained chunks done.
    }

    // Boot 2: reopen. Resume re-enqueues ONLY the pending dataflow chunk(s).
    let conn2 = db::open(Some(db_path.to_str().unwrap())).unwrap();
    let mut eng2 = Engine::new(conn2, ds.clone());
    eng2.poll_loop = true;
    let report2 = eng2.tick_report(&prog, true).unwrap();
    assert!(report2.cold_pending, "resume tick is cold-pending");
    let reenqueued: Vec<(String, u32)> =
        report2.cold_staged.iter().map(|j| (j.0.clone(), j.1)).collect();
    assert_eq!(
        reenqueued,
        vec![("dataflow-rels".to_string(), last_df_shard)],
        "resume re-enqueues exactly the one pending dataflow chunk; got {reenqueued:?}"
    );
    // Every other node stayed done.
    let done = eng2
        .query_sql("SELECT COUNT(*) FROM _cold_node WHERE state='done'", &[])
        .unwrap();
    let pending = eng2
        .query_sql("SELECT COUNT(*) FROM _cold_node WHERE state != 'done'", &[])
        .unwrap();
    assert_eq!(pending[0][0], json!(1), "exactly one chunk pending on resume");
    assert!(done[0][0].as_i64().unwrap() >= 1, "drained chunks stayed done");

    drain_all(&mut eng2, &prog, report2.cold_staged.clone());
    assert!(eng2.cold_nodes_complete().unwrap(), "complete after resume drain");
    eng2.tick(&prog, true).unwrap();
    for rel in COMPARE_RELS_DF {
        let inline = eng_inline.count_rows(rel).unwrap();
        let staged = eng2.count_rows(rel).unwrap();
        assert_eq!(inline, staged, "post-recovery rel `{rel}`: inline={inline} staged={staged}");
    }
}

/// Wall-time receipt (not a CI gate — run with `--ignored`): stage a cold boot
/// over sprefa's OWN `src/**/*.rs` corpus and print the per-node wall time, so
/// the longest single ColdExtract job is visible before/after chunking. Point
/// `DL_MEAS_ROOT` at a repo root; defaults to the current worktree.
#[test]
#[ignore]
fn measure_longest_cold_node() {
    let root = std::env::var("DL_MEAS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap());
    let prog = parse_df();
    let dbdir = sandbox("measure");
    let conn = db::open(Some(dbdir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root.clone());
    eng.poll_loop = true;
    let report = eng.tick_report(&prog, true).unwrap();
    let mut staged = report.cold_staged.clone();
    staged.sort_by(|a, b| b.2.cmp(&a.2));
    println!("[receipt] corpus root={}  nodes={}", root.display(), staged.len());
    let mut longest = (String::new(), 0u128);
    let mut per_family: std::collections::HashMap<String, u128> = std::collections::HashMap::new();
    for (family, shard, _) in &staged {
        let t = std::time::Instant::now();
        eng.run_cold_node(&prog, family, *shard).unwrap();
        let ms = t.elapsed().as_millis();
        *per_family.entry(family.clone()).or_default() += ms;
        if ms > longest.1 {
            longest = (format!("{family}/{shard}"), ms);
        }
    }
    let mut fam: Vec<_> = per_family.into_iter().collect();
    fam.sort_by(|a, b| b.1.cmp(&a.1));
    for (family, ms) in &fam {
        println!("[receipt] family {family}: {ms}ms total across its nodes");
    }
    println!("[receipt] LONGEST single node: {} = {}ms", longest.0, longest.1);
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
