//! Sum variant readback through the public compiler and all evaluation engines.

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
fn sum_variants_match_and_retract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("fixtures/sum/0_variant_match.dl7");
    let expected: Value = serde_json::from_slice(
        &std::fs::read(root.join("fixtures/sum/0_variant_match.expected.json")).unwrap(),
    )
    .unwrap();
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
    let directory = root.join(format!("target/sum-variant-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let program_path = directory.join("program.json");
    let frontier_path = directory.join("frontiers.json");
    std::fs::write(&program_path, &compiled.stdout).unwrap();
    std::fs::write(
        &frontier_path,
        serde_json::to_vec(&json!([{"changes":[{"rel":"Raw","args":[
            {"f":"const","args":[{"s":"a"}]},
            {"f":"const","args":[{"s":"v"}]}
        ],"w":-1}]}]))
        .unwrap(),
    )
    .unwrap();
    let names = &program["program"]["names"];
    for engine in ["ir:sqlite", "ir:dd"] {
        let initial = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("eval")
            .arg(&program_path)
            .env("DL8_ENGINE", engine)
            .output()
            .unwrap();
        assert!(
            initial.status.success(),
            "{engine}: {}",
            String::from_utf8_lossy(&initial.stderr)
        );
        let initial: Value = serde_json::from_slice(&initial.stdout).unwrap();
        assert_eq!(initial["diagnostics"], json!([]), "{engine}");
        for name in ["Loading", "Value"] {
            assert_eq!(
                json!(rows(&initial, &names[name])),
                expected["initial"][name],
                "{engine} {name}"
            );
        }
        let run = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("run")
            .arg(&program_path)
            .arg("--frontiers")
            .arg(&frontier_path)
            .env("DL8_ENGINE", engine)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{engine}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let run: Value = serde_json::from_slice(&run.stdout).unwrap();
        assert_eq!(run["diagnostics"], json!([]), "{engine}");
        let after = &run["frontiers"][0]["closure"];
        assert_eq!(after["diagnostics"], json!([]), "{engine}");
        for name in ["Loading", "Value"] {
            assert_eq!(
                json!(rows(after, &names[name])),
                expected["after_raw_retract"][name],
                "{engine} {name}"
            );
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn imported_sum_variants_construct_and_match_on_all_engines() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("fixtures/sum/2_module_variant.dl7");
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(root)
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
    let directory = std::env::temp_dir().join(format!("dl8-module-sum-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let program_path = directory.join("program.json");
    std::fs::write(&program_path, &compiled.stdout).unwrap();

    for engine in ["ir:sqlite", "ir:dd"] {
        let evaluated = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("eval")
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
        assert_eq!(
            json!(rows(&closure, &program["program"]["names"]["Matched"])),
            json!([["hello"]]),
            "{engine}"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn doc_layout_answers_inside_comptime() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("fixtures/comptime_effect/4_doc_layout.dl7");
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(root)
        .args(["compile"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let compiled: Value = serde_json::from_slice(&compiled.stdout).unwrap();
    assert_eq!(compiled["diagnostics"], json!([]));
    let rendered = &compiled["program"]["names"]["rendered"];
    let rows: Vec<_> = compiled["compiler_rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| &row["args"][0] == rendered)
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]["args"][1],
        json!([{"f":"const","args":[{"s":"hello"}]}])
    );
}

#[test]
fn sum_variant_catch_all_and_coexistence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join(format!("target/sum-variant-cases-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for stem in ["1_catch_all", "2_coexisting"] {
        let fixture = root.join(format!("fixtures/sum/{stem}.dl7"));
        let expected: Value = serde_json::from_slice(
            &std::fs::read(root.join(format!("fixtures/sum/{stem}.expected.json"))).unwrap(),
        )
        .unwrap();
        let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("compile")
            .arg(&fixture)
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{stem}: {}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let program: Value = serde_json::from_slice(&compiled.stdout).unwrap();
        assert_eq!(program["diagnostics"], json!([]), "{stem}");
        let program_path = directory.join(format!("{stem}.json"));
        std::fs::write(&program_path, &compiled.stdout).unwrap();
        let names = &program["program"]["names"];
        for engine in ["ir:sqlite", "ir:dd"] {
            let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
                .arg("eval")
                .arg(&program_path)
                .env("DL8_ENGINE", engine)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{stem} {engine}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let closure: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(closure["diagnostics"], json!([]), "{stem} {engine}");
            for (name, expected_rows) in expected["rows"].as_object().unwrap() {
                assert_eq!(
                    json!(rows(&closure, &names[name])),
                    *expected_rows,
                    "{stem} {engine} {name}"
                );
            }
            for (name, count) in expected["counts"].as_object().unwrap() {
                let actual = closure["closure"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|row| row["rel"] == names[name])
                    .count();
                assert_eq!(json!(actual), *count, "{stem} {engine} {name}");
            }
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
