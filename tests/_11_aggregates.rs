//! Aggregate-head parity through the real binary. Each
//! `fixtures/aggregates/*.dl7` compiles to a checked runtime program, which is
//! transported into `dl8 eval` and evaluated; the closure must match the sibling
//! `*.expected.json`, written from the program text. The fixture path is
//! replaced by `<fixture>` so the expected files carry no absolute path.
//!
//! `dl8 compile` prints the `program` object `dl8 eval` reads; the compile
//! output file goes into `dl8 eval` unchanged, and no test rebuilds it.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/aggregates");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "dl7"))
        .collect();
    out.sort();
    out
}

fn run(binary: &str, mode: &str, input: &Path) -> Result<Value, String> {
    let output = Command::new(binary)
        .arg(mode)
        .arg(input)
        .output()
        .map_err(|e| format!("{mode} spawn: {e}"))?;
    serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "{mode} no JSON on stdout ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// `dl8 compile`'s whole stdout is the program file; `dl8 eval` reads the
/// `program` key out of it.
fn compile_to(binary: &str, source: &Path, program: &Path) -> Result<(), String> {
    let output = Command::new(binary)
        .arg("compile")
        .arg(source)
        .output()
        .map_err(|e| format!("spawn: {e}"))?;
    if output.stdout.is_empty() {
        return Err(format!(
            "no stdout (exit {:?}); stderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    std::fs::write(program, &output.stdout).map_err(|e| format!("{e}"))
}

/// Compile one fixture, carry its runtime program through `dl8 eval`, and
/// compare the closure and diagnostics to the sibling expected file.
fn check_fixture(binary: &str, source: &Path) -> Result<(), String> {
    let stem = source.file_stem().unwrap().to_string_lossy().to_string();
    let canonical = std::fs::canonicalize(source).map_err(|e| format!("{stem}: {e}"))?;
    let fixture_path = canonical.to_string_lossy().to_string();
    let program_path =
        std::env::temp_dir().join(format!("dl8-aggregates-{}-{stem}.json", std::process::id()));
    compile_to(binary, &canonical, &program_path).map_err(|e| format!("{stem}: compile {e}"))?;
    let got = run(binary, "eval", &program_path).map_err(|e| format!("{stem}: eval {e}"));
    let _ = std::fs::remove_file(&program_path);
    let got = normalize(&got?, &fixture_path);
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
pub fn every_aggregate_fixture_matches_expected() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no aggregate fixtures found");
    let failures: Vec<String> = fixtures
        .iter()
        .filter_map(|source| check_fixture(binary, source).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
