//! The ts CHECKER tier binds a receiver the parse cannot type, and refuses a
//! name match the compiler knows is a lib global.
//!
//! FAIL-PRE-FIX RECEIPT (binary at 681c9126c, `--features cli` alone, same
//! fixture): the three call rows were
//!   EDGE check -> shadow.ts isNaN name_resolve   (WRONG: the global `isNaN`)
//!   EDGE drive -> pick.ts pick import_resolve
//!   DROP render inferred
//! `chosen` comes out of `pick<T>(items: T[]): T`, whose DECLARED return type is
//! the type parameter, so the one-hop inference reads no corpus type name; two
//! corpus files declare a `render`; and `isNaN` is declared in lib.es5.d.ts,
//! which no name match can see.
//!
//! The tier drives the project's own `typescript`. The fixture is a bare
//! directory with no `node_modules`, so this test pins the compiler the way a
//! monorepo does, through `SPREFA_TS_CHECKER_TYPESCRIPT`.

#![cfg(feature = "ts-checker")]

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/ts_checker";

const FILES: &[&str] = &[
    "src/main.ts",
    "src/widget.ts",
    "src/panel.ts",
    "src/pick.ts",
    "src/shadow.ts",
];

/// A `typescript` the driver can load, machine-local the way the ratchet's
/// corpus roots are. A checkout's `lib/typescript.js` is the built compiler.
fn typescript() -> String {
    if let Ok(pinned) = std::env::var("SPREFA_TS_CHECKER_TYPESCRIPT") {
        return pinned;
    }
    let root = std::env::var("RATCHET_TS_ROOT")
        .unwrap_or_else(|_| "/Users/chrishafley/projects/TypeScript-5.9".to_string());
    let built = PathBuf::from(&root).join("lib/typescript.js");
    assert!(
        built.is_file(),
        "no typescript for the checker tier: set SPREFA_TS_CHECKER_TYPESCRIPT to a \
         typescript.js, or RATCHET_TS_ROOT to a TypeScript checkout (tried {})",
        built.display()
    );
    built.to_string_lossy().into_owned()
}

fn run(checker: bool) -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call,type".to_string(),
        "--project-root".to_string(),
        DIR.to_string(),
    ];
    if checker {
        args.push("--ts-checker".to_string());
    }
    args.extend(FILES.iter().map(|name| format!("{DIR}/{name}")));
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SPREFA_TS_CHECKER_TYPESCRIPT", typescript())
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

/// (owner_name, target_path tail, target_name, resolution_origin) per type edge.
fn type_edges(facts: &[Value]) -> Vec<(String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String)> = facts
        .iter()
        .filter(|fact| fact["record"] == "resolved_type_edge")
        .map(|fact| {
            let target = fact["target_path"].as_str().unwrap_or_default();
            (
                fact["owner_name"].as_str().unwrap_or_default().to_string(),
                target.rsplit('/').next().unwrap_or_default().to_string(),
                fact["target_name"].as_str().unwrap_or_default().to_string(),
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
fn the_syntax_leg_alone_drops_the_generic_receiver() {
    let facts = run(false);
    assert_eq!(
        unresolved_reasons(&facts),
        vec![("render".to_string(), "inferred".to_string())],
        "without the tier the site stays a drop"
    );
    assert!(
        !call_edges(&facts)
            .iter()
            .any(|(_, _, name, _)| name == "render"),
        "no leg binds `render` without the tier"
    );
}

#[test]
fn the_syntax_leg_alone_name_matches_a_lib_global() {
    assert!(
        call_edges(&run(false)).contains(&(
            "check".to_string(),
            "shadow.ts".to_string(),
            "isNaN".to_string(),
            "name_resolve".to_string(),
        )),
        "the name match binds the corpus `isNaN` the call never names"
    );
}

#[test]
fn the_checker_binds_the_generic_receiver_to_its_own_file() {
    let facts = run(true);
    let edges = call_edges(&facts);
    assert!(
        edges.contains(&(
            "drive".to_string(),
            "widget.ts".to_string(),
            "render".to_string(),
            "checker_resolve".to_string(),
        )),
        "the checker names Widget::render in widget.ts, got {edges:?}"
    );
    assert!(
        !unresolved_reasons(&facts)
            .iter()
            .any(|(detail, _)| detail == "render"),
        "a bound site emits no drop row"
    );
}

/// Both `render` calls resolve, and each names ONLY its own receiver's type.
#[test]
fn the_checker_never_names_the_same_named_method_of_another_type() {
    let edges = call_edges(&run(true));
    assert!(
        edges.contains(&(
            "seat".to_string(),
            "panel.ts".to_string(),
            "render".to_string(),
            "checker_resolve".to_string(),
        )),
        "the checker names Panel::render for the Panel receiver, got {edges:?}"
    );
    assert!(
        !edges.iter().any(|(caller, path, name, _)| caller == "drive"
            && path == "panel.ts"
            && name == "render"),
        "Panel::render shares the name and nothing else, got {edges:?}"
    );
}

/// EXTERNAL is knowledge: the compiler placed `isNaN` in lib.es5.d.ts, so no
/// name-match leg may hand the site the corpus definition that shares its name.
#[test]
fn an_external_answer_suppresses_the_name_match() {
    let edges = call_edges(&run(true));
    assert!(
        !edges.iter().any(|(_, _, name, _)| name == "isNaN"),
        "the lib global binds no corpus edge, got {edges:?}"
    );
}

#[test]
fn the_checker_answers_the_type_plane_too() {
    let edges = type_edges(&run(true));
    assert!(
        edges.contains(&(
            "drive".to_string(),
            "widget.ts".to_string(),
            "Widget".to_string(),
            "checker".to_string(),
        )),
        "a param type edge carries resolution_origin checker, got {edges:?}"
    );
}
