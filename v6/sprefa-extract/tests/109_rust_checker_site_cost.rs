//! What the rust checker's per-file walk ASKS rust-analyzer, counted. Wall time
//! belongs to the machine; the number of `resolve_path` calls one supplied file
//! draws is this crate's own, and it is what the phase table pins here.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, commit d5217fca4, `in_type_position` deleted
//! from `walk_file`'s bare-path arm and `destination_of`'s memo deleted, so the
//! walk resolves every `ast::Path` and navs per site): `bodies.rs` read
//! `checker_type_path` 37 and `checker_nav` 17 against the 11 and 6 below;
//! `lib.rs` read 26 and 11 against the 25 and 9 below. The dropped calls are
//! expression-position paths the syntax leg answers, and repeat navs.
//!
//! The counts are the whole claim. The walk's wall clock did NOT move: the site
//! leg's cost is the first `infer` per function body, which some surviving arm
//! pays whatever the call count (PR body, hypothesis (a)).

#![cfg(all(feature = "cli", feature = "rust-checker"))]

use std::process::Command;

const ROOT: &str = "tests/fixtures/tsi/rust_probe";
const BODIES: &str = "tests/fixtures/tsi/rust_probe/src/bodies.rs";
const LIB: &str = "tests/fixtures/tsi/rust_probe/src/lib.rs";

/// One (lang, phase) row of the phase table as (files, calls, rows), read the
/// same way `31_tracing.rs` reads it.
fn phase_row(table: &str, lang: &str, phase: &str) -> Option<(u64, u64, u64)> {
    table
        .lines()
        .skip_while(|line| !line.starts_with("extract phases: load "))
        .skip(2)
        .take_while(|line| !line.trim().is_empty())
        .find_map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            (columns.len() == 7 && columns[0] == lang && columns[1] == phase).then(|| {
                (
                    columns[2].parse().unwrap(),
                    columns[3].parse().unwrap(),
                    columns[4].parse().unwrap(),
                )
            })
        })
}

/// One checker-driven resolve over one supplied file, phase table on stderr.
fn phases_of(supplied: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "--resolve",
            "--family",
            "type",
            "--project-root",
            ROOT,
            "--rust-checker",
            supplied,
        ])
        .env_remove("RUST_LOG")
        .env("DL_TRACE_SUMMARY", "1")
        .env("DL_TRAIL", "0")
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{supplied} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn calls(table: &str, phase: &str) -> u64 {
    let (files, calls, _) = phase_row(table, "rust", phase)
        .unwrap_or_else(|| panic!("no rust/{phase} row in\n{table}"));
    assert_eq!(files, 1, "{phase} ran over {files} files, want 1");
    calls
}

/// `bodies.rs` is all function bodies: 3 method calls, 5 call/record paths, and
/// a crowd of expression-position paths the walk must NOT ask about.
///
/// `checker_nav` counts memo misses, so it reads the number of DEFINITIONS the
/// file names, never the number of sites naming them.
const BODIES_CALLS: &[(&str, u64)] = &[
    ("checker_method", 3),
    ("checker_call_path", 5),
    ("checker_type_path", 11),
    ("checker_nav", 6),
];

/// `lib.rs` is all declarations, so nearly every path is type-position already
/// and the filter drops exactly one: the `vec![element]` body's own.
const LIB_CALLS: &[(&str, u64)] = &[
    ("checker_method", 0),
    ("checker_call_path", 0),
    ("checker_type_path", 25),
    ("checker_nav", 9),
];

#[test]
fn a_body_file_draws_one_resolve_per_type_position_path() {
    let table = phases_of(BODIES);
    let read: Vec<(&str, u64)> = BODIES_CALLS
        .iter()
        .map(|(phase, _)| (*phase, calls(&table, phase)))
        .collect();
    assert_eq!(read, BODIES_CALLS.to_vec(), "\n{table}");
}

#[test]
fn a_declaration_file_draws_one_resolve_per_type_position_path() {
    let table = phases_of(LIB);
    let read: Vec<(&str, u64)> = LIB_CALLS
        .iter()
        .map(|(phase, _)| (*phase, calls(&table, phase)))
        .collect();
    assert_eq!(read, LIB_CALLS.to_vec(), "\n{table}");
}
