//! `effect` rows for served relations, through the real binary. Each
//! `fixtures/host_effect/*.dl7` compiles to a checked runtime program, whose
//! `program` object goes straight into `dl8 eval --serve <name>`; the closure
//! must match the sibling `*.expected.json`. The fixture path is replaced by
//! `<fixture>` so the expected files carry no absolute path.
//!
//! The served set is a runtime input, so a program run without `--serve` writes
//! no `effect` row at all; `4_unserved` pins that, and with it every oracle case.
//!
//! `dl8 compile` prints `program.names`, the module-level `:/4` binds of
//! `root_graph`, which is what `--serve` resolves against. No test rebuilds it.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Which relations each fixture asks the outside to settle.
pub const SERVED: [(&str, &str); 5] = [
    ("0_pending", "fetch_json"),
    ("1_settled", "fetch_json"),
    ("2_source", "tick"),
    ("3_loading", "fetch_json"),
    ("4_unserved", ""),
];

/// The fixture path is machine-specific; the expected files spell it
/// `<fixture>`.
fn normalize(value: &Value, path: &str) -> Value {
    match value {
        Value::String(s) => Value::String(s.replace(path, "<fixture>")),
        Value::Array(items) => Value::Array(items.iter().map(|v| normalize(v, path)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, v)| (key.clone(), normalize(v, path)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn fixture_path(stem: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/host_effect")
        .join(format!("{stem}.dl7"))
}

fn run(binary: &str, args: &[&str]) -> Result<(Value, i32), String> {
    let output = Command::new(binary)
        .args(args)
        .output()
        .map_err(|e| format!("spawn: {e}"))?;
    let value = serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "no JSON on stdout ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    Ok((value, output.status.code().unwrap_or(-1)))
}

/// Compile one fixture and keep its whole stdout; `dl8 eval` reads the
/// `program` key out of it.
fn transport(binary: &str, source: &Path, stem: &str) -> Result<PathBuf, String> {
    let output = Command::new(binary)
        .args(["compile", &source.to_string_lossy()])
        .output()
        .map_err(|e| format!("{stem}: compile spawn {e}"))?;
    serde_json::from_slice::<Value>(&output.stdout).map_err(|e| {
        format!(
            "{stem}: compile no JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let path = std::env::temp_dir().join(format!("dl8-effect-{}-{stem}.json", std::process::id()));
    std::fs::write(&path, &output.stdout).unwrap();
    Ok(path)
}

/// Compile one fixture, evaluate it with its served set, and compare the
/// closure and diagnostics to the sibling expected file.
fn check_fixture(binary: &str, stem: &str, served: &str) -> Result<(), String> {
    let source = fixture_path(stem);
    let canonical = std::fs::canonicalize(&source).map_err(|e| format!("{stem}: {e}"))?;
    let program_path = transport(binary, &canonical, stem)?;
    let program = program_path.to_string_lossy().to_string();
    let mut args = vec!["eval", program.as_str()];
    if !served.is_empty() {
        args.push("--serve");
        args.push(served);
    }
    let got = run(binary, &args).map_err(|e| format!("{stem}: eval {e}"));
    let _ = std::fs::remove_file(&program_path);
    let got = normalize(&got?.0, &canonical.to_string_lossy());
    let expected_path = source.with_file_name(format!("{stem}.expected.json"));
    let text = std::fs::read_to_string(&expected_path).map_err(|e| format!("{stem}: {e}"))?;
    let want: Value = serde_json::from_str(&text).map_err(|e| format!("{stem}: {e}"))?;
    if got["closure"] != want["closure"] {
        return Err(format!(
            "{stem}: closure differs\n  want: {}\n  got:  {}",
            want["closure"], got["closure"]
        ));
    }
    if got["diagnostics"] != want["diagnostics"] {
        return Err(format!(
            "{stem}: diagnostics differ\n  want: {}\n  got:  {}",
            want["diagnostics"], got["diagnostics"]
        ));
    }
    Ok(())
}

#[test]
pub fn every_effect_fixture_matches_expected() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let failures: Vec<String> = SERVED
        .iter()
        .filter_map(|(stem, served)| check_fixture(binary, stem, served).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
pub fn serving_a_name_the_program_does_not_declare_is_a_diagnostic() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let canonical = std::fs::canonicalize(fixture_path("0_pending")).unwrap();
    let program_path = transport(binary, &canonical, "0_pending").unwrap();
    let program = program_path.to_string_lossy().to_string();
    let (got, code) = run(binary, &["eval", &program, "--serve", "fetch_jsonn"]).unwrap();
    let _ = std::fs::remove_file(&program_path);
    assert_eq!(code, 1, "exit code");
    assert_eq!(got["closure"], json!([]), "closure");
    assert_eq!(
        got["diagnostics"],
        json!([{
            "phase": "eval",
            "payload": {"f": "served_relation_unknown", "args": [{"a": "fetch_jsonn"}]}
        }])
    );
}

#[test]
pub fn every_declared_relation_reaches_the_names_table() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let canonical = std::fs::canonicalize(fixture_path("1_settled")).unwrap();
    let (compiled, code) = run(binary, &["compile", &canonical.to_string_lossy()]).unwrap();
    assert_eq!(code, 0, "compile exit code");
    let names = compiled["program"]["names"].as_object().unwrap();
    for declared in ["fetch_json", "Watch", "Body"] {
        let relation = names
            .get(declared)
            .unwrap_or_else(|| panic!("{declared} missing from names"));
        assert_eq!(relation["f"], "ref", "{declared} is not a ref");
    }
}
