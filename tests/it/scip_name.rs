//! `scip_name(symbol, name)` is the SCIP symbol's descriptor name (last
//! identifier run), computed in-engine. The motivation: extracting it in pure dl
//! with `split` fails on ~42% of real rust-analyzer monikers — methods and trait
//! impls carry `impl#[Type]` / `for#[Type]` segments that a single-separator
//! split can't strip. This test generates a REAL index over a fixture covering
//! every moniker shape (free fn, inherent method, trait impl, field, variant) and
//! asserts every name reduces to a bare identifier, and that the impl-method name
//! survives (the shape the naive split broke on).
//!
//! Skips (does not fail) when no rust-analyzer is found. Set SPREFA_RUST_ANALYZER
//! to point at one.

use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// Same RA discovery as oracle_rust: explicit env, else the VSCode-bundled server.
fn find_ra() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_RUST_ANALYZER") {
        let p = PathBuf::from(p);
        if p.is_file() { return Some(p); }
    }
    let home = std::env::var("HOME").ok()?;
    let ext = Path::new(&home).join(".vscode/extensions");
    let mut found: Option<PathBuf> = None;
    if let Ok(entries) = std::fs::read_dir(&ext) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("rust-lang.rust-analyzer-") {
                let bin = e.path().join("server/rust-analyzer");
                if bin.is_file() { found = Some(bin); }
            }
        }
    }
    found
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() { copy_dir(&p, &d); } else { std::fs::copy(&p, &d).unwrap(); }
    }
}

/// names emitted by `? scip_name` (the column-2 value of each tab row).
fn scip_names(root: &Path, index: &Path) -> Vec<String> {
    let prog = "rel here(p: file).\nhere(p) <- scan(\"WORK\", \"Cargo.toml\", p, rev).\n? scip_name(symbol, name).\n";
    std::fs::write(root.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(root.join("p.dl"))
        .args(["--db", root.join("p.db").to_str().unwrap()])
        .current_dir(root)
        .env("SPREFA_SCIP_INDEX", index)
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut names = Vec::new();
    let mut in_q = false;
    for line in stdout.lines() {
        if line.starts_with("? scip_name") { in_q = true; continue; }
        if line.starts_with('?') { in_q = false; continue; }
        if in_q {
            if let Some((_sym, name)) = line.trim_end().split_once('\t') {
                names.push(name.to_string());
            }
        }
    }
    names
}

fn is_bare_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[test]
fn scip_name_reduces_every_real_moniker_to_a_bare_identifier() {
    let Some(ra) = find_ra() else {
        eprintln!("SKIP scip_name: no rust-analyzer binary (set SPREFA_RUST_ANALYZER)");
        return;
    };
    let root = std::env::temp_dir().join("sprefa_scip_names_ws");
    let _ = std::fs::remove_dir_all(&root);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scip_names");
    copy_dir(&fixture, &root);

    let index = root.join("index.scip");
    let status = Command::new(&ra)
        .args(["scip", root.to_str().unwrap(), "--output", index.to_str().unwrap()])
        .output().expect("run rust-analyzer scip");
    assert!(index.is_file(), "rust-analyzer scip produced no index: {}",
        String::from_utf8_lossy(&status.stderr));

    let names = scip_names(&root, &index);
    assert!(!names.is_empty(), "no scip_name rows produced");

    // every descriptor name is a bare identifier — no `#`, `[`, `]`, `(`, `.`,
    // `impl`/`for` structural residue.
    let dirty: Vec<&String> = names.iter().filter(|n| !is_bare_ident(n)).collect();
    assert!(dirty.is_empty(), "scip_name leaked non-identifier descriptors: {dirty:?}");

    // the shapes the naive split broke on must be present and clean.
    for want in ["free_function", "inherent_method", "draw_shape", "inner_field"] {
        assert!(names.iter().any(|n| n == want),
            "expected {want:?} among scip_name descriptors: {names:?}");
    }
}
