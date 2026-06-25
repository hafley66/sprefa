//! Compile vendored tree-sitter grammar C sources into the crate.
//!
//! These two grammars have no usable crates.io dependency against our
//! tree-sitter 0.25 core:
//!   - dockerfile: only crate is 0.2.0, pins tree-sitter 0.20 (incompatible ABI).
//!   - go-template: no crate published at all.
//!
//! Both ship a generated `parser.c` (ABI 14/15, which 0.25 loads) plus the
//! `tree_sitter/` header dir. dockerfile also ships an external `scanner.c`.
//! We compile them here and declare/wrap the extern entry points in
//! src/engine.rs (ts_lang). Vendored sources: vendor/grammars/.

use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/grammars");

    // go-template: parser.c only, no external scanner.
    let gotmpl = root.join("gotmpl");
    let mut b = cc::Build::new();
    b.include(&gotmpl)
        .file(gotmpl.join("parser.c"))
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .warnings(false)
        .compile("tree_sitter_gotmpl");
    println!("cargo:rerun-if-changed={}", gotmpl.join("parser.c").display());

    // dockerfile: parser.c + external scanner.c.
    let dockerfile = root.join("dockerfile");
    let mut b = cc::Build::new();
    b.include(&dockerfile)
        .file(dockerfile.join("parser.c"))
        .file(dockerfile.join("scanner.c"))
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .warnings(false)
        .compile("tree_sitter_dockerfile");
    println!("cargo:rerun-if-changed={}", dockerfile.join("parser.c").display());
    println!("cargo:rerun-if-changed={}", dockerfile.join("scanner.c").display());
}
