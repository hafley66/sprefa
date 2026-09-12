//! Whole-pipeline parity through the real binary. Each
//! `oracle/compile/cases/*.json` is one v7 `compile_dl7/4` or
//! `compile_dl7_project_rows/6` call frozen at `f5018ad23` by
//! `oracle/compile/freeze.sh`; `dl8 compile` must print the same bytes and exit
//! with the same code. `oracle/compile/status.json` indexes all 46 cases; the
//! committed subset is whatever fits 6 MB, and `oracle/compile/verify.sh` runs
//! dl8 over every one of them.
//!
//! The comparison is byte equality: `serde_json::to_string` over a `Map` is a
//! BTreeMap walk with no whitespace, the same bytes `dl8 compile` writes.
//!
//! FAIL-FIRST RECEIPT (2026-09-12), applied, observed, reverted:
//! `_2_lower/_12_units.rs::install_importer_aliases` starts the alias ordinal
//! at 0 instead of `next_owner_index/3 + 1` (`0a_module_lowerer.pl:428`):
//! `applications-dl6-0_catalog` differs and the suite is red.
//!
//! UNCOVERED MUTATIONS, applied and observed green, so the corpus does not
//! price them: `_8_driver/_0_read.rs::join` with `""` instead of `"\n"` (every
//! prelude file already ends in a newline and no prelude offset reaches the
//! output), and dropping the `bound.contains(&name)` shadow guard at
//! `0a_module_lowerer.pl:443` (no fixture rebinds a prelude name locally).

use std::path::{Path, PathBuf};
use std::process::Command;

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/compile")
}

fn sources_dir() -> String {
    oracle_dir().join("sources").display().to_string()
}

fn status() -> serde_json::Value {
    let text = std::fs::read_to_string(oracle_dir().join("status.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn arguments(case: &serde_json::Value) -> Vec<String> {
    let sources = sources_dir();
    case["input"]["arguments"]
        .as_array()
        .expect("case without arguments")
        .iter()
        .map(|a| a.as_str().unwrap().replace("<root>", &sources))
        .collect()
}

#[test]
fn every_committed_case_matches_v7() {
    let sources = sources_dir();
    let mut checked = 0;
    let mut rows = Vec::new();
    for entry in status()["cases"].as_array().unwrap() {
        if !entry["committed"].as_bool().unwrap() {
            continue;
        }
        let stem = entry["stem"].as_str().unwrap();
        let path = oracle_dir().join("cases").join(format!("{stem}.json"));
        let case: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let started = std::time::Instant::now();
        let run = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("compile")
            .args(arguments(&case))
            .output()
            .unwrap();
        let ms = started.elapsed().as_millis();
        let got = String::from_utf8(run.stdout).unwrap();
        let got = got.trim_end_matches('\n').replace(&sources, "<root>");
        let want = serde_json::to_string(&case["expected"]).unwrap();
        assert_eq!(got, want, "{stem}: stdout differs from v7");
        let expected_code = i32::from(
            !case["expected"]["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty(),
        );
        assert_eq!(
            run.status.code(),
            Some(expected_code),
            "{stem}: exit code differs from v7"
        );
        rows.push((stem.to_string(), ms, entry["v7_ms"].as_u64().unwrap()));
        checked += 1;
    }
    assert!(checked > 0, "no committed cases");
    for (stem, ms, v7) in &rows {
        println!("{stem:58} dl8 {ms:>5} ms   v7 {v7:>5} ms");
    }
}

#[test]
fn every_committed_case_file_exists_and_every_case_is_indexed() {
    let cases = status();
    let cases = cases["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 46, "status.json case count");
    let mut committed = 0;
    for entry in cases {
        let stem = entry["stem"].as_str().unwrap();
        let path = oracle_dir().join("cases").join(format!("{stem}.json"));
        assert_eq!(
            path.exists(),
            entry["committed"].as_bool().unwrap(),
            "{stem}: committed flag and case file disagree"
        );
        if path.exists() {
            committed += std::fs::metadata(&path).unwrap().len();
        }
    }
    assert!(
        committed <= 6 * 1024 * 1024,
        "committed bytes {committed} over budget"
    );
}

#[test]
fn the_curry_limit_is_a_case_not_a_skip() {
    let case: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(oracle_dir().join("cases/test-fixtures-5_curry.json")).unwrap(),
    )
    .unwrap();
    let reason = &case["expected"]["diagnostics"][0]["args"][2];
    assert_eq!(reason["f"], "source_refreeze_limit_exhausted");
    assert_eq!(reason["args"][0], 16);
}
