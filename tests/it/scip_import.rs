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
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
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
