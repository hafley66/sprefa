//! Integration test: a host doc (rust) carries one Injected (json) covering a
//! byte range inside a string literal. locate(byte) inside the json body must
//! descend, return a path containing `Body`, and resolve(path) must round-trip
//! back to the same node in the json sub-tree.

use std::sync::Arc;

use v4::cst::doc::{Doc, DocId};
use v4::cst::injected::Injected;
use v4::cst::locate::{locate, resolve};
use v4::cst::path::{items, PathBuilder};

fn parse_rust(src: &str) -> tree_sitter::Tree {
    let mut p = tree_sitter::Parser::new();
    p.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
    p.parse(src, None).expect("parse rust")
}

fn parse_json(src: &str) -> tree_sitter::Tree {
    let mut p = tree_sitter::Parser::new();
    p.set_language(&tree_sitter_json::LANGUAGE.into()).unwrap();
    p.parse(src, None).expect("parse json")
}

#[test]
fn locate_descends_into_injected_subtree_and_resolve_round_trips() {
    // Host source: a string literal containing a json fragment.
    // We simulate "the host parser noticed a json carveout starting at byte 13".
    let host_src = "static S: &str = {\"k\":42};"; // not legal rust as-is, fine for the test
    let host_tree = parse_rust(host_src);

    // Injected json body: bytes 16..25 in the host source: `{"k":42}`.
    let body_off = host_src.find('{').unwrap();
    let body = &host_src[body_off..body_off + 8];
    assert_eq!(body, "{\"k\":42}");
    let json_tree = parse_json(body);

    // host_path — a stable address for the injection slot. For this test we
    // synthesize an arbitrary one; in real use the host generates it via
    // its own segment emission.
    let mut hp = PathBuilder::new();
    hp.push(items::Index(0));
    hp.push(items::Field { kind_id: 1, idx: 0 });
    let host_path = hp.build();

    let inj = Injected {
        dsl_id: Arc::from("json"),
        host_path: host_path.clone(),
        host_off: body_off,
        tree: json_tree,
        source: Arc::from(body),
        injected: vec![],
    };

    let doc = Doc {
        doc_id: DocId::new("file:///t.rs"),
        source: Arc::from(host_src),
        tree: host_tree,
        injected: vec![inj],
        version: 0,
    };

    // Pick a host byte inside the injected body (the digit `4` of `42`).
    let host_byte = body_off + 5; // body[5] == '4'
    assert_eq!(host_src.as_bytes()[host_byte], b'4');

    let loc = locate(&doc, host_byte);
    assert_eq!(
        loc.dsl_id.as_ref(),
        "json",
        "locate should land in the json injection"
    );
    assert_eq!(loc.host_off, body_off);
    assert_eq!(loc.trail.len(), 1, "one descent step");

    // The path must include a Body item (the cross-into-content marker).
    let has_body = loc.path.iter().any(|it| it.kind_tag() == "Body");
    assert!(
        has_body,
        "locate path should contain a Body marker, got {:?}",
        v4::cst::path::PathKey(loc.path.clone())
    );

    // Resolve back: should return the same node id within the json sub-tree.
    let resolved = resolve(&doc, &loc.path).expect("resolve across injection");
    assert_eq!(
        resolved.id(),
        loc.node.id(),
        "round-trip node id must match"
    );
}
