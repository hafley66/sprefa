//! CST-as-relation (christmas #3): `node(id, kind, file, lo, hi, parent)` and
//! `child(parent, child)` as lazy built-in query rels. Covers cross-language
//! nodes, child parent/child correctness, `closure(child)` ancestry, the
//! innermost-containment query, and a node id round-tripping through BOTH `ref`
//! (its span) AND `string` (its source bytes).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cst_node_rel_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run_json(dir: &PathBuf, prog: &str) -> Vec<serde_json::Value> {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap(), "--query-json"])
        .output().expect("run dl");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad json line `{l}`: {e}")))
        .collect()
}

fn q<'a>(recs: &'a [serde_json::Value], name: &str) -> &'a serde_json::Value {
    recs.iter().find(|r| r["query"] == name).unwrap_or_else(|| panic!("no query {name} in {recs:?}"))
}

#[test]
fn nodes_across_two_languages() {
    let d = sandbox("xlang");
    fs::write(d.join("src/a.rs"), "fn alpha() {}\n").unwrap();
    fs::write(d.join("src/b.py"), "def beta():\n    pass\n").unwrap();

    // A scan rule populates `_file`; the node walk reads it.
    let prog = r#"
rel seen(path: text).
rel kinds(kind: text).
seen(path) <- scan("WORK","src/**/*",path,rev), match(path, rev, /./, _l).
kinds(kind) <- node(_id, kind, _f, _lo, _hi, _p).
? kinds(kind).
"#;
    let recs = run_json(&d, prog);
    let kinds = q(&recs, "kinds");
    let set: Vec<String> = kinds["rows"].as_array().unwrap().iter()
        .map(|r| r[0].as_str().unwrap().to_string()).collect();
    // Rust grammar
    assert!(set.contains(&"function_item".to_string()), "rust function_item: {set:?}");
    // Python grammar
    assert!(set.contains(&"function_definition".to_string()), "python function_definition: {set:?}");
}

#[test]
fn child_edges_and_closure_ancestry() {
    let d = sandbox("closure");
    fs::write(d.join("src/a.rs"), "fn alpha() {}\n").unwrap();

    // `anc(a,b) <- closure(child).` uses the engine's existing recursion (child
    // is 2-col). The identifier `alpha` must be a descendant of the root
    // source_file. Pin the root by querying a known ancestor edge: the
    // function_item is a direct child of source_file.
    let prog = r#"
rel seen(path: text).
rel root_id(id: text).
rel fi_id(id: text).
rel direct(p: text, c: text).
rel anc(a: text, b: text).
seen(path) <- scan("WORK","src/**/*",path,rev), match(path, rev, /./, _l).
root_id(id) <- node(id, "source_file", _f, _lo, _hi, _p).
fi_id(id) <- node(id, "function_item", _f, _lo, _hi, _p).
direct(p, c) <- child(p, c).
anc(a, b) <- closure(child).
? root_id(id).
? fi_id(id).
"#;
    let recs = run_json(&d, prog);
    let root = q(&recs, "root_id");
    let fi = q(&recs, "fi_id");
    assert_eq!(root["count"], 1, "one source_file root: {root}");
    assert_eq!(fi["count"], 1, "one function_item: {fi}");

    let root_id = root["rows"][0][0].as_str().unwrap().to_string();
    let fi_id = fi["rows"][0][0].as_str().unwrap().to_string();
    assert_ne!(root_id, fi_id, "root and function_item have distinct ids");

    // Now assert the closure reaches fi from root via a seeded ancestor query.
    let prog2 = format!(r#"
rel seen(path: text).
rel anc(a: text, b: text).
rel reach(b: text).
seen(path) <- scan("WORK","src/**/*",path,rev), match(path, rev, /./, _l).
anc(a, b) <- closure(child).
reach(b) <- anc("{root_id}", b).
? reach(b).
"#);
    let recs2 = run_json(&d, &prog2);
    let reach = q(&recs2, "reach");
    let reached: Vec<String> = reach["rows"].as_array().unwrap().iter()
        .map(|r| r[0].as_str().unwrap().to_string()).collect();
    assert!(reached.contains(&fi_id),
        "closure(child) reaches the function_item from the root: {reached:?} should contain {fi_id}");
}

#[test]
fn innermost_containment_picks_tightest_node() {
    let d = sandbox("contain");
    // `fn alpha() { let x = 1; }` — byte 18 falls inside `let x = 1` (the
    // let_declaration), the block, the function_item, and the source_file. The
    // innermost (smallest span) containing byte 18 is the let_declaration / its
    // children, NOT the whole file.
    fs::write(d.join("src/a.rs"), "fn alpha() { let x = 1; }\n").unwrap();

    // contains(Id, Hi-Lo) for every node whose [lo,hi) holds byte 18; the test
    // asserts the minimum-span containing node is tighter than the source_file.
    let prog = r#"
rel seen(path: text).
rel cover(kind: text, span: int).
seen(path) <- scan("WORK","src/**/*",path,rev), match(path, rev, /./, _l).
cover(kind, hi - lo) <- node(_id, kind, _f, lo, hi, _p), lo <= 18, 18 < hi.
? cover(kind, span).
"#;
    let recs = run_json(&d, prog);
    let cover = q(&recs, "cover");
    let mut rows: Vec<(String, i64)> = cover["rows"].as_array().unwrap().iter()
        .map(|r| (r[0].as_str().unwrap().to_string(), r[1].as_i64().unwrap())).collect();
    rows.sort_by_key(|(_, s)| *s);
    assert!(!rows.is_empty(), "byte 18 is covered by some node");
    // The tightest (first) node is NOT the whole source_file.
    let (tightest_kind, tightest_span) = &rows[0];
    let source_span = rows.iter().find(|(k, _)| k == "source_file").map(|(_, s)| *s).unwrap_or(i64::MAX);
    assert!(*tightest_span < source_span,
        "innermost node {tightest_kind}({tightest_span}) is tighter than source_file({source_span}): {rows:?}");
}

#[test]
fn node_id_resolves_through_ref_and_string() {
    let d = sandbox("idresolve");
    fs::write(d.join("src/a.rs"), "fn alpha() {}\n").unwrap();

    // Take the `identifier` node whose text is `alpha`; its id must resolve via
    // ref(id, sid, file, lo, hi) -> string(sid, text, _) to "alpha".
    let prog = r#"
rel seen(path: text).
rel ident(id: text).
rel located(lo: int, hi: int).
rel texted(t: text).
seen(path) <- scan("WORK","src/**/*",path,rev), match(path, rev, /./, _l).
ident(id) <- node(id, "identifier", _f, lo, hi, _p), lo = 3, hi = 8.
located(lo, hi) <- ident(id), ref(id, _s, _f, lo, hi).
texted(t) <- ident(id), ref(id, sid, _f, _lo, _hi), string(sid, t, _n).
? located(lo, hi).
? texted(t).
"#;
    let recs = run_json(&d, prog);
    let loc = q(&recs, "located");
    assert_eq!(loc["count"], 1, "identifier id resolves through ref: {loc}");
    assert_eq!(loc["rows"][0][0].as_i64().unwrap(), 3, "lo");
    assert_eq!(loc["rows"][0][1].as_i64().unwrap(), 8, "hi");

    let txt = q(&recs, "texted");
    assert_eq!(txt["count"], 1, "identifier id resolves through string: {txt}");
    assert_eq!(txt["rows"][0][0], "alpha",
        "node id -> ref -> string recovers the source bytes `alpha`: {txt}");
}
