// `build.rs` helper. Walk a directory of in-tree DSL packages, compile each
// committed `parser.c` (and optional `scanner.c`) via `cc::Build`, and emit
// the right `cargo:rerun-if-changed` lines.
//
// Layout assumed:
//     <grammars_dir>/<dsl-name>/grammar.js          (optional)
//     <grammars_dir>/<dsl-name>/src/parser.c        (required)
//     <grammars_dir>/<dsl-name>/src/scanner.c       (optional)
//
// Each subdirectory containing `src/parser.c` produces a static lib named
// `tree-sitter-<dsl-name>`. The consumer imports the language symbol the
// usual tree-sitter way (extern "C" fn `tree_sitter_<dsl_name>`).
//
// Skipped subdirectories: anything without `src/parser.c`. Non-directories
// and dot-files are ignored.
//
// This file is shared between the lib (`cst::build::grammar_walk`) and the
// crate's own `build.rs` via `include!`. Keep it free of `//!` inner doc
// comments so the include site stays valid in both positions.

use std::path::Path as StdPath;

pub fn compile_grammars(grammars_dir: impl AsRef<StdPath>) {
    let dir = grammars_dir.as_ref();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let dsl_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) if !s.starts_with('.') => s,
            _ => continue,
        };
        let parser_c  = path.join("src").join("parser.c");
        if !parser_c.exists() { continue; }
        let scanner_c = path.join("src").join("scanner.c");
        let grammar_js = path.join("grammar.js");

        let mut build = cc::Build::new();
        build
            .include(path.join("src"))
            .flag_if_supported("-Wno-unused-parameter")
            .flag_if_supported("-Wno-unused-but-set-variable")
            .flag_if_supported("-Wno-trigraphs")
            .file(&parser_c);
        if scanner_c.exists() {
            build.file(&scanner_c);
        }
        let lib_name = format!("tree-sitter-{dsl_name}");
        build.compile(&lib_name);

        println!("cargo:rerun-if-changed={}", parser_c.display());
        if scanner_c.exists() {
            println!("cargo:rerun-if-changed={}", scanner_c.display());
        }
        if grammar_js.exists() {
            println!("cargo:rerun-if-changed={}", grammar_js.display());
        }
    }
}
