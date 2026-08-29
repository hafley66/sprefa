//! An interface-typed receiver, two shapes the walker could not read. (1)
//! DECLARATION MERGING: `interface Session` written twice in one file is ONE
//! type, and the module plane names whichever block exported the name last, so
//! the members of the other block were unreachable. (2) PROPERTY SIGNATURES:
//! only class `PropertyDefinition`s reached `fields`, so `holder.session.f()`
//! had no field type to hop through, and a base's property none at all.
//!
//! The oracle's coordinate for an interface dispatch is the SIGNATURE in the
//! declaring file (`interface_member_defs`, ts.rs), never an implementer;
//! impls.ts exists to make both member names ambiguous by name so a name match
//! cannot answer, and to hold the implementer shapes a fan-out would target.
//!
//! Fail-first at HEAD (ef6b47d91), `--resolve` over the four fixtures:
//!
//!     EDGE runSession -> stop api.ts name_resolve
//!
//! Only the LAST merged block's member bound, and only on the directly typed
//! receiver. Expected values are hand-derived from the fixtures.

use std::process::Command;

const DIR: &str = "tests/fixtures/ts5_findings/iface_receiver";

fn resolved_edges() -> Vec<(String, String, String)> {
    let root = env!("CARGO_MANIFEST_DIR");
    let files = ["api.ts", "impls.ts", "use.ts"];
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
                )
            })
        })
        .collect()
}

fn binds(edges: &[(String, String, String)], caller: &str, callee: &str) -> usize {
    edges
        .iter()
        .filter(|(c, m, path)| c == caller && m == callee && path.ends_with("api.ts"))
        .count()
}

/// Both merged blocks of `interface Session` carry members of one type, so a
/// directly typed receiver reaches the first block's `start` too.
#[test]
fn a_merged_interface_block_keeps_its_members() {
    let edges = resolved_edges();
    assert_eq!(binds(&edges, "runSession", "start"), 1, "{edges:?}");
    assert_eq!(binds(&edges, "runSession", "stop"), 1, "{edges:?}");
}

/// `holder.session.f()`: the interface PROPERTY signature carries the field's
/// type, and the member binds on it, across both merged blocks.
#[test]
fn an_interface_property_signature_carries_the_field_hop() {
    let edges = resolved_edges();
    assert_eq!(binds(&edges, "runHolder", "start"), 1, "{edges:?}");
    assert_eq!(binds(&edges, "runHolder", "stop"), 1, "{edges:?}");
}

/// The signature is the answer: a class or object-literal implementer of the
/// same interface is never the target of a dispatch site.
#[test]
fn an_implementer_is_never_the_dispatch_target() {
    let edges = resolved_edges();
    assert!(
        !edges
            .iter()
            .any(|(caller, _, path)| caller.starts_with("run") && path.ends_with("impls.ts")),
        "a dispatch site reached an implementer: {edges:?}"
    );
}
