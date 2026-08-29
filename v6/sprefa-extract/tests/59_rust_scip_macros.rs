//! The scip macro post-pass: a call written inside a macro invocation has no
//! parse site, so the per-file `Resolve<CallF>` mints nothing for it. With a
//! rust-analyzer scip index in hand the post-pass binds the edge from the
//! index's exact occurrence, kind `scip_macro`, and one `site` row per minted
//! edge carries the invocation span and which arm bound it (record
//! `macro_site`, the shared shape the mbe lane folds into, `source: "scip"`).

use std::process::Command;

use serde_json::Value;

const FIXTURE: &str = "tests/fixtures/rust_findings/scip_macros";

fn resolve(scip: bool) -> Vec<Value> {
    let mut argv = vec![
        env!("CARGO_BIN_EXE_extract").to_string(),
        "--resolve".to_string(),
        "--family".to_string(),
        "call".to_string(),
    ];
    if scip {
        argv.push("--scip-build".to_string());
        argv.push("--scip-timeout".to_string());
        argv.push("600".to_string());
        argv.push("--project-root".to_string());
        argv.push(FIXTURE.to_string());
    }
    argv.push(format!("{FIXTURE}/src/lib.rs"));
    let out = Command::new(&argv[0]).args(&argv[1..]).output().unwrap();
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect()
}

fn edges(facts: &[Value], kind: &str) -> Vec<(String, String, u32, u32, String)> {
    let mut rows: Vec<(String, String, u32, u32, String)> = facts
        .iter()
        .filter(|fact| fact["record"] == "resolved_edge" && fact["kind"] == kind)
        .map(|fact| {
            (
                fact["caller_name"].as_str().unwrap().to_string(),
                fact["callee_name"].as_str().unwrap().to_string(),
                fact["caller_site_start"].as_u64().unwrap() as u32,
                fact["caller_site_end"].as_u64().unwrap() as u32,
                fact["callee_path"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn macro_sites(facts: &[Value]) -> Vec<(u32, u32, String, String)> {
    let mut rows: Vec<(u32, u32, String, String)> = facts
        .iter()
        .filter(|fact| fact["record"] == "macro_site")
        .map(|fact| {
            (
                fact["span"]["start"].as_u64().unwrap() as u32,
                fact["span"]["end"].as_u64().unwrap() as u32,
                fact["macro_name"].as_str().unwrap().to_string(),
                fact["source"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn macro_calls_mint_scip_macro_edges_inside_the_invocation() {
    let facts = resolve(true);
    let macro_edges = edges(&facts, "scip_macro");
    assert_eq!(macro_edges.len(), 2, "the macro mints two helper calls");
    for (caller, callee, start, end, path) in &macro_edges {
        assert_eq!(caller, "caller");
        assert_eq!(callee, "helper");
        assert!(path.ends_with("src/lib.rs"), "callee path {path}");
    }
    // The invocation span check: both call sites fall inside the same
    // `twice!(...)` range recorded by the macro_site rows.
    let sites = macro_sites(&facts);
    assert_eq!(sites.len(), 2, "one macro_site row per minted edge");
    let (inv_start, inv_end, macro_name, source) = &sites[0];
    assert_eq!(macro_name, "assert_eq");
    assert_eq!(source, "scip");
    for (_, _, start, end, _) in &macro_edges {
        assert!(
            *start >= *inv_start && *end <= *inv_end,
            "call site {start}..{end} must sit inside the invocation {inv_start}..{inv_end}"
        );
    }
}

#[test]
fn an_occurrence_with_a_parse_site_mints_no_duplicate() {
    let facts = resolve(true);
    let direct: Vec<_> = edges(&facts, "name_resolve")
        .into_iter()
        .filter(|(caller, _, _, _, _)| caller == "direct")
        .collect();
    assert_eq!(direct.len(), 1, "the plain call keeps exactly its own edge");
    assert_eq!(edges(&facts, "scip_macro").len(), 2, "no extra mints");
}

#[test]
fn without_a_scip_index_the_pass_is_a_no_op() {
    let facts = resolve(false);
    assert!(edges(&facts, "scip_macro").is_empty(), "no scip, no mints");
    assert!(macro_sites(&facts).is_empty(), "no scip, no macro_site rows");
}
