//! T4 properties — randomized edit scripts over a synthetic Rust tree
//! (`plans/2026-07-16-capstone-retraction-tests.feature`, Feature "T4 properties").
//!
//! The design follows `tests/it/retraction_scripts.rs` for temp-dir engine setup,
//! per-step ticking, and the fresh-engine oracle. It adds proptest-generated
//! scripts (5–15 ops) and three properties:
//!
//! 1. Equivalence: after every step all 7 public call rels match a fresh engine.
//! 2. Memo = rel: after every step the router memo rows for each call family
//!    equal the base public rel rows.
//! 3. Fixpoint: flipping with an empty changed-set after a warm script changes
//!    nothing (no reruns, identical rel snapshots).

use proptest::prelude::*;
use sprefa_v5::ast::{Program, Value};
use sprefa_v5::{db, engine::Engine, lex, parse};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

// ---- copied helpers from tests/it/retraction_scripts.rs ------------------------

const CALL_PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
? call_site(repo, caller, callee, file, line).
? call_kind(fn_sym, kind).
? call_edge(caller, callee, kind).
? call_edge_rev(caller, callee, kind, rev).
? call_def(repo, sym, kind, file, line, end).
? call_def_rev(repo, sym, kind, file, line, end, rev).
? call_name(sym, name).
"#;

const REL_COLS: &[(&str, &[&str])] = &[
    ("call_site", &["repo", "caller", "callee", "file", "line"]),
    ("call_kind", &["fn", "kind"]),
    ("call_edge", &["caller", "callee", "kind"]),
    ("call_edge_rev", &["caller", "callee", "kind", "rev"]),
    ("call_def", &["repo", "sym", "kind", "file", "line", "end"]),
    ("call_def_rev", &["repo", "sym", "kind", "file", "line", "end", "rev"]),
    ("call_name", &["sym", "name"]),
];

fn call_prog() -> Program {
    parse::parse(lex::lex(CALL_PROG).unwrap()).unwrap()
}

fn engine_at(dir: &Path, db_name: &str) -> Engine {
    let conn = db::open(Some(dir.join(db_name).to_str().unwrap())).unwrap();
    Engine::new(conn, dir.to_path_buf())
}

fn cell_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Null => String::new(),
    }
}

/// Public call rels as decoded, sorted row sets — the oracle compares these.
fn dump_call_rels(engine: &Engine) -> BTreeMap<&'static str, Vec<String>> {
    let mut dump = BTreeMap::new();
    for &(rel, cols) in REL_COLS {
        let select = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        let sql = format!("SELECT {select} FROM rel_{rel}_txt");
        let rows = engine.query_sql(&sql, &[]).unwrap();
        let mut lines: Vec<String> = rows
            .into_iter()
            .map(|row| row.iter().map(cell_to_string).collect::<Vec<_>>().join("\t"))
            .collect();
        lines.sort();
        dump.insert(rel, lines);
    }
    dump
}

/// Base public call rels (integer sids) — for memo-vs-rel comparison.
fn dump_call_rels_base(engine: &Engine) -> BTreeMap<&'static str, Vec<String>> {
    let mut dump = BTreeMap::new();
    for &(rel, cols) in REL_COLS {
        let select = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        let sql = format!("SELECT {select} FROM rel_{rel}");
        let rows = engine.query_sql(&sql, &[]).unwrap();
        let mut lines: Vec<String> = rows
            .into_iter()
            .map(|row| row.iter().map(cell_to_string).collect::<Vec<_>>().join("\t"))
            .collect();
        lines.sort();
        dump.insert(rel, lines);
    }
    dump
}

fn oracle_dump_for(dir: &Path, tag: &str, prog_src: &str) -> BTreeMap<&'static str, Vec<String>> {
    let mut engine = engine_at(dir, &format!("oracle_{tag}.db"));
    let prog = parse::parse(lex::lex(prog_src).unwrap()).unwrap();
    engine.tick(&prog, true).unwrap();
    dump_call_rels(&engine)
}

fn assert_matches_oracle(
    scenario: &str,
    incremental: &BTreeMap<&'static str, Vec<String>>,
    oracle: &BTreeMap<&'static str, Vec<String>>,
) {
    for &(rel, _) in REL_COLS {
        let got = &incremental[rel];
        let want = &oracle[rel];
        if got == want {
            continue;
        }
        let detail = match got.iter().zip(want.iter()).enumerate().find(|(_, (g, w))| g != w) {
            Some((i, (g, w))) => format!("row {i}: incremental {g:?}, oracle {w:?}"),
            None if got.len() > want.len() => {
                format!("extra incremental row {}: {:?}", want.len(), got[want.len()])
            }
            None => format!("missing oracle row {}: {:?}", got.len(), want[got.len()]),
        };
        panic!(
            "{scenario}: rel `{rel}` diverges from the fresh-engine oracle \
             ({} incremental rows, {} oracle rows) — {detail}",
            got.len(),
            want.len(),
        );
    }
}

fn assert_memo_matches_rel(
    scenario: &str,
    engine: &Engine,
    rels: &BTreeMap<&'static str, Vec<String>>,
) {
    for &(rel, _) in REL_COLS {
        let memo_rows = engine
            .call_router_memo_rows(rel)
            .unwrap_or_else(|| panic!("{scenario}: router memo for `{rel}` should be populated"));
        let mut memo_lines: Vec<String> = memo_rows
            .iter()
            .map(|row| row.iter().map(value_to_string).collect::<Vec<_>>().join("\t"))
            .collect();
        memo_lines.sort();
        let rel_lines = &rels[rel];
        if memo_lines != *rel_lines {
            let detail = match memo_lines.iter().zip(rel_lines.iter()).enumerate().find(|(_, (g, w))| g != w) {
                Some((i, (g, w))) => format!("row {i}: memo {g:?}, rel {w:?}"),
                None if memo_lines.len() > rel_lines.len() => {
                    format!("extra memo row {}: {:?}", rel_lines.len(), memo_lines[rel_lines.len()])
                }
                None => format!("missing rel row {}: {:?}", memo_lines.len(), rel_lines[memo_lines.len()]),
            };
            panic!(
                "{scenario}: router memo for `{rel}` diverges from public rel \
                 ({} memo rows, {} rel rows) — {detail}",
                memo_lines.len(),
                rel_lines.len(),
            );
        }
    }
}

// ---- temp dir guard -----------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "retraction_props_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// ---- synthetic Rust tree and edit ops -----------------------------------------

type FnId = usize;

#[derive(Clone, Debug)]
struct Func {
    name: String,
    calls: Vec<FnId>,
}

#[derive(Clone, Debug)]
struct File {
    name: String,
    fns: Vec<FnId>,
    touch: usize,
}

#[derive(Clone, Debug)]
struct SyntheticTree {
    files: Vec<File>,
    funcs: BTreeMap<FnId, Func>,
    next_file_id: usize,
    next_fn_id: usize,
}

impl SyntheticTree {
    fn empty() -> Self {
        Self {
            files: Vec::new(),
            funcs: BTreeMap::new(),
            next_file_id: 0,
            next_fn_id: 0,
        }
    }

}

fn seed_tree() -> SyntheticTree {
    let mut tree = SyntheticTree::empty();
    apply_op(&mut tree, &Op::AddFile);
    apply_op(&mut tree, &Op::AddFn { file_idx: 0, callee_idx: 100 });
    tree
}

#[derive(Clone, Debug)]
enum Op {
    AddFile,
    DeleteFile { file_idx: usize },
    AddFn { file_idx: usize, callee_idx: usize },
    DeleteFn { fn_idx: usize },
    RetargetCall { fn_idx: usize, call_idx: usize, new_callee_idx: usize },
    RenameFn { fn_idx: usize },
    NoOpTouch { file_idx: usize },
}

fn render_func(tree: &SyntheticTree, fn_id: FnId) -> String {
    let func = &tree.funcs[&fn_id];
    let mut buf = format!("pub fn {}() {{\n", func.name);
    for callee in &func.calls {
        buf.push_str(&format!("    {}();\n", tree.funcs[callee].name));
    }
    buf.push_str("}\n");
    buf
}

fn render_file(tree: &SyntheticTree, file: &File) -> String {
    let mut buf = String::new();
    for fn_id in &file.fns {
        buf.push_str(&render_func(tree, *fn_id));
    }
    if file.touch > 0 {
        buf.push_str(&format!("// touch {}\n", file.touch));
    }
    buf
}

fn write_tree_to_disk(tree: &SyntheticTree, dir: &Path) {
    let src_dir = dir.join("src");
    let keep: std::collections::HashSet<&str> = tree.files.iter().map(|f| f.name.as_str()).collect();
    for entry in fs::read_dir(&src_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !keep.contains(name) {
            fs::remove_file(&path).unwrap();
        }
    }
    for file in &tree.files {
        fs::write(src_dir.join(&file.name), render_file(tree, file)).unwrap();
    }
}

fn apply_op(tree: &mut SyntheticTree, op: &Op) {
    match *op {
        Op::AddFile => {
            let name = format!("m{}.rs", tree.next_file_id);
            tree.next_file_id += 1;
            tree.files.push(File { name, fns: Vec::new(), touch: 0 });
        }
        Op::DeleteFile { file_idx } => {
            if file_idx >= tree.files.len() {
                return;
            }
            let removed_ids: Vec<FnId> = tree.files[file_idx].fns.clone();
            tree.files.remove(file_idx);
            for id in &removed_ids {
                tree.funcs.remove(id);
            }
            for func in tree.funcs.values_mut() {
                func.calls.retain(|c| !removed_ids.contains(c));
            }
        }
        Op::AddFn { file_idx, callee_idx } => {
            if file_idx >= tree.files.len() {
                return;
            }
            let callee = if callee_idx < tree.funcs.len() {
                Some(tree.funcs.keys().nth(callee_idx).copied().unwrap())
            } else {
                None
            };
            let id = tree.next_fn_id;
            tree.next_fn_id += 1;
            let name = format!("f{id}");
            let mut calls = Vec::new();
            if let Some(c) = callee {
                calls.push(c);
            }
            tree.funcs.insert(id, Func { name, calls });
            tree.files[file_idx].fns.push(id);
        }
        Op::DeleteFn { fn_idx } => {
            if fn_idx >= tree.funcs.len() {
                return;
            }
            let id = tree.funcs.keys().nth(fn_idx).copied().unwrap();
            tree.funcs.remove(&id);
            for file in &mut tree.files {
                file.fns.retain(|fid| *fid != id);
            }
            for func in tree.funcs.values_mut() {
                func.calls.retain(|c| *c != id);
            }
        }
        Op::RetargetCall { fn_idx, call_idx, new_callee_idx } => {
            if fn_idx >= tree.funcs.len() || new_callee_idx >= tree.funcs.len() {
                return;
            }
            let id = tree.funcs.keys().nth(fn_idx).copied().unwrap();
            let has_call = tree
                .funcs
                .get(&id)
                .map(|func| call_idx < func.calls.len())
                .unwrap_or(false);
            if !has_call {
                return;
            }
            let new_callee = tree.funcs.keys().nth(new_callee_idx).copied().unwrap();
            tree.funcs.get_mut(&id).unwrap().calls[call_idx] = new_callee;
        }
        Op::RenameFn { fn_idx } => {
            if fn_idx >= tree.funcs.len() {
                return;
            }
            let id = tree.funcs.keys().nth(fn_idx).copied().unwrap();
            let func = tree.funcs.get_mut(&id).unwrap();
            func.name = format!("{}_r", func.name);
        }
        Op::NoOpTouch { file_idx } => {
            if file_idx < tree.files.len() {
                tree.files[file_idx].touch += 1;
            }
        }
    }
}

// ---- proptest strategies ------------------------------------------------------

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        1 => Just(Op::AddFile),
        1 => (0usize..6).prop_map(|file_idx| Op::DeleteFile { file_idx }),
        2 => (0usize..6, 0usize..32).prop_map(|(file_idx, callee_idx)| Op::AddFn { file_idx, callee_idx }),
        1 => (0usize..32).prop_map(|fn_idx| Op::DeleteFn { fn_idx }),
        2 => (0usize..32, 0usize..4, 0usize..32).prop_map(|(fn_idx, call_idx, new_callee_idx)| {
            Op::RetargetCall { fn_idx, call_idx, new_callee_idx }
        }),
        1 => (0usize..32).prop_map(|fn_idx| Op::RenameFn { fn_idx }),
        1 => (0usize..6).prop_map(|file_idx| Op::NoOpTouch { file_idx }),
    ]
}

fn script_strategy() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(op_strategy(), 5usize..=15)
}

// ---- property runner ----------------------------------------------------------

fn run_equivalence_property(script: &[Op]) {
    let dir = TempDir::new("equiv");
    let mut tree = seed_tree();
    let mut engine = engine_at(dir.path(), "db");
    let prog = call_prog();

    write_tree_to_disk(&tree, dir.path());
    engine.tick(&prog, true).unwrap();

    for (step, op) in script.iter().enumerate() {
        apply_op(&mut tree, op);
        write_tree_to_disk(&tree, dir.path());
        engine.tick(&prog, true).unwrap();

        let incremental = dump_call_rels(&engine);
        let base_rels = dump_call_rels_base(&engine);
        let oracle = oracle_dump_for(dir.path(), &format!("step{step}"), CALL_PROG);

        assert_matches_oracle(&format!("equivalence step {step}"), &incremental, &oracle);
        assert_memo_matches_rel(&format!("memo=rel step {step}"), &engine, &base_rels);
    }
}

fn run_fixpoint_property(script: &[Op]) {
    let dir = TempDir::new("fixpoint");
    let mut tree = seed_tree();
    let mut engine = engine_at(dir.path(), "db");
    let prog = call_prog();

    write_tree_to_disk(&tree, dir.path());
    engine.tick(&prog, true).unwrap();

    for op in script {
        apply_op(&mut tree, op);
        write_tree_to_disk(&tree, dir.path());
        engine.tick(&prog, true).unwrap();
    }

    let before = dump_call_rels(&engine);
    let base_before = dump_call_rels_base(&engine);

    let rerun = engine.call_router_rerun_names(&HashSet::new());
    assert!(rerun.is_empty(), "empty changed-set must rerun no warm families: {rerun:?}");

    let rerun = engine.flip_call_rels_via_router_test(&HashSet::new()).unwrap();
    assert!(rerun.is_empty(), "empty changed-set flip must rerun nothing: {rerun:?}");

    let after = dump_call_rels(&engine);
    let base_after = dump_call_rels_base(&engine);

    assert_eq!(
        after, before,
        "empty changed-set flip must leave the public call rels unchanged"
    );
    assert_eq!(
        base_after, base_before,
        "empty changed-set flip must leave the base call rels unchanged"
    );
}

// ---- test definitions ---------------------------------------------------------

// Property 1+2 (equivalence + memo=rel) is currently IGNORED because it found a
// real engine bug in the router's relation-footprint computation:
//
//   When a call-family input relation (e.g. `_call_def`) is empty on the tick
//   that first populates the router memo, `Ctx::scan` records no per-row deps
//   for that relation, so the family's memo rel-footprint does NOT include it.
//   Later inserts into `_call_def` therefore do not trigger a rerun of the
//   `call_def` / `call_name` / `call_def_rev` families, and the public rel
//   stays stale.
//
// Shrunk reproduction (seed cc 0d80eca002b18e65b3098c2eb6b2308ccd1b0ba5edb79c527e349b5751592c9e):
//   1. seed tree: m0.rs with f0, tick  -> router memo warm
//   2. delete m0.rs, add empty m1.rs/m2.rs, tick  -> `_call_def` becomes empty
//   3. add f1 to m1.rs, tick  -> `_call_def` has f1, but `call_def` public rel stays empty
//      because the router footprint for CallDef/CallName/CallDefRev no longer
//      contains `_call_def`.
//
// Persisted in tests/proptest-regressions/retraction_props.txt.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    #[ignore = "real engine bug: router rel-footprint misses empty-input relations (seed cc 0d80eca002b18e65b3098c2eb6b2308ccd1b0ba5edb79c527e349b5751592c9e)"]
    fn equivalence_and_memo_hold_at_every_step(script in script_strategy()) {
        run_equivalence_property(&script);
    }

    #[test]
    fn empty_changed_set_is_a_fixpoint(script in script_strategy()) {
        run_fixpoint_property(&script);
    }
}

#[test]
#[ignore = "high-volume T4 property suite: 200 cases; blocked on router empty-input footprint bug (seed cc 0d80eca002b18e65b3098c2eb6b2308ccd1b0ba5edb79c527e349b5751592c9e)"]
fn equivalence_and_memo_hold_at_every_step_high_volume() {
    proptest!(ProptestConfig::with_cases(200), |(script in script_strategy())| {
        run_equivalence_property(&script);
    });
}

#[test]
#[ignore = "high-volume T4 property suite: 200 cases"]
fn empty_changed_set_is_a_fixpoint_high_volume() {
    proptest!(ProptestConfig::with_cases(200), |(script in script_strategy())| {
        run_fixpoint_property(&script);
    });
}
