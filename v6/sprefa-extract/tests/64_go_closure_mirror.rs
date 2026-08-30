//! The go closure-caller mirror (the rust arm's kink 3, mirrored): a call
//! whose caller is a Lambda def (`closure@<n>`) gets ONE extra edge onto the
//! innermost NAMED enclosing def. Nested closures mirror to the named fn;
//! package-level func literals mint no def and mirror nothing.

use std::collections::BTreeSet;
use std::process::Command;

const FIXTURE: &str = "tests/fixtures/go_findings/closure_mirror/closure_mirror.go";

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
/// same-site, same-callee row whose caller is named.
#[test]
fn one_mirror_edge_per_closure_caller_edge() {
    let edges = resolved_edges();
    let closure_edges: Vec<_> = edges
        .iter()
        .filter(|(caller, _, _)| caller.starts_with("closure@"))
        .collect();
    assert_eq!(closure_edges.len(), 3, "all edges: {edges:?}");
    for (caller, callee, site) in &closure_edges {
        let mirrors = edges
            .iter()
            .filter(|(m_caller, m_callee, m_site)| {
                m_callee == callee && m_site == site && !m_caller.starts_with("closure@")
            })
            .count();
        assert_eq!(mirrors, 1, "closure edge {caller} {callee} @{site} among {edges:?}");
    }
    assert_eq!(edges.len(), 6, "3 primaries + 3 mirrors: {edges:?}");
}

/// The nested-closure call mirrors to the innermost NAMED def (`outer`), never
/// to the outer closure.
#[test]
fn nested_closures_mirror_to_the_named_fn() {
    let edges = resolved_edges();
    let mirror_callers: BTreeSet<&str> = edges
        .iter()
        .filter(|(caller, _, _)| !caller.starts_with("closure@") && caller != "helper" && caller != "wrap")
        .map(|(caller, _, _)| caller.as_str())
        .collect();
    assert_eq!(
        mirror_callers,
        BTreeSet::from(["outer"]),
        "all edges: {edges:?}"
    );
}

/// The package-level literal's body mints no def: its `helper()` site emits no
/// row at all, primary or mirror.
#[test]
fn a_package_level_literal_mirrors_nothing() {
    let edges = resolved_edges();
    // `helper` appears only at the two in-fn sites; the pkg-level one is
    // dropped for want of a caller def.
    let helper_sites: Vec<u64> = edges
        .iter()
        .filter(|(_, callee, _)| callee == "helper")
        .map(|(_, _, site)| *site)
        .collect();
    assert_eq!(helper_sites.len(), 4, "2 primaries + 2 mirrors: {edges:?}");
    // 3 closure-caller edges, all inside `outer`; the pkg-level site sits at a
    // byte offset before `func helper`, so any leak would show as a 5th helper
    // row or a mirror caller that is not `outer`.
    assert!(edges.iter().all(|(caller, _, _)| caller != "pkgLevel"));
}
