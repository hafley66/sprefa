//! Generic product application in a bind and downstream variant queries.

use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

fn rows(closure: &Value, relation: &Value) -> Vec<Vec<String>> {
    let mut rows = closure["closure"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| &row["rel"] == relation)
        .map(|row| {
            row["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|arg| arg["args"][0]["s"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

#[test]
fn generic_bind_evaluates_and_inserts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("fixtures/generic/0_eresult.dl7");
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .args(["compile"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let program: Value = serde_json::from_slice(&compiled.stdout).unwrap();
    assert_eq!(program["diagnostics"], json!([]));

    let directory = root.join(format!("target/generic-bind-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let program_path = directory.join("program.json");
    std::fs::write(&program_path, &compiled.stdout).unwrap();
    let names = &program["program"]["names"];

    for engine in ["ir:dd"] {
        let evaluated = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .args(["eval"])
            .arg(&program_path)
            .env("DL8_ENGINE", engine)
            .output()
            .unwrap();
        assert!(
            evaluated.status.success(),
            "{engine}: {}",
            String::from_utf8_lossy(&evaluated.stderr)
        );
        let closure: Value = serde_json::from_slice(&evaluated.stdout).unwrap();
        assert_eq!(closure["diagnostics"], json!([]), "{engine}");
        assert_eq!(rows(&closure, &names["Got"]), vec![vec!["hi".to_owned()]], "{engine}");
        assert_eq!(
            rows(&closure, &names["Failed"]),
            vec![vec!["boom".to_owned()]],
            "{engine}"
        );
    }

    std::fs::remove_dir_all(directory).unwrap();
}

/// The `(: owner name target index)` rows check emitted, as their four arguments.
fn edge_rows(program: &Value) -> Vec<Vec<Value>> {
    let kernel = json!({"f": "ref", "args": [{"f": "kernel", "args": [{"a": ":"}]}]});
    program["compiler_rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["f"] == "call" && row["args"][0] == kernel)
        .map(|row| row["args"][1].as_array().unwrap().clone())
        .collect()
}

fn label(name: &str) -> Value {
    json!({"f": "const", "args": [{"a": name}]})
}

fn edge_target<'a>(edges: &'a [Vec<Value>], owner: &Value, name: &str) -> &'a Value {
    let matched: Vec<_> = edges
        .iter()
        .filter(|edge| &edge[0] == owner && edge[1] == label(name))
        .collect();
    assert_eq!(matched.len(), 1, "{name}");
    &matched[0][2]
}

#[test]
fn nested_generic_argument_builds_inner_instance_first() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("fixtures/generic/1_nested.dl7");
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .args(["compile"])
        .arg(&fixture)
        .output()
        .unwrap();
    let program: Value = serde_json::from_slice(&compiled.stdout).unwrap();
    assert_eq!(program["diagnostics"], json!([]));
    assert!(compiled.status.success());

    let edges = edge_rows(&program);
    let module = &edges
        .iter()
        .find(|edge| edge[1] == label("Inner"))
        .unwrap()[0];
    let inner = edge_target(&edges, module, "Inner");
    let outer = edge_target(&edges, module, "Outer");
    assert_ne!(inner, outer);
    let next = edge_target(&edges, outer, "next");
    assert_eq!(edge_target(&edges, next, "value"), inner);
    assert_eq!(edge_target(&edges, inner, "item"), &json!({"f": "ref", "args": [{"f": "primitive", "args": [{"a": "str"}]}]}));

    let directory = root.join(format!("target/generic-nested-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let program_path = directory.join("program.json");
    std::fs::write(&program_path, &compiled.stdout).unwrap();
    for engine in ["ir:dd"] {
        let evaluated = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .args(["eval"])
            .arg(&program_path)
            .env("DL8_ENGINE", engine)
            .output()
            .unwrap();
        assert!(evaluated.status.success(), "{engine}: {}", String::from_utf8_lossy(&evaluated.stderr));
        let closure: Value = serde_json::from_slice(&evaluated.stdout).unwrap();
        assert_eq!(closure["diagnostics"], json!([]), "{engine}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}

/// A generic product applied to rule variables, as a goal and as a head
/// value, builds the instance: its `intern` row, its `item` edge and the row.
#[test]
fn variable_application_in_rule_builds_the_instance() {
    use super::_9e_generic_rules::{render_row, ENGINES};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join(format!("target/generic-variable-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let program_path = directory.join("program.json");
    for fixture in ["2_variable_goal", "3_variable_value"] {
        let path = root.join(format!("fixtures/generic/{fixture}.dl7"));
        let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .args(["compile"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(compiled.status.success(), "{fixture}");
        let program: Value = serde_json::from_slice(&compiled.stdout).unwrap();
        assert_eq!(program["diagnostics"], json!([]), "{fixture}");
        std::fs::write(&program_path, &compiled.stdout).unwrap();
        let names = &program["program"]["names"];
        for engine in ENGINES {
            let evaluated = Command::new(env!("CARGO_BIN_EXE_dl8"))
                .arg("eval")
                .arg(&program_path)
                .env("DL8_ENGINE", engine)
                .output()
                .unwrap();
            assert!(evaluated.status.success(), "{fixture} {engine}");
            let closure: Value = serde_json::from_slice(&evaluated.stdout).unwrap();
            assert_eq!(closure["diagnostics"], json!([]), "{fixture} {engine}");
            let mut rows: Vec<String> = closure["closure"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| render_row(row, names))
                .filter(|row| row.contains("Box") || row.starts_with("Names"))
                .collect();
            rows.sort();
            assert_eq!(
                rows,
                vec![
                    "Boxed: const(\"a\"), ref(application(Box, [\"a\"]))",
                    "Names: const(\"a\")",
                    "kernel(:): ref(application(Box, [\"a\"])), const(item), const(\"a\"), const(0)",
                    "kernel(intern): ref(Box), ref(application(tuple, [const(\"a\")])), ref(application(Box, [\"a\"]))",
                ],
                "{fixture} {engine}"
            );
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
