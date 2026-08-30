//! Trait dispatch for the rust arm, classes 12/4/6/6b/8 of the receiver
//! census (`plans/extract-crawl-2026-08-29/rust.REPORT.md` 18.2):
//! a `T::f()` whose T is a corpus trait binds the trait's fn def, or the one
//! impl fn that defines it; a type whose impl leaves a trait default fn
//! unbound calls through to the trait's default body; a receiver whose type
//! IS a corpus trait (`dyn T`, a bound generic param) binds the trait's fn
//! def; a 0-impl `T::f()` provided by a trait default body binds the trait.
//!
//! Fixtures: `tests/fixtures/rust_findings/traits/fixture/src/`.

use std::process::Command;

use serde_json::Value;

const SRC: &str = "tests/fixtures/rust_findings/traits/fixture/src";

const FILES: &[&str] = &[
    "{SRC}/lib.rs",
    "{SRC}/traits.rs",
    "{SRC}/dog.rs",
    "{SRC}/robot.rs",
    "{SRC}/users.rs",
    "{SRC}/ext.rs",
];

fn run() -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call,type".to_string(),
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

fn text(row: &Value, key: &str) -> String {
    row[key].as_str().unwrap_or("").to_string()
}

/// (caller_name, callee_name, callee file stem) per `resolved_edge`.
fn edges() -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = run()
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                text(row, "caller_name"),
                text(row, "callee_name"),
                text(row, "callee_path")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".rs")
                    .to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn binds(caller: &str, callee: &str, stem: &str) -> bool {
    edges()
        .iter()
        .any(|(c, f, s)| c == caller && f == callee && s == stem)
}

/// (detail, reason) per unresolved call row.
fn drops() -> Vec<(String, String)> {
    run()
        .iter()
        .filter(|row| row["record"] == "unresolved" && row["family"] == "call")
        .map(|row| (text(row, "detail"), text(row, "reason")))
        .collect()
}

/// Class 11: `mem::take`'s prefix is a `use` binding to an external module,
/// so the drop reads `external`, never `ambiguous`.
#[test]
fn external_module_qualified_prefix_drops_external() {
    assert!(
        drops()
            .iter()
            .any(|(detail, reason)| detail == "mem::take" && reason == "external"),
        "{:?}",
        drops()
    );
}

/// Class 12: `Talk::level()` names a corpus trait; the call binds the
/// trait's own fn def.
#[test]
fn trait_assoc_call_binds_the_trait_fn_def() {
    assert!(binds("bare_trait_call", "level", "traits"), "{:?}", edges());
}

/// Class 12, impl first: `Robot::helper()` has exactly one impl defining the
/// fn, so the impl fn binds (a pre-existing pin against the new fallbacks
/// stealing a bindable impl pair).
#[test]
fn impl_fn_beats_the_trait_fallback() {
    assert!(binds("robot_volume", "helper", "robot"), "{:?}", edges());
}

/// Class 4: `d.greet()` with Dog's impl of Speak not overriding greet binds
/// the trait's default body.
#[test]
fn trait_default_body_binds_the_unoverridden_method() {
    assert!(binds("default_call", "greet", "traits"), "{:?}", edges());
}

/// Class 6: the receiver's type is the corpus trait `dyn Talk`; the call
/// binds the trait's fn def, named via the call facet's def for the bare
/// signature.
#[test]
fn dyn_trait_receiver_binds_the_trait_fn_def() {
    assert!(binds("dyn_call", "chat", "traits"), "{:?}", edges());
}

/// Class 6b: the receiver is a generic param whose bound names the trait.
#[test]
fn bound_generic_receiver_binds_the_trait_fn_def() {
    assert!(binds("generic_call", "chat", "traits"), "{:?}", edges());
}

/// Class 8, zero impls: `Dog::helper()` has no impl of (Dog, helper); the
/// trait-provided assoc fn binds the trait's def.
#[test]
fn zero_impl_assoc_call_binds_the_trait_default() {
    assert!(
        binds("trait_assoc_call", "helper", "traits"),
        "{:?}",
        edges()
    );
}
