//! The ts closure-caller mirror (the rust and go arms, mirrored): a call whose
//! caller is a Lambda def (`closure@<n>`) gets ONE extra edge onto the
//! innermost NAMED enclosing def, which is the walk the tsc oracle does
//! (`plans/extract-bench-2026-08-29/oracle_ts.mjs` `enclosingName`, falling
//! back to `<module>`). Nested anonymous arrows mirror to the named fn; a
//! module-level arrow mirrors to `<module>`.
//!
//! Fail-first at HEAD (950a349be), `--resolve` over the fixture: six rows, no
//! mirror among them.
//!
//!     EDGE <module> -> run site 661
//!     EDGE closure@591 -> helper site 603
//!     EDGE closure@591 -> run site 618
//!     EDGE closure@622 -> wrap site 636
//!     EDGE closure@665 -> helper site 675
//!     EDGE outer -> run site 587
//!
//! Expected values are hand-derived from the fixture, never copied from the
//! extractor's output.

use std::collections::BTreeSet;
use std::process::Command;

const FIXTURE: &str = "tests/fixtures/ts5_findings/closure_mirror/closure_mirror.ts";

/// `(caller_name, callee_name, caller_site_start)` per resolved edge.
fn resolved_edges() -> Vec<(String, String, u64)> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .arg(env!("CARGO_MANIFEST_DIR").to_owned() + "/" + FIXTURE)
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
                    row["caller_site_start"].as_u64().unwrap_or(0),
                )
            })
        })
        .collect()
}

/// Exactly one mirror per closure-caller edge: for each closure-caller row a
/// same-site, same-callee row whose caller is named. The closure row stays; it
/// names the frame.
#[test]
fn one_mirror_edge_per_closure_caller_edge() {
    let edges = resolved_edges();
    let closure_edges: Vec<_> = edges
        .iter()
        .filter(|(caller, _, _)| caller.starts_with("closure@"))
        .collect();
    assert_eq!(closure_edges.len(), 4, "all edges: {edges:?}");
    for (caller, callee, site) in &closure_edges {
        let mirrors = edges
            .iter()
            .filter(|(m_caller, m_callee, m_site)| {
                m_callee == callee && m_site == site && !m_caller.starts_with("closure@")
            })
            .count();
        assert_eq!(
            mirrors, 1,
            "closure edge {caller} {callee} @{site} among {edges:?}"
        );
    }
    assert_eq!(edges.len(), 10, "6 primaries + 4 mirrors: {edges:?}");
}

/// The nested-arrow call mirrors to the innermost NAMED def (`outer`), never to
/// the enclosing arrow: `wrap()` sits two arrows deep and still names `outer`.
#[test]
fn nested_arrows_mirror_to_the_named_fn() {
    let edges = resolved_edges();
    let wrap_callers: BTreeSet<&str> = edges
        .iter()
        .filter(|(_, callee, _)| callee == "wrap")
        .map(|(caller, _, _)| caller.as_str())
        .collect();
    assert_eq!(
        wrap_callers,
        BTreeSet::from(["closure@622", "outer"]),
        "all edges: {edges:?}"
    );
}

/// A module-level arrow has no named fn above it: the mirror names `<module>`,
/// the same fallback the oracle takes.
#[test]
fn a_module_level_arrow_mirrors_to_the_module() {
    let edges = resolved_edges();
    let helper_callers: BTreeSet<&str> = edges
        .iter()
        .filter(|(_, callee, _)| callee == "helper")
        .map(|(caller, _, _)| caller.as_str())
        .collect();
    assert_eq!(
        helper_callers,
        BTreeSet::from(["<module>", "closure@591", "closure@665", "outer"]),
        "all edges: {edges:?}"
    );
}

/// A named caller mirrors nothing: `outer`'s own `run(...)` site keeps one row.
#[test]
fn a_named_caller_mirrors_nothing() {
    let edges = resolved_edges();
    let at_587: Vec<_> = edges.iter().filter(|(_, _, site)| *site == 587).collect();
    assert_eq!(at_587.len(), 1, "one row at the named-caller site: {edges:?}");
    assert_eq!(at_587[0].0, "outer", "all edges: {edges:?}");
}
