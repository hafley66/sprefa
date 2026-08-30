//! A def coordinate several names share names nothing.
//!
//! Every item spliced out of one macro expansion reports the macro CALL's own
//! span (`rust_mbe.rs`), so `(blob, span)` stops identifying a def: the emitted
//! `callee_name` is whichever def won the span slot. In rust-analyzer's
//! `crates/intern/src/symbol/symbols.rs` the `define_symbols!` expansion puts
//! 537 defs on one span, and 2,998 of our 10,693 doubly-unsupported excess rows
//! (neither ra_ap_ide nor codeql emits them) point at it.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, `src/lang/rust{,_modules}.rs` stashed at
//! 10e937541): `a_collapsed_span_binds_nothing` red with
//! `[("caller", "alpha", "gen")]`; `a_clean_span_in_the_same_file_still_binds`
//! green pre-fix, which is what makes the guard's narrowness measured.
//!
//! Fixtures: `tests/fixtures/rust_findings/collapsed_span/` (`define_pair!`
//! expands to two fns; both defs land on the invocation's span).

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_findings/collapsed_span";

const FILES: &[&str] = &["lib.rs", "gen.rs", "user.rs"];

fn run() -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call,type".to_string(),
    ];
    args.extend(FILES.iter().map(|name| format!("{DIR}/{name}")));
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

fn text(row: &Value, key: &str) -> String {
    row[key].as_str().unwrap_or("").to_string()
}

/// (caller_name, callee_name, callee file stem) per `resolved_edge`.
fn edges() -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = run()
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                text(row, "caller_name"),
                text(row, "callee_name"),
                text(row, "callee_path")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".rs")
                    .to_string(),
            )
        })
        .collect();
    rows.sort();
    rows.dedup();
    rows
}

#[test]
fn a_collapsed_span_binds_nothing() {
    let rows = edges();
    let into_gen: Vec<&(String, String, String)> = rows
        .iter()
        .filter(|(caller, callee, stem)| caller == "caller" && stem == "gen" && callee != "plain")
        .collect();
    assert!(
        into_gen.is_empty(),
        "alpha() sits on a span alpha and beta share, so it binds nothing: {into_gen:?}"
    );
}

/// The guard is per coordinate, never per file: `plain` sits on its own span
/// in the same file and still binds.
#[test]
fn a_clean_span_in_the_same_file_still_binds() {
    let rows = edges();
    assert!(
        rows.iter().any(|(caller, callee, stem)| {
            caller == "caller" && callee == "plain" && stem == "gen"
        }),
        "plain() must still bind: {rows:?}"
    );
}
