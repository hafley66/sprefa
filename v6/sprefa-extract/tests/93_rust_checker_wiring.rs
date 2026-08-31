//! The checker tier's loader config decides how much rust-analyzer will answer.
//!
//! FAIL-PRE-FIX RECEIPT (same fixture, binary built from e26fbb228 with only
//! `sysroot`/`set_test` reverted): the tier logs `walk_ms=3 external=0`, and the
//! three method sites come out as
//! `{"record":"unresolved","reason":"inferred","detail":"replace"|"render"|"label"}`
//! with no `resolved_edge` naming them. `CargoConfig::default()` leaves `sysroot`
//! unset, so the crate graph has no std and rust-analyzer declines every method
//! whose receiver type flows through one; over the rust-analyzer corpus that was
//! 172 of 259 method sites in one file, 2,591 of 84,627 after the fix.
//!
//! The decoys file exists so a corpus-wide name match cannot bind these sites by
//! name alone: every callee here is declared twice, and only the checker's answer
//! picks the right file.
//!
//! The fixture is its own cargo workspace (`[workspace]` with no members), so
//! `cargo metadata` resolves it standalone and it pulls no dependency.

#![cfg(feature = "rust-checker")]

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_checker_wiring";

const FILES: &[&str] = &[
    "src/lib.rs",
    "src/editor.rs",
    "src/widget.rs",
    "src/helpers.rs",
    "src/decoys.rs",
];

fn run() -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call".to_string(),
        "--project-root".to_string(),
        DIR.to_string(),
        "--rust-checker".to_string(),
    ];
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

/// (caller, callee, dst file, resolution origin) for every call edge.
fn edges(facts: &[Value]) -> Vec<(String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String)> = facts
        .iter()
        .filter(|fact| fact["record"] == "resolved_edge")
        .map(|fact| {
            (
                fact["caller_name"].as_str().unwrap_or_default().to_string(),
                fact["callee_name"].as_str().unwrap_or_default().to_string(),
                fact["callee_path"]
                    .as_str()
                    .unwrap_or_default()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                fact["resolution_origin"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn drops(facts: &[Value]) -> Vec<(String, String)> {
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

fn bound(facts: &[Value], caller: &str, callee: &str) -> Option<(String, String)> {
    edges(facts)
        .into_iter()
        .find(|(from, name, _, _)| from == caller && name == callee)
        .map(|(_, _, file, origin)| (file, origin))
}

/// Census class A2: the receiver is a call result the parse types `Inferred`,
/// and the same dst file is already reached under another callee name.
#[test]
fn the_checker_names_a_second_method_of_an_inferred_receivers_file() {
    let facts = run();
    assert_eq!(
        bound(&facts, "drive_a2", "replace"),
        Some(("editor.rs".to_string(), "checker".to_string())),
        "Editor::replace is the answer, not Panel::replace, got {:?}",
        edges(&facts)
    );
    assert!(
        !drops(&facts).iter().any(|(detail, _)| detail == "replace"),
        "a bound site emits no drop row, got {:?}",
        drops(&facts)
    );
}

/// Census class T2: the receiver's type flows through a std container, so the
/// answer exists only once the crate graph carries a sysroot.
#[test]
fn the_checker_names_an_impl_method_behind_a_std_container() {
    let facts = run();
    assert_eq!(
        bound(&facts, "drive_t2", "render"),
        Some(("widget.rs".to_string(), "checker".to_string())),
        "Widget::render is the answer, not Panel::render, got {:?}",
        edges(&facts)
    );
}

/// Census class T1: the same receiver shape answered by a trait DEFAULT body
/// rather than an inherent impl.
#[test]
fn the_checker_names_a_trait_default_body_behind_a_std_container() {
    let facts = run();
    assert_eq!(
        bound(&facts, "drive_t1", "label"),
        Some(("widget.rs".to_string(), "checker".to_string())),
        "Described::label is the answer, not Panel::label, got {:?}",
        edges(&facts)
    );
}

/// `set_test`: a `#[cfg(test)]` body is in the module tree, so its sites are
/// answered like any other.
#[test]
fn the_checker_answers_inside_a_cfg_test_body() {
    let facts = run();
    assert_eq!(
        bound(&facts, "drive_cfg_test", "render"),
        Some(("widget.rs".to_string(), "checker".to_string())),
        "the cfg(test) body's method site is answered, got {:?}",
        edges(&facts)
    );
    assert_eq!(
        bound(&facts, "drive_cfg_test", "helper"),
        Some(("helpers.rs".to_string(), "checker".to_string())),
        "helpers::helper is the answer, not decoys::helper, got {:?}",
        edges(&facts)
    );
}

/// A std method is knowledge, not absence: the tier says `external` and no
/// name-match leg may invent a corpus edge for it.
#[test]
fn a_std_method_stays_an_external_answer() {
    let facts = run();
    let reasons = drops(&facts);
    for callee in ["first", "unwrap", "len"] {
        assert!(
            reasons
                .iter()
                .any(|(detail, reason)| detail == callee && reason == "external"),
            "{callee} is a std method and drops `external`, got {reasons:?}"
        );
    }
}
