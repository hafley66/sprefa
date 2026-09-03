//! A trait edge points at the file that declares THAT trait. The module
//! plane's trait tables were keyed on the bare trait name, so when two files
//! declare `trait Shape`, `trait_fn_target` returned the first entry's blob
//! with the matched entry's span: an edge into the wrong file. Each caller
//! now binds the declaration in its own file when it has one, else the one
//! its `use` resolves to, else nothing.
//!
//! Fixture: `tests/fixtures/rust_findings/trait_blob/` — `a.rs` and `b.rs`
//! each declare `trait Shape` with a default `area`; each file calls it
//! through `&dyn Shape`. The call in `a.rs` must target `a.rs`'s `area`,
//! the call in `b.rs` must target `b.rs`'s.

use std::process::Command;

use serde_json::Value;

const SRC: &str = "tests/fixtures/rust_findings/trait_blob";

const FILES: &[&str] = &["{SRC}/a.rs", "{SRC}/b.rs"];

fn run() -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call".to_string(),
    ];
    args.extend(FILES.iter().map(|tpl| tpl.replace("{SRC}", SRC)));
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

/// (caller_name, callee file stem) per `resolved_edge`.
fn edges() -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = run()
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                row["caller_name"].as_str().unwrap_or("").to_string(),
                row["callee_path"]
                    .as_str()
                    .unwrap_or("")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn trait_edge_targets_the_callers_own_declaration() {
    let edges = edges();
    assert!(
        edges.contains(&("f".into(), "a.rs".into())),
        "a.rs's call must target a.rs's area, got {edges:?}"
    );
    assert!(
        edges.contains(&("g".into(), "b.rs".into())),
        "b.rs's call must target b.rs's area, got {edges:?}"
    );
    assert_eq!(
        edges.len(),
        2,
        "no other call edges expected, got {edges:?}"
    );
}
