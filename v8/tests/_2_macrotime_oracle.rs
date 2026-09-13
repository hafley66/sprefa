//! Macrotime parity through the real binary. Every `oracle/macrotime/*_<n>.json`
//! holds one reader unit's forms, its source rows and the sliced macro program
//! v7 gave `expand_syntax/5`, next to the rows, forms and diagnostics v7
//! produced, frozen by `oracle/macrotime/dump_macrotime.pl`. `dl8 expand` must
//! print all of it.

use std::path::{Path, PathBuf};
use std::process::Command;

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
