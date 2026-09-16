//! Oracle parity through the real binary. Every `oracle/reify/*.json` holds one
//! reifier, grapher or artifact-emitter call v7 made, frozen by
//! `oracle/reify/dump_reify.pl`. `dl8 reify` must print the same rows, the same
//! calls, the same artifact and diagnostics, and exit with the same code.
//!
//! `oracle/reify/status.json` carries a `skip` array of case file names whose
//! arms are not ported yet, so the gate stays green and the debt is a number.
//!
//! FAIL-FIRST RECEIPTS (2026-09-12), each applied, observed, and reverted:
//!
//! 1. `_1_rows.rs` writes the atom `rule` instead of `level` for
//!    `program_rule_kind` (`0_logical_program_reifier.pl:228`): every
//!    `logical_program_rows` and `logical_program_calls*` case differs.
//! 2. `_2_calls.rs` `constant_text` keeps the atom instead of running
//!    `atom_string/2` (`:161`, `:166`, `:184`): every case carrying
//!    `program_rule_kind`, `program_goal` or `program_dependency` calls differs.
//! 3. `_3_graph.rs` `argument_target` wraps a `reference` target the way the
//!    calls path does (`0a_logical_program_grapher.pl:115` versus
//!    `0_logical_program_reifier.pl:113`): `case-17_argument_shapes_1` and
//!    `_3` differ. This mutation passed before `cases/17_argument_shapes.pl`
//!    existed; that case was authored because of it.
//! 4. `_4_emit.rs` dl7 arm returns the bare `[]` rather than `artifacts([])`
//!    when key validation fails (`1_artifact_emitter.pl:107`):
//!    `case-8_emit_functional_key_conflict_5` reports `artifact differs`.
//! 5. `_1_rows.rs` swaps the `argument_child(ArgumentId, input)` arguments
//!    (`0_logical_program_reifier.pl:270`): `case-17_argument_shapes_0`, `_1`
//!    and `_3` differ.

use std::path::{Path, PathBuf};
use std::process::Command;

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/reify")
}

fn status() -> serde_json::Value {
    let text = std::fs::read_to_string(oracle_dir().join("status.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn skipped() -> Vec<String> {
    status()["skip"]
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
fn every_reify_case_matches_v7() {
    let cases = cases();
    assert!(!cases.is_empty(), "no reify cases found");
    let mut failures = Vec::new();
    for path in &cases {
        let text = std::fs::read_to_string(path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("reify")
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
fn every_skipped_case_exists_and_names_its_v7_line() {
    let status = status();
    let notes = status["not_ported"]
        .as_object()
        .expect("status.json without not_ported");
    for name in skipped() {
        let path = oracle_dir().join(&name);
        assert!(path.exists(), "{name} is skipped but not committed");
        assert!(
            notes.contains_key(&name),
            "{name} is skipped with no reason"
        );
    }
    for note in notes.values() {
        let note = note.as_str().expect("not_ported note");
        assert!(note.contains(".pl:"), "{note} names no v7 line");
    }
}

#[test]
fn every_diagnostic_reason_is_covered_or_explained() {
    let status = status();
    let reasons = &status["diagnostic_reasons"];
    let covered = reasons["covered_by_authored_case"]
        .as_object()
        .expect("covered_by_authored_case");
    for (reason, note) in covered {
        let note = note.as_str().expect("reason note");
        let file = note.split(',').next().unwrap().trim();
        let path = oracle_dir().join(file);
        assert!(path.exists(), "{reason}: {} is missing", path.display());
    }
    let total = covered.len() + reasons["frozen_but_not_ported"].as_object().unwrap().len();
    assert_eq!(total, 10, "every reason functor must be classified");
}

fn truncate(value: &serde_json::Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(400) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}
