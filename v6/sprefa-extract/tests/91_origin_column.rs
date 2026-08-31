//! `resolution_origin`: every `resolved_edge` names WHICH resolver leg answered
//! it, as a closed enum (`src/types.rs`, `ResolutionOrigin`).
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): before the column, a
//! `resolved_edge` row carried `kind` and nothing else about its origin, so
//! every assertion below read `null` for `resolution_origin`. `kind` cannot
//! stand in: `name_resolve` is what the same-file leg, the corpus-unique leg,
//! the receiver leg and the python dynamic-shape legs ALL emit, so a leg that
//! starts over-answering moves no observable field until precision drops.
//!
//! One fixture per language, each picked so exactly one leg can answer:
//! ts through a barrel import (module plane), go through a file's own dot
//! import (module plane), rust a bare same-file call (same file), python a
//! decorator rebind (decorator).

use std::process::Command;

use serde_json::Value;

/// The closed vocabulary, as `ResolutionOrigin::as_str` spells it. A row
/// outside it means a leg minted a string instead of a variant.
const ORIGINS: &[&str] = &[
    "same_file",
    "corpus_unique",
    "module_plane",
    "checker",
    "alias_chain",
    "param",
    "receiver",
    "self_type",
    "iface_impl",
    "decorator",
    "subscript",
    "return_call",
    "scip",
    "unresolved",
];

fn run(args: &[&str]) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("--resolve")
        .args(args)
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

fn call_edges(args: &[&str]) -> Vec<Value> {
    run(args)
        .into_iter()
        .filter(|row| row["record"] == "resolved_edge")
        .collect()
}

/// The origins on every edge from `caller` to `callee`, sorted and deduped, so
/// a failure prints what the leg actually answered. `caller` is `None` for a
/// module-level site, whose caller def carries no name.
fn origins(edges: &[Value], caller: Option<&str>, callee: &str) -> Vec<String> {
    let mut found: Vec<String> = edges
        .iter()
        .filter(|row| row["caller_name"].as_str() == caller && row["callee_name"] == callee)
        .map(|row| {
            row["resolution_origin"]
                .as_str()
                .unwrap_or("<absent>")
                .to_string()
        })
        .collect();
    found.sort();
    found.dedup();
    assert!(
        !found.is_empty(),
        "no {caller:?} -> {callee} edge at all; fixture drifted: {edges:#?}"
    );
    found
}

const TS_DIR: &str = "tests/fixtures/ts5_findings/module_plane";
const GO_DIR: &str = "tests/fixtures/go_modules";
const RUST_FIXTURE: &str = "tests/fixtures/origin/rust_same_file.rs";
const PY_FIXTURE: &str = "tests/fixtures/py_findings/decorators/main.py";

/// `run` calls `normalize` through `./index.js`, an `export *` barrel: the ts
/// module plane binds it, and the corpus-unique leg never sees the name.
#[test]
fn ts_barrel_import_edge_says_module_plane() {
    let edges = call_edges(&[
        "--family",
        "call",
        &format!("{TS_DIR}/barrel_consumer.ts"),
        &format!("{TS_DIR}/index.ts"),
        &format!("{TS_DIR}/helpers.ts"),
        &format!("{TS_DIR}/widgets.ts"),
        &format!("{TS_DIR}/other.ts"),
    ]);
    assert_eq!(
        origins(&edges, Some("run"), "normalize"),
        ["module_plane"]
    );
}

/// `UseDot` calls a bare `Widget()`: two corpus packages export the name and
/// main's own package declares none, so only main.go's dot import answers.
#[test]
fn go_dot_import_edge_says_module_plane() {
    let files: Vec<String> = [
        "module_a/pkgutil2/widget.go",
        "module_a/pkgutil3/widget2.go",
        "module_a/blankpkg/blank.go",
        "module_a/vendorlike/yaml.v3/yaml.go",
        "module_a/main.go",
        "module_a/shadowpkg/shadow.go",
    ]
    .iter()
    .map(|name| format!("{GO_DIR}/{name}"))
    .collect();
    let mut args = vec!["--family", "call"];
    args.extend(files.iter().map(String::as_str));
    let edges = call_edges(&args);
    assert_eq!(
        origins(&edges, Some("UseDot"), "Widget"),
        ["module_plane"]
    );
}

/// A bare call to a def in the same file: the same-file leg answers before the
/// corpus-wide name match is allowed to guess.
#[test]
fn rust_same_file_edge_says_same_file() {
    let edges = call_edges(&["--family", "call", RUST_FIXTURE, RUST_FIXTURE]);
    assert_eq!(origins(&edges, Some("run"), "helper"), ["same_file"]);
}

/// `func()` resolves to `wrapper`, and only the decorator leg knows that: the
/// name `func` matches a corpus def of its own.
#[test]
fn python_decorator_rebind_edge_says_decorator() {
    let edges = call_edges(&["--family", "call", PY_FIXTURE, PY_FIXTURE]);
    assert_eq!(origins(&edges, None, "wrapper"), ["decorator"]);
}

/// Nothing is untagged and nothing is stringly-typed: every emitted row's
/// origin is one of the closed variants.
#[test]
fn every_edge_carries_an_origin_from_the_closed_enum() {
    let edges = call_edges(&["--family", "call", PY_FIXTURE, PY_FIXTURE, RUST_FIXTURE]);
    assert!(!edges.is_empty(), "fixtures emitted no edges");
    for row in &edges {
        let origin = row["resolution_origin"].as_str().unwrap_or("<absent>");
        assert!(
            ORIGINS.contains(&origin),
            "origin `{origin}` is outside the closed enum: {row}"
        );
    }
}
