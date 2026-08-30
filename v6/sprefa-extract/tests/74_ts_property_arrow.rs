//! Property-named lambda callers: an arrow that is an object-literal
//! property's value takes the property as its def name, so its call sites name
//! the property as their caller, exactly how the tsc oracle
//! (`plans/extract-bench-2026-08-29/oracle_ts.mjs` `enclosingName`, the
//! `PropertyAssignment` arm) names the enclosing callable. Before this leg the
//! site's caller was the anonymous lambda mirrored onto `<module>`.
//!
//! Fail-first at 18cdeff8f (the agreed-and-missed measure commit), `--resolve`
//! over the fixture: the arrow site named `<module>`.
//!
//!     EDGE <module> -> leaf site 306
//!
//! Expected values are hand-derived from the fixture, never copied from the
//! extractor's output.

use std::collections::BTreeSet;
use std::process::Command;

const FIXTURE: &str = "tests/fixtures/ts5_findings/property_arrow/property_arrow.ts";

/// `(caller_name, callee_name, caller_site_start)` per resolved edge.
fn resolved_edges() -> BTreeSet<(String, String, u64)> {
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

#[test]
fn the_arrow_site_names_the_property() {
    let edges = resolved_edges();
    assert!(
        edges.contains(&("getAllCodeActions".into(), "leaf".into(), 306)),
        "edges: {edges:?}"
    );
    let closure_callers: Vec<_> = edges
        .iter()
        .filter(|(caller, _, _)| caller.starts_with("closure@") || caller == "<module>")
        .collect();
    assert!(
        closure_callers.is_empty(),
        "no anonymous caller may shadow the property name: {closure_callers:?}"
    );
}
