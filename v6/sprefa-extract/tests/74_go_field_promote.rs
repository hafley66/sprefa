//! A selector chain whose FIELD hop names a field the struct PROMOTES through
//! an embedded base (`outer.Part` where `Outer` embeds `Inner` and `Inner`
//! owns `Part`). The replay's Field hop must walk the embeds the way Go's
//! field promotion does: shallowest embed wins, a tie at one depth binds
//! nothing.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.
//!
//! TEST HEADER, HEAD failure before the fix
//! (`cargo test --release --features cli --test 74_go_field_promote`, 1 of 1):
//!   promoted_field_hop               FAILED  one Ring edge to base.go: []

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
    let mut paths = walk(&fixture("go_field_promote"));
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

#[test]
fn promoted_field_hop() {
    let edges = resolved_edges();
    let hit: Vec<_> = edges
        .iter()
        .filter(|e| e.0 == "UseOuter" && e.1 == "Ring" && e.2.ends_with("base.go"))
        .collect();
    assert_eq!(hit.len(), 1, "one Ring edge to base.go: {hit:?}");
}
