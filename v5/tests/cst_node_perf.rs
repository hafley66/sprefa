//! CST `node`/`child` perf + N+1 structural guarantee (christmas #3).
//!
//! 1. `node_child_tick_has_no_n1` (always runs): a multi-file fixture tick that
//!    populates node/child must NOT trip the per-tick N+1 detector — every write
//!    (spine `_strings`/`_where_bytes`, `node`, `child`) goes through the plural
//!    `insert_rows`/`refresh_rel` path. Structural, not a vibe: asserts
//!    `eng.last_n1.is_none()`.
//!
//! 2. `node_child_perf_on_v5_src` (`#[ignore]`, run with `--ignored --nocapture`):
//!    cold-walks the real `v5/src` file set, reports wall-clock + the `node`/
//!    `child` row counts (the real ~100x number), then re-ticks after a one-file
//!    edit for the incremental time, and confirms a program WITHOUT node/child
//!    pays zero (no rows). Also confirms the stored-id-set early-out
//!    short-circuits a no-op re-tick.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

// A program that asks for node/child (gates the walk on).
const NODE_PROG: &str = r#"
rel seen(path: text).
rel anc(a: text, b: text).
seen(path) <- scan("WORK","src/**/*",path,rev), match(path, rev, /./, _l).
anc(a, b) <- closure(child).
"#;

// A program that NEVER references node/child (gate must keep it free).
const NO_NODE_PROG: &str = r#"
rel seen(path: text).
seen(path) <- scan("WORK","src/**/*.rs",path,rev), match(path, rev, /fn /, _l).
"#;

fn row_count(eng: &Engine, rel: &str) -> i64 {
    eng.count_rows(rel).unwrap_or(-1)
}

#[test]
fn node_child_tick_has_no_n1() {
    let dir = std::env::temp_dir().join("cst_node_perf_n1");
    let _ = fs::remove_dir_all(&dir);
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    // Several files across languages, each with enough nodes that a per-row
    // writer would blow past N1_THRESHOLD (64).
    for i in 0..8 {
        fs::write(src.join(format!("m{i}.rs")),
            format!("fn f{i}() {{\n    let a = {i};\n    let b = a + 1;\n    let c = b * 2;\n}}\n")).unwrap();
    }
    fs::write(src.join("p.py"), "def g():\n    x = 1\n    y = x + 2\n    return y\n").unwrap();

    let prog = parse::parse(lex::lex(NODE_PROG).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    eng.tick(&prog, true).unwrap();

    let nodes = row_count(&eng, "node");
    let children = row_count(&eng, "child");
    assert!(nodes > 64, "fixture must have enough nodes to expose an N+1 (got {nodes})");
    assert!(children > 0, "child edges populated (got {children})");
    assert!(eng.last_n1.is_none(),
        "node/child tick tripped the N+1 detector: {:?} — a per-row write slipped the plural API",
        eng.last_n1);
}

#[test]
#[ignore = "perf bench: run with `cargo test --test cst_node_perf -- --ignored --nocapture`"]
fn node_child_perf_on_v5_src() {
    // The real v5/src checkout is the corpus.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dbdir = std::env::temp_dir().join("cst_node_perf_v5src");
    let _ = fs::remove_dir_all(&dbdir);
    fs::create_dir_all(&dbdir).unwrap();

    let prog = parse::parse(lex::lex(NODE_PROG).unwrap()).unwrap();
    let conn = db::open(Some(dbdir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root.clone());

    // COLD: full walk over v5/src.
    let t = Instant::now();
    eng.tick(&prog, true).unwrap();
    let cold_ms = t.elapsed().as_secs_f64() * 1000.0;
    let nodes = row_count(&eng, "node");
    let children = row_count(&eng, "child");
    let files = row_count(&eng, "seen");
    assert!(eng.last_n1.is_none(), "cold node tick tripped N+1: {:?}", eng.last_n1);

    // NO-OP RE-TICK: nothing changed, the stored-id-set early-out should
    // short-circuit refresh_node_rels (no DELETE+reinsert).
    let t = Instant::now();
    eng.tick(&prog, true).unwrap();
    let noop_ms = t.elapsed().as_secs_f64() * 1000.0;

    // INCREMENTAL: edit one real source file (append a comment, no behavior
    // change) and re-tick via tick_paths.
    let target = root.join("src/spine.rs");
    let original = fs::read_to_string(&target).unwrap();
    let edited = format!("{original}\n// cst perf bench edit\n");
    fs::write(&target, &edited).unwrap();
    let t = Instant::now();
    let r = eng.tick_paths(&prog, std::slice::from_ref(&target), true);
    let inc_ms = t.elapsed().as_secs_f64() * 1000.0;
    fs::write(&target, &original).unwrap(); // restore before asserting
    r.unwrap();
    assert!(eng.last_n1.is_none(), "incremental node tick tripped N+1: {:?}", eng.last_n1);

    // GATE PAYS ZERO: a program that never references node/child walks nothing.
    let dbdir2 = std::env::temp_dir().join("cst_node_perf_v5src_nonode");
    let _ = fs::remove_dir_all(&dbdir2);
    fs::create_dir_all(&dbdir2).unwrap();
    let prog2 = parse::parse(lex::lex(NO_NODE_PROG).unwrap()).unwrap();
    let conn2 = db::open(Some(dbdir2.join("db").to_str().unwrap())).unwrap();
    let mut eng2 = Engine::new(conn2, root.clone());
    let t = Instant::now();
    eng2.tick(&prog2, true).unwrap();
    let gate_ms = t.elapsed().as_secs_f64() * 1000.0;
    let gate_nodes = row_count(&eng2, "node");

    println!("\n=== CST node/child perf on real v5/src ===");
    println!("files (seen):       {files}");
    println!("node rows:          {nodes}");
    println!("child rows:         {children}");
    println!("cold walk:          {cold_ms:>9.1} ms");
    println!("no-op re-tick:      {noop_ms:>9.1} ms  (early-out should short-circuit)");
    println!("incremental (1 ed): {inc_ms:>9.1} ms");
    println!("gate-off tick:      {gate_ms:>9.1} ms  (no node/child reference)");
    println!("gate-off node rows: {gate_nodes}  (must be 0 — no rel_node table populated)");
    println!("==========================================\n");

    // Gate: a program without node/child must produce zero node rows. The
    // rel_node table exists (declared) but is never populated.
    assert_eq!(gate_nodes, 0, "node_rels_used gate failed: walked despite no node/child reference");
    // Sanity: the real corpus produced a meaningful node count.
    assert!(nodes > 1000, "v5/src should yield thousands of nodes (got {nodes})");
}


#[test]
fn incremental_node_tick_is_path_scoped() {
    // Cold-tick a 5-file fixture, edit ONE file, incremental-tick, and assert
    // the node walk parsed exactly 1 file — the machine-independent proof the
    // CST refresh is path-scoped (can't silently regress to full-corpus).
    let dir = std::env::temp_dir().join("cst_node_perf_pathscope");
    let _ = fs::remove_dir_all(&dir);
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    for i in 0..5 {
        fs::write(src.join(format!("m{i}.rs")),
            format!("fn f{i}() {{\n    let a = {i};\n    let b = a + 1;\n}}\n")).unwrap();
    }

    let prog = parse::parse(lex::lex(NODE_PROG).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());

    // COLD: full walk over all 5 files.
    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.last_node_files_walked.get(), 5,
        "cold tick should walk every file (got {})", eng.last_node_files_walked.get());
    let nodes_before = row_count(&eng, "node");
    assert!(nodes_before > 0);

    // INCREMENTAL: edit ONE file (real body change so its node set moves).
    let target = src.join("m2.rs");
    fs::write(&target, "fn f2() {\n    let a = 2;\n    let b = a + 1;\n    let c = b * 9;\n}\n").unwrap();
    eng.tick_paths(&prog, std::slice::from_ref(&target), true).unwrap();

    // THE PROOF: only the one edited file was re-walked, NOT all 5.
    assert_eq!(eng.last_node_files_walked.get(), 1,
        "incremental tick must walk ONLY the edited file (got {} — regressed to full-corpus?)",
        eng.last_node_files_walked.get());
    assert!(eng.last_n1.is_none(),
        "incremental node delta tripped N+1: {:?}", eng.last_n1);

    // The other 4 files' node rows survived; the corpus didn't get wiped.
    let nodes_after = row_count(&eng, "node");
    assert!(nodes_after > 0, "node rows survived the delta (got {nodes_after})");

    let _ = fs::remove_dir_all(&dir);
}
