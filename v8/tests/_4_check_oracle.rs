//! Oracle parity through the real binary. Every `oracle/check/*.json` holds one
//! `check_datalog/4` or `check_resolved_rules/5` call v7 made, frozen by
//! `oracle/check/dump_check.pl`. `dl8 check` must print the same checked
//! program, the same dependency and stratum rows, the same diagnostics, and
//! exit with the same code.
//!
//! `oracle/check/status.json` carries a `skip` array of case file names whose
//! passes are not ported yet, so the gate stays green and the debt is a number.
//!
//! FAIL-FIRST RECEIPTS (2026-09-12), each applied, observed, and reverted:
//!
//! 1. `_5_kernel.rs` `COMPARISONS` reordered to the lowerer's spelling
//!    (`int_lt, int_le, int_gt, int_ge, int_eq, int_ne`): all four
//!    `*_check_datalog` cases report `checked differs`. The kernel node list is
//!    an unsorted append at `1_checker.pl:423`, so the order is observable.
//! 2. `_5_kernel.rs` `kernel_relation_keys("nil")` set to the lowerer's
//!    `[[]]` instead of the checker's `[[0]]`: the same four cases report
//!    `checked differs`.
//! 3. `_4_mode.rs` `cons` input modes reordered to `[[0,1],[2]]`:
//!    `case-4_under_cons_0` reports `diagnostics differs`.
//!
//! A fourth mutation did NOT fail and is recorded as an uncovered path:
//! replacing `prolog_msort` with `prolog_sort` on the kernel edge list leaves
//! the suite green, because no committed case carries a duplicated `':'` edge.

use std::path::{Path, PathBuf};
use std::process::Command;

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/check")
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
fn every_checker_case_matches_v7() {
    let cases = cases();
    assert!(!cases.is_empty(), "no checker cases found");
    let mut failures = Vec::new();
    for path in &cases {
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
                    truncate(value),
                    truncate(got.get(field).unwrap_or(&serde_json::Value::Null))
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
    let text = std::fs::read_to_string(oracle_dir().join("status.json")).unwrap();
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

fn truncate(value: &serde_json::Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(400) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}
