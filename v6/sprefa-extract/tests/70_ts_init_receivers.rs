//! The unannotated-const receiver, one hop through the initializer callee's
//! DECLARED return type, cross-file: `const printer = createPrinter()` binds
//! `printer` to `NodePrinter`, a name written in the callee's file and
//! imported nowhere into the caller's, so the type anchors in the callee's
//! file through the module plane. A nested arrow reads the same binding off
//! the lexical chain of covering defs. An initializer with no declared return
//! type stays a drop, never a name-match fallback.
//!
//! Fail-first at HEAD (950a349be), `--resolve` over the two fixtures: the init
//! calls resolve and every member site drops.
//!
//!     EDGE emitInClosure -> run emit.ts name_resolve
//!     EDGE emitInClosure -> createPrinter printers.ts import_resolve
//!     EDGE emitNode -> createPrinter printers.ts import_resolve
//!     EDGE emitText -> createWriter printers.ts import_resolve
//!     EDGE createPrinter -> NodePrinter printers.ts name_resolve
//!     EDGE createWriter -> TextWriter printers.ts name_resolve
//!     DROP inferred writeNode
//!     DROP inferred writeText
//!     DROP inferred writeNode
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.

use std::process::Command;

const DIR: &str = "tests/fixtures/ts5_findings/init_receivers";

fn resolve() -> String {
    let root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .arg(format!("{root}/{DIR}/printers.ts"))
        .arg(format!("{root}/{DIR}/emit.ts"))
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8 wire")
}

/// `(caller_name, callee_name, callee_path)` per resolved edge.
fn resolved_edges() -> Vec<(String, String, String)> {
    resolve()
        .lines()
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).ok()?;
            (row["record"] == "resolved_edge").then(|| {
                (
                    row["caller_name"].as_str().unwrap_or("").to_string(),
                    row["callee_name"].as_str().unwrap_or("").to_string(),
                    row["callee_path"].as_str().unwrap_or("").to_string(),
                )
            })
        })
        .collect()
}

/// `(reason, detail)` per unresolved row.
fn unresolved_rows() -> Vec<(String, String)> {
    resolve()
        .lines()
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).ok()?;
            (row["record"] == "unresolved").then(|| {
                (
                    row["reason"].as_str().unwrap_or("").to_string(),
                    row["detail"].as_str().unwrap_or("").to_string(),
                )
            })
        })
        .collect()
}

/// The one hop: `createPrinter`'s declared `NodePrinter` carries `writeNode`,
/// and the class is anchored in printers.ts, the file that WROTE the name.
#[test]
fn a_cross_file_init_call_binds_the_member_on_its_declared_return_type() {
    let edges = resolved_edges();
    let bound = edges
        .iter()
        .find(|(caller, callee, _)| caller == "emitNode" && callee == "writeNode")
        .expect("const printer = createPrinter(); printer.writeNode() must bind");
    assert!(
        bound.2.ends_with("printers.ts"),
        "writeNode bound outside printers.ts: {edges:?}"
    );
}

/// An initializer with no declared return type binds nothing: the receiver
/// stays inferred and the site drops.
#[test]
fn an_undeclared_return_type_leaves_the_receiver_a_drop() {
    let edges = resolved_edges();
    assert!(
        !edges
            .iter()
            .any(|(caller, callee, _)| caller == "emitText" && callee == "writeText"),
        "createWriter declares no return type: {edges:?}"
    );
    let unresolved = unresolved_rows();
    assert!(
        unresolved
            .iter()
            .any(|(reason, detail)| reason == "inferred" && detail == "writeText"),
        "writeText must drop with reason inferred: {unresolved:?}"
    );
}

/// A nested arrow closes over the outer `const`: the binding is read off the
/// lexical chain of covering defs, so the closure row and its mirror both name
/// `writeNode`.
#[test]
fn a_nested_arrow_reads_the_outer_const_binding() {
    let edges = resolved_edges();
    let writers: Vec<&(String, String, String)> = edges
        .iter()
        .filter(|(caller, callee, _)| callee == "writeNode" && caller != "emitNode")
        .collect();
    assert_eq!(writers.len(), 2, "closure row plus its mirror: {edges:?}");
    assert!(
        writers
            .iter()
            .any(|(caller, _, _)| caller.starts_with("closure@")),
        "the closure row names the frame: {edges:?}"
    );
    assert!(
        writers.iter().any(|(caller, _, _)| caller == "emitInClosure"),
        "the mirror names the enclosing fn: {edges:?}"
    );
}

/// The member bind never displaces the init call's own edge.
#[test]
fn the_initializer_call_keeps_its_own_edge() {
    let edges = resolved_edges();
    assert!(
        edges
            .iter()
            .any(|(caller, callee, _)| caller == "emitNode" && callee == "createPrinter"),
        "all edges: {edges:?}"
    );
}
