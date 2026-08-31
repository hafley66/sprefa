//! The type a `x := pkg.F()` define binds, when `F`'s declaring file writes its
//! result QUALIFIED through its own import (`*types.Widget`), and the caller's
//! qualifier for the declaring package differs from the declaring file's
//! qualifier for the result's package. The bound name must resolve through the
//! DECLARING file's imports to (result package dir, bare name), then through
//! the caller's imports back to a name the receiver legs consume — never as a
//! double-qualified string whose first dot names the wrong package.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.
//!
//! TEST HEADER, HEAD failure before the fix
//! (`cargo test --release --features cli --test 72_go_bound_qualify`, 2 of 2):
//!   define_binds_declared_result      FAILED  one Ping edge: []
//!   multi_value_define_binds_result   FAILED  one Ping edge: []

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
    let mut paths = walk(&fixture("go_bound_type"));
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
fn define_binds_declared_result() {
    let edges = resolved_edges();
    one_edge(&edges, "Call", "NewWidget", "callee.go");
    one_edge(&edges, "Call", "Ping", "types.go");
}

#[test]
fn multi_value_define_binds_result() {
    let edges = resolved_edges();
    one_edge(&edges, "CallPair", "GivePair", "callee.go");
    one_edge(&edges, "CallPair", "Ping", "types.go");
}
