//! Integration test: build a real tree-sitter Doc, locate by byte, get back
//! a Path; resolve the Path back to the same Node.

use std::sync::Arc;

use v4::cst::doc::{Doc, DocId};
use v4::cst::locate::{locate, resolve};

fn parse_rust(src: &str) -> tree_sitter::Tree {
    let mut p = tree_sitter::Parser::new();
    p.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
    p.parse(src, None).expect("parse")
}

#[test]
fn locate_returns_a_path_inside_function_body() {
    let src = "fn foo() { let x = 7; }";
    let tree = parse_rust(src);
    let doc = Doc {
        doc_id: DocId::new("file:///t.rs"),
        source: Arc::from(src),
        tree,
        injected: vec![],
        version: 0,
    };

    // Byte 11 is `l` of `let`.
    let loc = locate(&doc, 11);
    assert_eq!(loc.dsl_id.as_ref(), "");
    assert!(
        !loc.path.is_empty(),
        "expected a non-empty path inside the file"
    );
    assert!(loc.node.byte_range().contains(&11));
}

#[test]
fn locate_then_resolve_round_trip() {
    let src = "fn foo() { let x = 7; }";
    let tree = parse_rust(src);
    let doc = Doc {
        doc_id: DocId::new("file:///t.rs"),
        source: Arc::from(src),
        tree,
        injected: vec![],
        version: 0,
    };

    let loc = locate(&doc, 15); // somewhere in `let x = 7;`
    let resolved = resolve(&doc, &loc.path).expect("resolve");
    assert_eq!(
        resolved.id(),
        loc.node.id(),
        "resolve should land on the same node"
    );
    assert_eq!(resolved.kind(), loc.node.kind());
}

#[test]
fn locate_at_root_returns_empty_or_top_level_path() {
    let src = "fn foo() {}";
    let tree = parse_rust(src);
    let doc = Doc {
        doc_id: DocId::new("file:///t.rs"),
        source: Arc::from(src),
        tree,
        injected: vec![],
        version: 0,
    };
    // Byte 0 — top of file. Either source_file root or its first child.
    let loc = locate(&doc, 0);
    let resolved = resolve(&doc, &loc.path).expect("resolve");
    assert_eq!(resolved.id(), loc.node.id());
}
