//! Oracle parity through the real binary. Every `oracle/load/*.json` holds one
//! call v7 made to a project fact loader, frozen by `oracle/load/dump_load.pl`
//! while v7 ran its own plunit files, compiled three projects, and answered a
//! crafted probe per diagnostic code the corpus does not reach. `dl8 load`
//! must print the same rows, basements, origins and diagnostics, and exit with
//! the same code.
//!
//! FAIL-FIRST RECEIPT: on the base sha `dl8 load` is the `_0_load` stub, which
//! prints nothing and exits 3, so `every_loader_case_matches_v7` fails on the
//! first case with "no JSON on stdout" and the impure-edge test fails to link
//! `dl8::_4_comptime::load::read`.
//!
//! `oracle/load/status.json` carries `skip` for cases the port does not answer
//! yet (empty), and `checked_not_committed` for cases the freeze verified and
//! kept out of git for size.

use dl8::_4_comptime::load::read::{read_dir_tree, read_source_fact_files, read_tsi_stream};
use std::path::{Path, PathBuf};
use std::process::Command;

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/load")
}

fn status() -> serde_json::Value {
    let text = std::fs::read_to_string(oracle_dir().join("status.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn cases() -> Vec<PathBuf> {
    let status = status();
    let skip: Vec<String> = status["skip"]
        .as_array()
        .expect("status.json without skip")
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
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
fn every_loader_case_matches_v7() {
    let cases = cases();
    assert!(!cases.is_empty(), "no loader cases found");
    let mut failures = Vec::new();
    for path in &cases {
        let text = std::fs::read_to_string(path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("load")
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
        let fields = want.as_object().expect("expected is an object");
        for field in fields.keys() {
            if got[field] != want[field] {
                failures.push(format!(
                    "{}: {field} differs\n  want: {}\n  got:  {}",
                    path.display(),
                    truncate(&want[field]),
                    truncate(&got[field])
                ));
            }
        }
        let want_code = match want.get("diagnostics").and_then(|d| d.as_array()) {
            Some(rows) if !rows.is_empty() => 1,
            _ => 0,
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

/// The byte edge, over the committed directory the pure cases were frozen
/// from. Each assertion ties an edge function to the case that carries the
/// same bytes as already-read data.
#[test]
fn the_byte_edge_reproduces_the_case_inputs() {
    let cases = oracle_dir().join("cases");

    let stream = cases.join("tsi/0_syntax_user.jsonl");
    let lines = read_tsi_stream(&stream).unwrap();
    assert_eq!(lines, frozen_lines("0_syntax_user.jsonl"));

    let broken = cases.join("tsi/1_broken.jsonl");
    assert_eq!(
        read_tsi_stream(&broken).unwrap(),
        frozen_lines("1_broken.jsonl")
    );

    let facts = cases.join("source_facts/1_expected.json");
    let read = read_source_fact_files(std::slice::from_ref(&facts));
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].0, facts);
    assert_eq!(
        read[0].1.as_ref().unwrap(),
        &frozen_text("source_facts/1_expected.json")
    );

    let tree = read_dir_tree(&cases.join("project")).unwrap();
    assert_eq!(
        tree,
        vec![
            cases.join("project/0_src/0_root.dl7"),
            cases.join("project/0_src/1_models/0_users.dl7"),
        ]
    );
}

/// The `lines` array of the committed `load_tsi_lines` case whose origin ends
/// in this file name.
fn frozen_lines(name: &str) -> Vec<String> {
    for path in cases() {
        let text = std::fs::read_to_string(&path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let input = &case["input"];
        if input["call"] != "load_tsi_lines" {
            continue;
        }
        let origin = input["origin"]["a"].as_str().unwrap_or_default();
        if !origin.ends_with(name) {
            continue;
        }
        return input["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["s"].as_str().unwrap().to_string())
            .collect();
    }
    panic!("no committed load_tsi_lines case for {name}");
}

/// The `text` of the committed `load_source_fact_texts` case whose path ends
/// in this suffix.
fn frozen_text(suffix: &str) -> String {
    for path in cases() {
        let text = std::fs::read_to_string(&path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let input = &case["input"];
        if input["call"] != "load_source_fact_texts" {
            continue;
        }
        for file in input["files"].as_array().unwrap() {
            let args = file["args"].as_array().unwrap();
            let path = args[0]["a"].as_str().unwrap_or_default();
            if path.ends_with(suffix) {
                return args[1]["s"].as_str().unwrap().to_string();
            }
        }
    }
    panic!("no committed load_source_fact_texts case for {suffix}");
}

fn truncate(value: &serde_json::Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(400) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}
