use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use scip::types::{Document, Index, Occurrence, SymbolRole};

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("scip_import_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn occurrence(symbol: &str, roles: i32) -> Occurrence {
    let mut occ = Occurrence::new();
    occ.symbol = symbol.to_string();
    occ.symbol_roles = roles;
    occ
}

/// Like `occurrence` but with a SCIP packed range `[start_line, start_col,
/// end_line, end_col]` (0-based). Needed for fn-level attribution, which uses
/// each reference's start position to find its enclosing fn def.
fn occurrence_r(symbol: &str, roles: i32, range: [i32; 4]) -> Occurrence {
    let mut occ = occurrence(symbol, roles);
    occ.range = range.to_vec();
    occ
}

fn document(path: &str, occurrences: Vec<Occurrence>) -> Document {
    let mut doc = Document::new();
    doc.relative_path = path.to_string();
    doc.occurrences = occurrences;
    doc
}

fn write_index(path: &Path) {
    let sym = "rust-analyzer cargo test 1.0.0 `crate`/answer().";
    let mut index = Index::new();
    index.documents = vec![
        document("src/lib.rs", vec![occurrence(sym, SymbolRole::Definition as i32)]),
        document("src/main.rs", vec![occurrence(sym, 0)]),
    ];
    scip::write_message_to_file(path, index).unwrap();
}

fn run(dir: &Path, prog: &str, index: &Path) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .current_dir(dir)
        .args(["--db", dir.join("db").to_str().unwrap()])
        .env("SPREFA_SCIP_INDEX", index)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn imports_scip_def_ref_and_edge_relations() {
    let d = sandbox("basic");
    let index = d.join("index.scip");
    write_index(&index);
    let prog = r#"
rel dep(src: text, dst: text).
dep(src, dst) <- scip_edge(src, dst, _).
? scip_def(symbol, file, repo).
? scip_ref(file, symbol, def_file, repo).
? dep(src, dst).
"#;
    let (code, out, err) = run(&d, prog, &index);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("src/lib.rs"), "definition file imported: {out}");
    assert!(out.contains("src/main.rs"), "reference file imported: {out}");
    assert!(out.contains("src/main.rs\tsrc/lib.rs"), "SCIP edge feeds derived relation: {out}");
}

/// A fn-level call graph requires (a) the caller's def to carry a `(` in its
/// symbol (the fn-like filter), (b) the reference's start position to fall after
/// the caller def's start in the SAME file (predecessor-search attribution).
/// RA's def ranges cover only the fn name, not the body, so attribution is by
/// "most recent fn def at or before the ref" — verified here by placing the ref
/// on a later line than the caller def within one document.
#[test]
fn extracts_fn_level_call_edges() {
    let d = sandbox("fn_edge");
    let index = d.join("index.scip");
    let callee = "rust-analyzer cargo test 1.0.0 `crate`/callee().";
    let caller = "rust-analyzer cargo test 1.0.0 `crate`/caller().";
    let mut idx = Index::new();
    // callee def at line 0; caller fn def at line 2; ref to callee at line 5,
    // inside caller's body per predecessor search (most recent start <= 5 is 2).
    idx.documents = vec![document("src/lib.rs", vec![
        occurrence_r(callee, SymbolRole::Definition as i32, [0, 0, 0, 10]),
        occurrence_r(caller, SymbolRole::Definition as i32, [2, 0, 2, 8]),
        occurrence_r(callee, 0, [5, 0, 5, 8]),
    ])];
    scip::write_message_to_file(&index, idx).unwrap();
    let prog = r#"
rel e(caller: text, callee: text).
e(caller, callee) <- scip_fn_edge(caller, callee).
? e(caller, callee).
"#;
    let (code, out, err) = run(&d, prog, &index);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    let row = out.lines().find(|l| l.contains("caller()") && l.contains("callee()"))
        .unwrap_or_else(|| panic!("no fn_edge row in:\nstdout={out}\nstderr={err}"));
    assert!(row.contains("caller()") && row.contains("callee()"),
        "fn_edge row should name both fns: {row}");
}

/// A reference before the first fn def in its file is module-level. With
/// name-only def ranges the predecessor search can't distinguish "inside the
/// preceding fn's body" from "after it at module scope", so such a ref
/// attributes to the most-recently-started fn (here: callee itself, producing
/// a self-edge). The unit test in scip_import.rs documents this directly; this
/// e2e case confirms the self-edge is visible through the full pipeline so a
/// downstream rule can filter caller==callee if it cares.
#[test]
fn ref_before_real_fn_yields_self_edge() {
    let d = sandbox("fn_no_caller");
    let index = d.join("index.scip");
    let callee = "rust-analyzer cargo test 1.0.0 `crate`/callee().";
    let caller = "rust-analyzer cargo test 1.0.0 `crate`/caller().";
    let mut idx = Index::new();
    // caller def at line 10; ref to callee at line 2 — before caller starts.
    // Predecessor search finds callee (line 0) → self-edge callee→callee.
    idx.documents = vec![document("src/lib.rs", vec![
        occurrence_r(callee, SymbolRole::Definition as i32, [0, 0, 0, 10]),
        occurrence_r(caller, SymbolRole::Definition as i32, [10, 0, 10, 8]),
        occurrence_r(callee, 0, [2, 0, 2, 8]),
    ])];
    scip::write_message_to_file(&index, idx).unwrap();
    let prog = r#"
rel e(caller: text, callee: text).
e(caller, callee) <- scip_fn_edge(caller, callee).
? e(caller, callee).
"#;
    let (code, out, err) = run(&d, prog, &index);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("callee()"), "self-edge should appear: stdout={out}\nstderr={err}");
}

/// S1: `scip_occurrence` surfaces the 0-based line/col span + role that
/// `scip_ref` lacked. A def and a reference of the same symbol on different
/// lines produce two rows, distinguished by role, positions intact. The
/// co-queried `scip_def` proves this is a PURE ADDITION — the def/ref/edge
/// rels are unchanged.
#[test]
fn imports_occurrence_spans_with_roles() {
    let d = sandbox("occurrence");
    let index = d.join("index.scip");
    let sym = "pkg/foo().";
    let mut idx = Index::new();
    idx.documents = vec![document("src/lib.rs", vec![
        occurrence_r(sym, SymbolRole::Definition as i32, [3, 4, 3, 7]),
        occurrence_r(sym, 0, [7, 8, 7, 11]),
        occurrence_r(sym, SymbolRole::Import as i32, [1, 4, 1, 7]),
        occurrence_r(sym, SymbolRole::WriteAccess as i32, [9, 0, 9, 3]),
        // read+write compound: write outranks read in role_label
        occurrence_r(sym, (SymbolRole::ReadAccess as i32) | (SymbolRole::WriteAccess as i32), [10, 0, 10, 3]),
        occurrence_r(sym, SymbolRole::ReadAccess as i32, [11, 0, 11, 3]),
    ])];
    scip::write_message_to_file(&index, idx).unwrap();
    let prog = r#"
? scip_occurrence(file, symbol, line, col, end_line, end_col, role, repo).
? scip_def(symbol, file, repo).
"#;
    let (code, out, err) = run(&d, prog, &index);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    // The definition occurrence: exact file/symbol/span/role prefix (repo is the
    // sandbox slug, not asserted).
    assert!(out.contains("src/lib.rs\tpkg/foo().\t3\t4\t3\t7\tdefinition"),
        "definition occurrence with span+role: {out}");
    assert!(out.contains("src/lib.rs\tpkg/foo().\t7\t8\t7\t11\treference"),
        "reference occurrence with span+role: {out}");
    // Widened role vocabulary: import/write/read bits survive instead of
    // collapsing to reference; compound read+write reports write.
    assert!(out.contains("src/lib.rs\tpkg/foo().\t1\t4\t1\t7\timport"),
        "import occurrence role: {out}");
    assert!(out.contains("src/lib.rs\tpkg/foo().\t9\t0\t9\t3\twrite"),
        "write occurrence role: {out}");
    assert!(out.contains("src/lib.rs\tpkg/foo().\t10\t0\t10\t3\twrite"),
        "compound read+write reports write: {out}");
    assert!(out.contains("src/lib.rs\tpkg/foo().\t11\t0\t11\t3\tread"),
        "read occurrence role: {out}");
    // scip_def still emits — pure addition, def rel untouched.
    assert!(out.contains("pkg/foo().\tsrc/lib.rs"), "scip_def unchanged: {out}");
}

/// S2: the LOCAL binding name is joinable to the canonical symbol. An aliased
/// import `import { foo as bar }` has the canonical `foo` symbol occurring over
/// `bar`'s range; `scip_binding` slices "bar" from the source and pairs it with
/// the canonical symbol, so a name-based join on the local name — which
/// `scip_name` (canonical-only) silently dropped — now lands.
#[test]
fn binds_aliased_import_local_name() {
    let d = sandbox("binding_alias");
    let index = d.join("index.scip");
    // The aliased import must exist ON DISK — scip_binding slices WORK content.
    fs::write(d.join("app.ts"), "import { foo as bar } from \"./mod\"\n").unwrap();
    let canonical = "scip-typescript npm mod 1.0.0 `mod`/foo#";
    let mut idx = Index::new();
    // `bar` is UTF-16 cols [16, 19) on line 0.
    idx.documents = vec![document("app.ts", vec![
        occurrence_r(canonical, 0, [0, 16, 0, 19]),
    ])];
    scip::write_message_to_file(&index, idx).unwrap();
    let prog = r#"
rel resolved(local: text, sym: text).
resolved(local, sym) <- scip_binding(_, sym, local, _, _, _).
? resolved(local, sym).
"#;
    let (code, out, err) = run(&d, prog, &index);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    // The join on the LOCAL name "bar" resolves to the CANONICAL symbol.
    assert!(out.contains("bar\tscip-typescript npm mod 1.0.0 `mod`/foo#"),
        "local name bar joins to canonical foo symbol: {out}");
}
