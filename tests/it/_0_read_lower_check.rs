use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

// From tests/_1_read_oracle.rs
// Reader parity through the real binary. Every `oracle/read/*.json` holds one
// input and the forms, source rows and diagnostics v7 `read_dl7/5` produced.

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/read");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    out.sort();
    out
}

#[test]
fn every_oracle_fixture_matches_v7() {
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no oracle fixtures found");
    let mut failures = Vec::new();
    for path in &fixtures {
        let text = std::fs::read_to_string(path).unwrap();
        let fixture: serde_json::Value = serde_json::from_str(&text).unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let input_path =
            std::env::temp_dir().join(format!("dl8-read-{}-{}.dl7", std::process::id(), stem));
        std::fs::write(&input_path, fixture["input"].as_str().unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("read")
            .arg(&input_path)
            .env("DL8_READ_PATH", fixture["path"].as_str().unwrap())
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&input_path);
        let got: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "{stem}: no JSON on stdout ({e}); stderr: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
        };
        let want = &fixture["expected"];
        for key in ["forms", "source_rows", "diagnostics"] {
            if got[key] != want[key] {
                failures.push(format!(
                    "{stem}: {key} differ\n  want: {}\n  got:  {}",
                    want[key], got[key]
                ));
            }
        }
        let want_code = if want["diagnostics"].as_array().is_some_and(|d| d.is_empty()) {
            0
        } else {
            1
        };
        if output.status.code() != Some(want_code) {
            failures.push(format!(
                "{stem}: exit {:?}, want {want_code}",
                output.status.code()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// From tests/_2_macrotime_oracle.rs
// Macrotime parity through the real binary. Every `oracle/macrotime/*_<n>.json`
// holds one reader unit's forms, its source rows and the sliced macro program
// v7 gave `expand_syntax/5`, next to the rows, forms and diagnostics v7
// produced, frozen by `oracle/macrotime/dump_macrotime.pl`. `dl8 expand` must
// print all of it.

fn cases() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/macrotime");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.rsplit('_').next())
                .is_some_and(|s| s.parse::<u32>().is_ok())
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_macrotime_case_matches_v7() {
    let cases = cases();
    assert!(!cases.is_empty(), "no macrotime oracle cases found");
    let keys = [
        "syntax_rows",
        "expanded_rows",
        "origin_rows",
        "diagnostics",
        "forms",
        "source_rows",
        "materialize_diagnostics",
    ];
    let mut failures = Vec::new();
    for path in &cases {
        let text = std::fs::read_to_string(path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("expand")
            .arg(path)
            .output()
            .unwrap();
        let got: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "{}: no JSON on stdout ({e}); stderr: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
        };
        let want = &case["expected"];
        for key in keys {
            if got[key] != want[key] {
                failures.push(format!(
                    "{}: {key} differs\n  want {} rows\n  got  {} rows\n  first difference: {}",
                    path.display(),
                    count(&want[key]),
                    count(&got[key]),
                    first_difference(&want[key], &got[key])
                ));
            }
        }
        let clean = want["diagnostics"].as_array().is_some_and(|d| d.is_empty())
            && want["materialize_diagnostics"]
                .as_array()
                .is_some_and(|d| d.is_empty());
        let want_code = if clean { 0 } else { 1 };
        if output.status.code() != Some(want_code) {
            failures.push(format!(
                "{}: exit {:?}, want {want_code}",
                path.display(),
                output.status.code()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn count(value: &serde_json::Value) -> usize {
    value.as_array().map_or(0, |a| a.len())
}

fn first_difference(want: &serde_json::Value, got: &serde_json::Value) -> String {
    let empty = Vec::new();
    let want = want.as_array().unwrap_or(&empty);
    let got = got.as_array().unwrap_or(&empty);
    for (i, w) in want.iter().enumerate() {
        match got.get(i) {
            Some(g) if g == w => continue,
            Some(g) => return format!("at {i}\n    want {w}\n    got  {g}"),
            None => return format!("at {i}: missing\n    want {w}"),
        }
    }
    match got.get(want.len()) {
        Some(g) => format!("at {}: extra\n    got {g}", want.len()),
        None => "none".into(),
    }
}

// From tests/_3_lower_oracle.rs
// Oracle parity through the real binary. Every `oracle/lower/*.json` holds one
// `lower_datalog/5` or `lower_datalog_deferred/5` call v7 made while compiling
// a fixture, frozen by `oracle/lower/dump_lower.pl`. `dl8 lower` must print the
// same program, the same origins and the same diagnostics.
//
// `oracle/lower/status.json` carries a `skip` array of case file names whose
// passes are not ported yet, so the gate stays green and the debt is a number.

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/lower")
}

fn skipped() -> Vec<String> {
    let text = std::fs::read_to_string(oracle_dir().join("status.json")).unwrap();
    let status: serde_json::Value = serde_json::from_str(&text).unwrap();
    status["skip"]
        .as_array()
        .expect("status.json without skip")
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn s3loweroracle_cases() -> Vec<PathBuf> {
    let skip = skipped();
    let mut out: Vec<PathBuf> = std::fs::read_dir(oracle_dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            name != "status.json" && !skip.contains(&name)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_lowering_case_matches_v7() {
    let s3loweroracle_cases = s3loweroracle_cases();
    assert!(!s3loweroracle_cases.is_empty(), "no lowering cases found");
    let mut failures = Vec::new();
    for path in &s3loweroracle_cases {
        let text = std::fs::read_to_string(path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("lower")
            .arg(path)
            .output()
            .unwrap();
        let got: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "{}: no JSON on stdout ({e}); stderr: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
        };
        let want = &case["expected"];
        for field in ["program", "program_bound", "origins", "diagnostics"] {
            if got[field] != want[field] {
                failures.push(format!(
                    "{}: {field} differs\n  want: {}\n  got:  {}",
                    path.display(),
                    truncate(&want[field]),
                    truncate(&got[field])
                ));
            }
        }
        let want_code = if want["diagnostics"].as_array().is_none_or(|d| d.is_empty()) {
            0
        } else {
            1
        };
        if output.status.code() != Some(want_code) {
            failures.push(format!(
                "{}: exit {:?}, want {want_code}",
                path.display(),
                output.status.code()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn truncate(value: &serde_json::Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(400) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}

// From tests/_4_check_oracle.rs
// Oracle parity through the real binary. Every `oracle/check/*.json` holds one
// `check_datalog/4` or `check_resolved_rules/5` call v7 made, frozen by
// `oracle/check/dump_check.pl`. `dl8 check` must print the same checked
// program, the same dependency and stratum rows, the same diagnostics, and
// exit with the same code.
//
// `oracle/check/status.json` carries a `skip` array of case file names whose
// passes are not ported yet, so the gate stays green and the debt is a number.
//
// FAIL-FIRST RECEIPTS (2026-09-12), each applied, observed, and reverted:
//
// 1. `_5_kernel.rs` `COMPARISONS` reordered to the lowerer's spelling
//    (`lt, le, gt, ge, eq, ne` off `int`): all four
//    `*_check_datalog` cases report `checked differs`. The kernel node list is
//    an unsorted append at `1_checker.pl:423`, so the order is observable.
// 2. `_5_kernel.rs` `kernel_relation_keys("nil")` set to the lowerer's
//    `[[]]` instead of the checker's `[[0]]`: the same four cases report
//    `checked differs`.
// 3. `_4_mode.rs` `cons` input modes reordered to `[[0,1],[2]]`:
//    `case-4_under_cons_0` reports `diagnostics differs`.
//
// A fourth mutation did NOT fail and is recorded as an uncovered path:
// replacing `prolog_msort` with `prolog_sort` on the kernel edge list leaves
// the suite green, because no committed case carries a duplicated `':'` edge.

fn s4checkoracle_oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/check")
}

fn s4checkoracle_skipped() -> Vec<String> {
    let text = std::fs::read_to_string(s4checkoracle_oracle_dir().join("status.json")).unwrap();
    let status: serde_json::Value = serde_json::from_str(&text).unwrap();
    status["skip"]
        .as_array()
        .expect("status.json without skip")
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn s4checkoracle_cases() -> Vec<PathBuf> {
    let skip = s4checkoracle_skipped();
    let mut out: Vec<PathBuf> = std::fs::read_dir(s4checkoracle_oracle_dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            name != "status.json" && !skip.contains(&name)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_checker_case_matches_v7() {
    let s4checkoracle_cases = s4checkoracle_cases();
    assert!(!s4checkoracle_cases.is_empty(), "no checker cases found");
    let mut failures = Vec::new();
    for path in &s4checkoracle_cases {
        let text = std::fs::read_to_string(path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("check")
            .arg(path)
            .output()
            .unwrap();
        let got: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "{}: no JSON on stdout ({e}); stderr: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
        };
        let want = &case["expected"];
        for (field, value) in want.as_object().expect("expected object") {
            if got.get(field) != Some(value) {
                failures.push(format!(
                    "{}: {field} differs\n  want: {}\n  got:  {}",
                    path.display(),
                    s4checkoracle_truncate(value),
                    s4checkoracle_truncate(got.get(field).unwrap_or(&serde_json::Value::Null))
                ));
            }
        }
        let want_code = i32::from(!want["diagnostics"].as_array().is_none_or(|d| d.is_empty()));
        if output.status.code() != Some(want_code) {
            failures.push(format!(
                "{}: exit {:?}, want {want_code}",
                path.display(),
                output.status.code()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_diagnostic_reason_named_in_status_is_reachable_or_explained() {
    let text = std::fs::read_to_string(s4checkoracle_oracle_dir().join("status.json")).unwrap();
    let status: serde_json::Value = serde_json::from_str(&text).unwrap();
    let reasons = &status["diagnostic_reasons"];
    let covered = reasons["covered_by_authored_case"]
        .as_object()
        .expect("covered_by_authored_case");
    for (reason, note) in covered {
        let note = note.as_str().expect("reason note");
        let file = note.split(',').next().unwrap().trim();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("oracle")
            .join("check")
            .join(file);
        assert!(path.exists(), "{reason}: {} is missing", path.display());
    }
    let total = covered.len()
        + reasons["no_dl7_reaches_it"].as_object().unwrap().len()
        + reasons["unreachable_by_construction"]
            .as_object()
            .unwrap()
            .len();
    assert_eq!(total, 25, "every reason functor must be classified");
}

fn s4checkoracle_truncate(value: &serde_json::Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(400) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}
