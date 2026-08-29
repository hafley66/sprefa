//! An IMPORTED receiver that seats no def node. `TsModuleIndex::bind` joins an
//! export's identifier span to the def node containing it, and two corpus
//! shapes have none: `export namespace Debug {}` (the members are defs, the
//! namespace is not) and `export const factory: NodeFactory = ...` (a const
//! initialized by a call mints no CallF def). Both fell through to the name
//! match, which answers only while the member name is corpus-unique.
//!
//! Fail-first at HEAD (ef6b47d91), `--resolve` over the five fixtures: every
//! member call on `Debug` and on `factory` binds nothing, the decoy having
//! made both member names ambiguous by name.
//!
//!     EDGE <module> -> makeFactory impl.ts name_resolve
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.

use std::process::Command;

const DIR: &str = "tests/fixtures/ts5_findings/namespace_members";

fn resolved_edges() -> Vec<(String, String, String, String)> {
    let root = env!("CARGO_MANIFEST_DIR");
    let files = ["api.ts", "impl.ts", "barrel.ts", "caller.ts", "decoy.ts"];
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(files.map(|file| format!("{root}/{DIR}/{file}")))
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

/// `Debug.assertKind(kind)` binds the function INSIDE the namespace, reached
/// through the `export *` barrel, and does so on the module plane.
#[test]
fn a_namespace_member_binds_through_the_barrel() {
    let edges = resolved_edges();
    let bound: Vec<&(String, String, String, String)> = edges
        .iter()
        .filter(|(caller, callee, _, _)| caller == "build" && callee == "assertKind")
        .collect();
    assert_eq!(bound.len(), 2, "bare and namespace-qualified: {edges:?}");
    for edge in &bound {
        assert!(
            edge.2.ends_with("impl.ts") && edge.3 == "import_resolve",
            "assertKind must bind in impl.ts on the module plane: {edge:?}"
        );
    }
}

/// `factory.createLiteral(...)` goes through the exported const's DECLARED
/// type, so the target is the interface signature in api.ts, never the const.
#[test]
fn an_exported_const_binds_the_member_on_its_declared_type() {
    let edges = resolved_edges();
    let bound: Vec<&(String, String, String, String)> = edges
        .iter()
        .filter(|(caller, callee, _, _)| caller == "build" && callee == "createLiteral")
        .collect();
    assert_eq!(bound.len(), 2, "bare and namespace-qualified: {edges:?}");
    for edge in &bound {
        assert!(
            edge.2.ends_with("api.ts") && edge.3 == "import_resolve",
            "createLiteral must bind the api.ts signature: {edge:?}"
        );
    }
}

/// The decoy declares both member names, so a name match cannot answer either
/// site: nothing in this fixture binds into decoy.ts.
#[test]
fn the_decoy_never_wins_a_member_site() {
    let edges = resolved_edges();
    assert!(
        !edges
            .iter()
            .any(|(_, _, path, _)| path.ends_with("decoy.ts")),
        "a name match reached the decoy: {edges:?}"
    );
}
