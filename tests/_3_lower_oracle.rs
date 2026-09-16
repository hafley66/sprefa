//! Oracle parity through the real binary. Every `oracle/lower/*.json` holds one
//! `lower_datalog/5` or `lower_datalog_deferred/5` call v7 made while compiling
//! a fixture, frozen by `oracle/lower/dump_lower.pl`. `dl8 lower` must print the
//! same program, the same origins and the same diagnostics.
//!
//! `oracle/lower/status.json` carries a `skip` array of case file names whose
//! passes are not ported yet, so the gate stays green and the debt is a number.

use std::path::{Path, PathBuf};
use std::process::Command;

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

fn cases() -> Vec<PathBuf> {
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
    let cases = cases();
    assert!(!cases.is_empty(), "no lowering cases found");
    let mut failures = Vec::new();
    for path in &cases {
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
