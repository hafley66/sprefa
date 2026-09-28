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

    for engine in ["rust", "ir:dd", "ir:sqlite"] {
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
