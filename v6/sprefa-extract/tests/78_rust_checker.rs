//! The rust CHECKER tier binds a receiver the parse cannot type.
//!
//! FAIL-PRE-FIX RECEIPT (binary at 02d162f2e, `--features cli` alone, same
//! fixture): the `render` site is `{"record":"unresolved","reason":"inferred",
//! "detail":"render"}` and no `resolved_edge` names it. `made` comes out of
//! `widget::make()`, so its type is written nowhere the parse can read, and two
//! corpus files declare a `render`.
//!
//! The fixture is its own cargo workspace (`[workspace]` with no members), so
//! `cargo metadata` resolves it standalone and it pulls no dependency.

#![cfg(feature = "rust-checker")]

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_findings/checker";

const FILES: &[&str] = &["src/lib.rs", "src/widget.rs", "src/panel.rs"];

fn run(checker: bool) -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call,type".to_string(),
        "--project-root".to_string(),
        DIR.to_string(),
    ];
    if checker {
        args.push("--rust-checker".to_string());
    }
    args.extend(FILES.iter().map(|name| format!("{DIR}/{name}")));
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
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("one json fact per line"))
        .collect()
}

/// (caller_name, callee_path tail, callee_name, kind) for every call edge.
fn call_edges(facts: &[Value]) -> Vec<(String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String)> = facts
        .iter()
        .filter(|fact| fact["record"] == "resolved_edge")
        .map(|fact| {
            let callee_path = fact["callee_path"].as_str().unwrap_or_default();
            (
                fact["caller_name"].as_str().unwrap_or_default().to_string(),
                callee_path
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                fact["callee_name"].as_str().unwrap_or_default().to_string(),
                fact["kind"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn unresolved_reasons(facts: &[Value]) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = facts
        .iter()
        .filter(|fact| fact["record"] == "unresolved")
        .map(|fact| {
            (
                fact["detail"].as_str().unwrap_or_default().to_string(),
                fact["reason"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn the_syntax_leg_alone_drops_the_inferred_receiver() {
    let facts = run(false);
    assert_eq!(
        unresolved_reasons(&facts),
        vec![("render".to_string(), "inferred".to_string())],
        "without the tier the site stays a drop"
    );
}

#[test]
fn the_checker_binds_the_inferred_receiver_to_its_own_file() {
    let facts = run(true);
    let edges = call_edges(&facts);
    assert!(
        edges.contains(&(
            "drive".to_string(),
            "widget.rs".to_string(),
            "render".to_string(),
            "checker_resolve".to_string(),
        )),
        "the checker names Widget::render in widget.rs, got {edges:?}"
    );
    assert!(
        !unresolved_reasons(&facts)
            .iter()
            .any(|(detail, _)| detail == "render"),
        "a bound site emits no drop row"
    );
}

#[test]
fn the_checker_never_names_the_same_named_method_of_another_type() {
    let edges = call_edges(&run(true));
    assert!(
        !edges
            .iter()
            .any(|(_, path, name, _)| path == "panel.rs" && name == "render"),
        "Panel::render shares the name and nothing else, got {edges:?}"
    );
}
