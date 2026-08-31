//! Go's embedded-struct method promotion: `o.M()` where `M` is declared on a
//! type `o` embeds, at any depth up to the cap. The receiver-method lookup
//! matched a def whose OWNER equals the receiver type name, so every promoted
//! call declined. Go's own rule decides ties: the shallowest depth wins, two
//! candidates at one depth bind nothing, and the receiver's own method
//! shadows a promoted one of the same name.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.
//!
//! TEST HEADER, HEAD failure before the fix (`cargo test --test 69_go_promoted`):
//!   depth_one_embed          FAILED  one InnerPing edge: []
//!   pointer_embed            FAILED  one InnerPing edge: []
//!   cross_package_embed      FAILED  one WriteBase edge: []
//!   depth_four_embed         FAILED  one Deep5 edge: []
//!   inferred_receiver_embed  FAILED  one InnerPing edge: []
//!   own_method_shadows_promoted  ok
//!   depth_five_embed             ok (was depth_five_declines: the cap was 4)
//!   ambiguous_embeds_decline     ok

use std::process::Command;

fn fixture(rel: &str) -> String {
    format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn promoted_dir() -> Vec<String> {
    let mut paths = walk(&fixture("go_promoted"));
    paths.sort();
    paths
}

fn walk(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("fixture dir readable") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            out.extend(walk(&path.to_string_lossy()));
        } else if path.extension().is_some_and(|ext| ext == "go") {
            out.push(path.to_string_lossy().into_owned());
        }
    }
    out
}

/// `(caller_name, callee_name, callee_path, kind)` per resolved edge of one
/// `--resolve` run over the fixture dir.
fn resolved_edges() -> Vec<(String, String, String, String)> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(promoted_dir())
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("utf8 wire")
        .lines()
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).ok()?;
            (row["record"] == "resolved_edge").then(|| {
                (
                    row["caller_name"].as_str().unwrap_or("").to_string(),
                    row["callee_name"].as_str().unwrap_or("").to_string(),
                    row["callee_path"].as_str().unwrap_or("").to_string(),
                    row["kind"].as_str().unwrap_or("").to_string(),
                )
            })
        })
        .collect()
}

fn callables<'a>(
    edges: &'a [(String, String, String, String)],
    caller: &str,
    callee: &str,
) -> Vec<&'a (String, String, String, String)> {
    edges
        .iter()
        .filter(|e| e.0 == caller && e.1 == callee)
        .collect()
}

/// `Outer` embeds `Inner` by value: one hop.
#[test]
fn depth_one_embed() {
    let edges = resolved_edges();
    let hit = callables(&edges, "callDepthOne", "InnerPing");
    assert_eq!(hit.len(), 1, "one InnerPing edge: {hit:?}");
    assert!(hit[0].2.ends_with("lib.go"), "bound in lib.go: {hit:?}");
    assert_eq!(hit[0].3, "name_resolve");
}

/// `PtrHolder` embeds `*Inner`; the pointer is stripped like any other
/// declared type.
#[test]
fn pointer_embed() {
    let edges = resolved_edges();
    let hit = callables(&edges, "callThroughPointer", "InnerPing");
    assert_eq!(hit.len(), 1, "one InnerPing edge: {hit:?}");
    assert!(hit[0].2.ends_with("lib.go"), "bound in lib.go: {hit:?}");
}

/// `Importer` embeds `base.Writer`: the qualifier resolves through the
/// DECLARING file's own imports, so the walk crosses a package directory.
#[test]
fn cross_package_embed() {
    let edges = resolved_edges();
    let hit = callables(&edges, "callCrossPackage", "WriteBase");
    assert_eq!(hit.len(), 1, "one WriteBase edge: {hit:?}");
    assert!(hit[0].2.ends_with("base.go"), "bound in base.go: {hit:?}");
}

/// `D1` reaches `D5`'s method through four embedded fields, the cap.
#[test]
fn depth_four_embed() {
    let edges = resolved_edges();
    let hit = callables(&edges, "callDepthFour", "Deep5");
    assert_eq!(hit.len(), 1, "one Deep5 edge: {hit:?}");
    assert!(hit[0].2.ends_with("lib.go"), "bound in lib.go: {hit:?}");
}

/// The cap is 9 (the ast.Node hierarchy reaches 9 embeds, #577), so `D0` at
/// five embeds binds, and the corpus wall stays under the 10 s law.
#[test]
fn depth_five_embed() {
    let edges = resolved_edges();
    let hit = callables(&edges, "callDepthFive", "Deep5");
    assert_eq!(hit.len(), 1, "one Deep5 edge: {hit:?}");
    assert!(hit[0].2.ends_with("lib.go"), "bound in lib.go: {hit:?}");
}

/// `Ambiguous` embeds two types that each declare `Tied` at depth one. Go
/// rejects the selector, so the tier binds nothing rather than coin-flipping.
#[test]
fn ambiguous_embeds_decline() {
    let edges = resolved_edges();
    let hit = callables(&edges, "callAmbiguous", "Tied");
    assert!(hit.is_empty(), "tie at one depth binds nothing: {hit:?}");
}

/// `Outer` declares its own `Shadowed`, which wins over `Inner`'s.
#[test]
fn own_method_shadows_promoted() {
    let edges = resolved_edges();
    let hit = callables(&edges, "callShadowed", "Shadowed");
    assert_eq!(hit.len(), 1, "one Shadowed edge: {hit:?}");
    assert!(hit[0].2.ends_with("lib.go"), "bound in lib.go: {hit:?}");
}

/// The receiver typed by the one-hop return inference (`o := newOuter()`)
/// reaches the promoted method the same way a parameter does.
#[test]
fn inferred_receiver_embed() {
    let edges = resolved_edges();
    let hit = callables(&edges, "callInferredReceiver", "InnerPing");
    assert_eq!(hit.len(), 1, "one InnerPing edge: {hit:?}");
    assert!(hit[0].2.ends_with("lib.go"), "bound in lib.go: {hit:?}");
}
