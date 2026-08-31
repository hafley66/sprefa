//! A range whose operand's type is a CALL's written slice result (`for _, it
//! := range sh.Items()`), directly or through a variable the call bound. The
//! chain replay must read the written `[]*T` result as (element, collection)
//! and let the `Elem` hop name the element, so the call on the range variable
//! binds on the element type.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.
//!
//! TEST HEADER, HEAD failure before the fix
//! (`cargo test --release --features cli --test 73_go_range_elem`, 2 of 2):
//!   range_over_call_result            FAILED  one Tag edge: []
//!   range_over_inferred_slice_var     FAILED  one Tag edge: []

use std::process::Command;

fn fixture(rel: &str) -> String {
    format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"))
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

fn resolved_edges() -> Vec<(String, String, String, String)> {
    let mut paths = walk(&fixture("go_range_elem"));
    paths.sort();
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(&paths)
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

/// One edge from `caller` to `callee`, landing in the file `file` names.
fn one_edge(edges: &[(String, String, String, String)], caller: &str, callee: &str, file: &str) {
    let hit: Vec<_> = edges
        .iter()
        .filter(|e| e.0 == caller && e.1 == callee)
        .collect();
    assert_eq!(hit.len(), 1, "one {callee} edge: {hit:?}");
    assert!(hit[0].2.ends_with(file), "bound in {file}: {hit:?}");
}

#[test]
fn range_over_call_result() {
    let edges = resolved_edges();
    one_edge(&edges, "RangeCall", "Items", "store.go");
    one_edge(&edges, "RangeCall", "Tag", "store.go");
}

#[test]
fn range_over_inferred_slice_var() {
    let edges = resolved_edges();
    one_edge(&edges, "RangeInferred", "Items", "store.go");
    one_edge(&edges, "RangeInferred", "Tag", "store.go");
}
