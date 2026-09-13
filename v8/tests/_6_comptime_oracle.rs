//! Oracle parity through the real binary. Every `oracle/comptime/*.json` holds
//! one comptime call v7 made, frozen by `oracle/comptime/dump_comptime.pl`.
//!
//! FAIL-FIRST RECEIPTS (2026-09-12), each applied, observed, reverted:
//!
//! 1. `_2_rounds.rs` stable arm returning `closure` instead of
//!    `strip_intern_rows(u, &closure)`: `2_partial` exits 3 with
//!    `replay compiler facts length differs`, so the replay guard catches a
//!    wrong `CompilerFacts` before the byte compare ever runs.
//! 2. `_5_finish.rs` `semantic_label` losing its `ref` arm: `2_partial`
//!    reports `compiled differs`; `:/4` rows whose label is a reference drop
//!    out of `TypeGraphFacts`.
//! 3. `_4_host.rs` `rule_uses_any` returning before the body scan:
//!    `8_hosted` reports `compiled differs` AND `diagnostics differs`.
//! 4. `_4_host.rs` planning ids dropping `HostPort`: both fixture cases report
//!    `compiled differs`.
//! 5. `_5_finish.rs` running `check_resolved_rules` on the pre-erase relation
//!    list: both fixture cases report `compiled differs`.
//!
//! Three mutations did NOT fail and are recorded as uncovered paths:
//! dropping `sum` from `graph_seeds` (no committed graph carries a `sum/1`
//! node), leaving `compiler_round_seeds` unsorted (the evaluator inserts seeds
//! into a set, so their order is unobservable), and dropping the
//! `frozen_generated_rules` arm of the stability test (no committed round
//! changes its generated rules without also changing its edges).

use std::path::{Path, PathBuf};
use std::process::Command;

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/comptime")
}

fn status() -> serde_json::Value {
    let text = std::fs::read_to_string(oracle_dir().join("status.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn skipped() -> Vec<String> {
    status()["skip"]
        .as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
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

fn truncate(value: &serde_json::Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(400) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}

#[test]
fn every_comptime_case_matches_v7() {
    let cases = cases();
    assert!(!cases.is_empty(), "no comptime cases found");
    let mut failures = Vec::new();
    for path in &cases {
        let text = std::fs::read_to_string(path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("comptime")
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
        let want = case["expected"].as_object().expect("expected object");
        for (field, value) in want {
            if got.get(field) != Some(value) {
                failures.push(format!(
                    "{}: {field} differs\n  want: {}\n  got:  {}",
                    path.display(),
                    truncate(value),
                    truncate(got.get(field).unwrap_or(&serde_json::Value::Null))
                ));
            }
        }
        let empty = want
            .get("diagnostics")
            .is_none_or(|d| d.as_array().is_none_or(|d| d.is_empty()));
        let want_code = i32::from(!empty);
        if output.status.code() != Some(want_code) {
            failures.push(format!(
                "{}: exit {:?}, want {want_code}",
                path.display(),
                output.status.code()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn dl8_round_counts_match_v7() {
    let status = status();
    let rounds = status["round_counts"]
        .as_object()
        .expect("status.json round_counts");
    for path in cases() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        if case["entry"] != "evaluate_checked" {
            continue;
        }
        let want = rounds
            .get(&name)
            .unwrap_or_else(|| panic!("{name}: no round count in status.json"));
        // One v7 assemble_generated_program call is one compiler round; one v7
        // refreeze call is one source-refreeze pass.
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("comptime")
            .arg(&path)
            .output()
            .unwrap();
        let got: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let inner = got["rounds"]
            .as_array()
            .expect("rounds")
            .iter()
            .filter(|e| e["event"] == "assemble")
            .count();
        let outer = got["rounds"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["event"] == "refreeze")
            .count();
        assert_eq!(
            (want["v7_assemble"].as_u64(), want["v7_refreeze"].as_u64()),
            (Some(inner as u64), Some(outer as u64)),
            "{name}: round counts differ"
        );
    }
}

#[test]
fn every_status_number_is_present() {
    let status = status();
    let numbers = status["numbers"].as_object().expect("numbers");
    for key in [
        "calls_dumped",
        "byte_distinct",
        "committed",
        "committed_bytes",
        "checked_but_not_committed",
        "fixtures",
    ] {
        assert!(numbers.contains_key(key), "status.json lacks {key}");
    }
    let uncovered = status["diagnostic_reasons"]["no_case_reaches_it"]
        .as_object()
        .expect("no_case_reaches_it");
    for (reason, site) in uncovered {
        assert!(
            site.as_str().is_some_and(|s| s.contains(".pl:")),
            "{reason}: no throw site"
        );
    }
}
