//! Programmable-fold parity through the real binary. Each `fixtures/fold/*.dl7`
//! is compiled by `dl8 compile`, whose comptime rounds evaluate the program in
//! process; the rows the fixture's own relations carry, plus the diagnostics,
//! must match the sibling `*.expected.json`.
//!
//! `dl8 eval` is not the door here: its JSON transport
//! (`src/_6_eval/_6_json.rs`) carries the four aggregate keys and no `fold`
//! key, and that file belongs to another lane.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A relation reference the fixture file owns, rather than the prelude or the
/// kernel.
fn owned_by_fixture(relation: &Value, path: &str) -> bool {
    serde_json::to_string(relation)
        .map(|text| text.contains(path))
        .unwrap_or(false)
}

/// `call(Relation, Arguments)` rows of the fixture's own relations, in the
/// order the closure carries them.
fn fixture_rows(compiler_rows: &Value, path: &str) -> Result<Vec<Value>, String> {
    let rows = compiler_rows
        .as_array()
        .ok_or("compiler_rows is not a list")?;
    let mut out = Vec::new();
    for row in rows {
        if row.get("f").and_then(Value::as_str) != Some("call") {
            continue;
        }
        let args = row
            .get("args")
            .and_then(Value::as_array)
            .ok_or("call/2 expected")?;
        let relation = args.first().ok_or("call without relation")?;
        if !owned_by_fixture(relation, path) {
            continue;
        }
        let arguments = args.get(1).ok_or("call without arguments")?;
        out.push(json!({ "rel": relation, "args": arguments }));
    }
    Ok(out)
}

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
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/fold");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "dl7"))
        .collect();
    out.sort();
    out
}

fn compile(binary: &str, input: &Path) -> Result<Value, String> {
    let output = Command::new(binary)
        .arg("compile")
        .arg(input)
        .output()
        .map_err(|e| format!("compile spawn: {e}"))?;
    serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "compile no JSON on stdout ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn observed(binary: &str, source: &Path) -> Result<(String, Value), String> {
    let stem = source.file_stem().unwrap().to_string_lossy().to_string();
    let canonical = std::fs::canonicalize(source).map_err(|e| format!("{stem}: {e}"))?;
    let fixture_path = canonical.to_string_lossy().to_string();
    let compiled = compile(binary, &canonical).map_err(|e| format!("{stem}: {e}"))?;
    let rows = fixture_rows(&compiled["compiler_rows"], &fixture_path)
        .map_err(|e| format!("{stem}: {e}"))?;
    let got = json!({ "rows": rows, "diagnostics": compiled["diagnostics"] });
    Ok((stem, normalize(&got, &fixture_path)))
}

fn check_fixture(binary: &str, source: &Path) -> Result<(), String> {
    let (stem, got) = observed(binary, source)?;
    let expected_path = source.with_file_name(format!("{stem}.expected.json"));
    let text = std::fs::read_to_string(&expected_path).map_err(|e| format!("{stem}: {e}"))?;
    let want: Value = serde_json::from_str(&text).map_err(|e| format!("{stem}: {e}"))?;
    for key in ["rows", "diagnostics"] {
        if got[key] != want[key] {
            return Err(format!(
                "{stem}: {key} differ\n  want: {}\n  got:  {}",
                want[key], got[key]
            ));
        }
    }
    Ok(())
}

#[test]
pub fn every_fold_fixture_matches_expected() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no fold fixtures found");
    let failures: Vec<String> = fixtures
        .iter()
        .filter_map(|source| check_fixture(binary, source).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The folded value each fixture is written to produce, and the diagnostic the
/// zero-row step reports. Pins the numbers the expected files encode in term
/// shape: 142 = 7+35+100, 284 = twice that, 12 = 1,2,4 folded in ascending
/// order by an accumulator that doubles first (21 is the descending answer).
#[test]
pub fn every_fold_fixture_carries_its_written_answer() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let wanted: [(&str, Option<i64>, &str); 4] = [
        ("0_kernel_step", Some(142), ""),
        ("1_program_step", Some(284), ""),
        ("2_order_matters", Some(12), ""),
        ("3_step_no_row", None, "fold_step_no_row"),
    ];
    let mut failures: Vec<String> = Vec::new();
    for source in fixtures() {
        let (stem, got) = observed(binary, &source).unwrap();
        let Some((_, value, diagnostic)) = wanted.iter().find(|(name, _, _)| *name == stem) else {
            failures.push(format!("{stem}: fixture has no written answer"));
            continue;
        };
        let text = got.to_string();
        match value {
            Some(n) => {
                let folded = json!({ "f": "const", "args": [n] }).to_string();
                if !text.contains(&folded) {
                    failures.push(format!("{stem}: {n} absent from the rows"));
                }
            }
            None => {
                if !text.contains(diagnostic) {
                    failures.push(format!("{stem}: {diagnostic} absent"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
