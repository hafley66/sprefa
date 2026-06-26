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
dep(src, dst) <- scip_edge(src, dst).
? scip_def(symbol, file).
? scip_ref(file, symbol, def_file).
? dep(src, dst).
"#;
    let (code, out, err) = run(&d, prog, &index);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    assert!(out.contains("src/lib.rs"), "definition file imported: {out}");
    assert!(out.contains("src/main.rs"), "reference file imported: {out}");
    assert!(out.contains("src/main.rs\tsrc/lib.rs"), "SCIP edge feeds derived relation: {out}");
}
