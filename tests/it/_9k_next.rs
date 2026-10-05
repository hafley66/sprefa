//! `(next (R ...))` heads through the public compiler on every evaluation engine:
//! the three page 3 examples (plans/v8/2026-10-05-macrotime-rewrite-rules.findings.md),
//! an aggregate inside one wave, negation inside one wave, recursion inside one wave (the page 3
//! walk, reach and slot_start shapes), and the wave limit.

use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

const ENGINES: [&str; 2] = ["ir:sqlite", "ir:dd"];

fn cell(arg: &Value) -> Value {
    arg["args"][0].clone()
}

fn rows(closure: &Value, relation: &Value) -> Value {
    let mut rows = closure["closure"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| &row["rel"] == relation)
        .map(|row| Value::Array(row["args"].as_array().unwrap().iter().map(cell).collect()))
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.to_string());
    Value::Array(rows)
}

fn compile(stem: &str, engine: &str) -> (Vec<u8>, Value) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(root)
        .arg("compile")
        .arg(root.join(format!("fixtures/next/{stem}.dl7")))
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    // A program with diagnostics exits nonzero and still prints them.
    let program: Value = serde_json::from_slice(&compiled.stdout)
        .unwrap_or_else(|e| panic!("{stem} {engine}: {e}; {}", String::from_utf8_lossy(&compiled.stderr)));
    (compiled.stdout, program)
}

/// Compiles `fixtures/next/<stem>.dl7` and evaluates it on every engine (compile and eval on
/// the same engine); the named relations' rows match `expected` exactly.
fn run(stem: &str, expected: Value) {
    for engine in ENGINES {
        let (bytes, program) = compile(stem, engine);
        assert_eq!(program["diagnostics"], json!([]), "{stem} {engine}");
        let directory = std::env::temp_dir().join(format!("dl8-next-{stem}-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let program_path = directory.join("program.json");
        std::fs::write(&program_path, &bytes).unwrap();
        let evaluated = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("eval")
            .arg(&program_path)
            .env("DL8_ENGINE", engine)
            .output()
            .unwrap();
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(evaluated.status.success(), "{stem} {engine}: {}", String::from_utf8_lossy(&evaluated.stderr));
        let closure: Value = serde_json::from_slice(&evaluated.stdout).unwrap();
        assert_eq!(closure["diagnostics"], json!([]), "{stem} {engine}");
        let names = &program["program"]["names"];
        let actual: serde_json::Map<String, Value> = expected
            .as_object()
            .unwrap()
            .keys()
            .map(|name| (name.clone(), rows(&closure, &names[name])))
            .collect();
        assert_eq!(Value::Object(actual), expected, "{stem} {engine}");
    }
}

/// The compile diagnostics' payload functor and phase, on every engine.
fn diagnostics(stem: &str) -> Vec<Value> {
    ENGINES
        .iter()
        .map(|engine| {
            let (_, program) = compile(stem, engine);
            let mut found: Vec<Value> = program["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| json!([d["args"][0]["a"], d["args"][2]["f"], d["args"][2]["args"][0]["a"]]))
                .collect();
            found.insert(0, json!(engine));
            Value::Array(found)
        })
        .collect()
}

#[test]
fn graph_carry() {
    run(
        "0_graph_carry",
        json!({
            "graph_item": [[0, 2, 1], [0, 20, 0]],
            "removed_item": [],
            "added_item": [],
        }),
    );
}

#[test]
fn counter() {
    run(
        "1_counter",
        json!({
            "cur_wave": [[3]],
            "rewrite_pending": [],
            "bound": [[3]],
        }),
    );
}

#[test]
fn origin() {
    run(
        "2_origin",
        json!({
            "graph_item": [[0, 2, 1], [0, 20, 0]],
            "cur_wave": [[2]],
            "origin_claim": [[1, 10, 0], [10, 20, 1]],
            "rewrite_hit": [],
        }),
    );
}

#[test]
fn count_inside_one_wave() {
    run(
        "6_wave_count",
        json!({
            "pile": [[0], [1], [2]],
            "pile_size": [[3]],
        }),
    );
}

#[test]
fn negation_inside_one_wave_is_a_strict_cycle() {
    assert_eq!(
        diagnostics("3_same_wave_negation"),
        vec![
            json!(["ir:sqlite", ["stratify", "strict_dependency_cycle", null]]),
            json!(["ir:dd", ["stratify", "strict_dependency_cycle", null]]),
        ]
    );
}

#[test]
fn recursion_inside_one_wave() {
    run(
        "4_wave_recursion",
        json!({
            "graph_item": [[0, 1], [1, 2]],
            "reach": [[0], [1], [2]],
        }),
    );
}

#[test]
fn walk_inside_one_wave() {
    run(
        "7_wave_walk",
        json!({
            "graph_item": [[0, 1, 0], [1, 2, 0], [1, 6, 1], [3, 4, 0], [9, 5, 0]],
            "walk": [[0], [1], [2], [6]],
            "hit": [],
        }),
    );
}

#[test]
fn reach_inside_one_wave() {
    run(
        "8_wave_reach",
        json!({
            "graph_item": [[0, 1], [1, 5], [1, 7]],
            "reach": [[0], [1], [5], [7]],
            "below": [],
        }),
    );
}

#[test]
fn slot_start_inside_one_wave() {
    run(
        "9_wave_slot",
        json!({
            "item": [[0, 0, 3], [0, 1, 2], [0, 2, 2]],
            "slot_start": [[0, 0, 0], [0, 1, 3], [0, 2, 5]],
            "growing": [],
        }),
    );
}

#[test]
fn wave_limit() {
    assert_eq!(
        diagnostics("5_wave_limit"),
        vec![
            json!(["ir:sqlite", ["ir_eval", "wave_limit", null]]),
            json!(["ir:dd", ["ir_eval", "wave_limit", null]]),
        ]
    );
}
