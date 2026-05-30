//! Proof of the cross-language module graph (modgraph.rs): the built-in
//! `module_edge`/`module_import`/`module_unresolved` relations are populated by
//! the Rust resolver from `mod`/`use` decls, and `reaches(a,b) <- closure(module_edge)`
//! gives transitive file-to-file reach. "The filesystem from language."

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("modgraph_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .args(extra)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

// A scan rule populates the file set (built-in `file`); the resolver then runs
// over it. Module relations are unpopulated unless the program references one.
const PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), sg(path, rev, :rust, "fn $N() {}", line).
rel reaches(a: text, b: text).
reaches(a, b) <- closure(module_edge).
? module_edge(s, d).
? reaches(a, b).
? module_unresolved(f, spec, why).
"#;

#[test]
fn rust_mod_and_use_edges_with_closure() {
    let d = sandbox("rust");
    fs::create_dir_all(d.join("src")).unwrap();
    // lib.rs declares two submodules; a.rs uses b; b.rs is a leaf; mod missing has no file.
    fs::write(d.join("src/lib.rs"), "mod a;\nmod b;\nmod missing;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/a.rs"), "use crate::b::Thing;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/b.rs"), "pub struct Thing;\nfn x() {}\n").unwrap();

    let (code, out, _) = run(&d, PROG, &[]);
    assert_eq!(code, 0, "run failed: {out}");

    // mod edges (filesystem inclusion)
    assert!(out.contains("src/lib.rs\tsrc/a.rs"), "lib->a mod edge: {out}");
    assert!(out.contains("src/lib.rs\tsrc/b.rs"), "lib->b mod edge: {out}");
    // use edge (cross-module dependency)
    assert!(out.contains("src/a.rs\tsrc/b.rs"), "a->b use edge: {out}");

    // transitive reach: lib -> a -> b means reaches(lib, b) holds
    let reaches_block = out.split("? reaches").nth(1).unwrap_or("");
    assert!(reaches_block.contains("src/lib.rs\tsrc/b.rs"), "transitive reaches(lib,b): {out}");
    assert!(reaches_block.contains("src/a.rs\tsrc/b.rs"), "reaches(a,b): {out}");

    // `mod missing;` has no child file -> module_unresolved
    let unres_block = out.split("? module_unresolved").nth(1).unwrap_or("");
    assert!(unres_block.contains("missing"), "mod missing must be unresolved: {out}");
}

#[test]
fn unreferenced_program_does_not_populate_module_rels() {
    // A program that never mentions a module relation pays nothing: the resolver
    // pass is skipped. Verified indirectly — querying file works, no module rows.
    let d = sandbox("lazy");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.rs"), "mod a;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/a.rs"), "fn x() {}\n").unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), sg(path, rev, :rust, "fn $N() {}", line).
? seen(p).
"#;
    let (code, out, _) = run(&d, prog, &[]);
    assert_eq!(code, 0);
    assert!(out.contains("(2 rows)"), "both files seen: {out}");
}

#[test]
fn edge_drops_when_target_file_deleted() {
    let d = sandbox("drop");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.rs"), "mod a;\nfn x() {}\n").unwrap();
    fs::write(d.join("src/a.rs"), "fn x() {}\n").unwrap();

    let (_, out, _) = run(&d, PROG, &[]);
    assert!(out.contains("src/lib.rs\tsrc/a.rs"), "cold: lib->a edge: {out}");

    // Delete the submodule file and tick that path. The mod decl in lib.rs still
    // exists but now resolves to nothing -> edge drops, becomes unresolved.
    fs::remove_file(d.join("src/a.rs")).unwrap();
    let _ = run(&d, PROG, &["--changed", "src/a.rs"]);
    let (_, out2, _) = run(&d, PROG, &[]);
    assert!(!out2.contains("src/lib.rs\tsrc/a.rs"), "edge must drop when a.rs gone: {out2}");
}
