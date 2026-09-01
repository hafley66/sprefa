//! The rust CHECKER tier on the TYPE plane: its answer takes the edge, and its
//! EXTERNAL answer suppresses a name match that binds the wrong file.
//!
//! FAIL-PRE-FIX RECEIPT (same fixture, `--features cli` alone, checker off):
//! `Located -> decoys.rs PathBuf | corpus_unique`. `PathBuf` is std and
//! `decoys.rs` holds the only corpus declaration of the name, so the
//! corpus-unique leg hands the field the decoy. With the tier on, the row is
//! absent. Measured on the rust-analyzer corpus the same suppression is the
//! type plane's precision: 92.31 checker-off, 98.26 on (REPORT sec 33.2).
//!
//! The rust twin of `92_ts_checker.rs:the_checker_answers_the_type_plane_too`,
//! which pinned the ts type plane while rust carried call assertions only.
//!
//! The module plane already chases glob imports and renames, so `Holder` and
//! `Renamed` pin WHICH tier answers, not whether the edge exists.
//!
//! The fixture is its own cargo workspace (`[workspace]` with no members), so
//! `cargo metadata` resolves it standalone and it pulls no dependency.

#![cfg(feature = "rust-checker")]

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_checker_types";

const FILES: &[&str] = &["src/lib.rs", "src/widget.rs", "src/decoys.rs"];

fn run(checker: bool) -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call,type".to_string(),
        "--project-root".to_string(),
        DIR.to_string(),
    ];
    if checker {
        args.push("--rust-checker".to_string());
    }
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
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("one json fact per line"))
        .collect()
}

/// (owner_name, target file tail, target_name, resolution origin) per type edge.
fn type_edges(facts: &[Value]) -> Vec<(String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String)> = facts
        .iter()
        .filter(|fact| fact["record"] == "resolved_type_edge")
        .map(|fact| {
            (
                fact["owner_name"].as_str().unwrap_or_default().to_string(),
                fact["target_path"]
                    .as_str()
                    .unwrap_or_default()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                fact["target_name"].as_str().unwrap_or_default().to_string(),
                fact["resolution_origin"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// `PathBuf` is std, and `decoys.rs` holds the only corpus declaration of the
/// name, so the corpus-unique leg hands the field the decoy.
#[test]
fn the_syntax_leg_alone_binds_a_std_name_to_its_decoy() {
    let edges = type_edges(&run(false));
    assert!(
        edges.iter().any(|(owner, file, name, origin)| owner == "Located"
            && file == "decoys.rs"
            && name == "PathBuf"
            && origin == "corpus_unique"),
        "the syntax leg binds the decoy PathBuf, got {edges:?}"
    );
}

/// EXTERNAL is knowledge: the compiler placed `PathBuf` in std, so no
/// name-match leg may hand the field the corpus type that shares its name.
#[test]
fn an_external_type_answer_suppresses_the_name_match() {
    let edges = type_edges(&run(true));
    assert!(
        !edges
            .iter()
            .any(|(owner, _, name, _)| owner == "Located" && name == "PathBuf"),
        "a std field type binds no corpus edge, got {edges:?}"
    );
}

/// The tier answers ahead of the name-match legs, and names the `widget.rs`
/// declaration rather than the decoy that shares the name.
#[test]
fn the_checker_answers_the_type_plane() {
    let edges = type_edges(&run(true));
    assert!(
        edges.contains(&(
            "Holder".to_string(),
            "widget.rs".to_string(),
            "Widget".to_string(),
            "checker".to_string(),
        )),
        "the glob-imported field type carries origin checker, got {edges:?}"
    );
}

/// The answer is keyed on the DECLARED name, so a renaming re-export binds a
/// target this file never spells.
#[test]
fn the_checker_answers_under_a_name_the_reference_never_spells() {
    let edges = type_edges(&run(true));
    assert!(
        edges.contains(&(
            "Renamed".to_string(),
            "widget.rs".to_string(),
            "Widget".to_string(),
            "checker".to_string(),
        )),
        "`Gadget` binds widget.rs's `Widget` under origin checker, got {edges:?}"
    );
}

/// The type plane keys the tier's answers on (file, name) because a
/// `TypeEdgeCandidate` carries no reference span. `Mixed` names two `Config`
/// types in one file, so both answers collapse and the name-match legs bind.
/// Counted as `type_ambiguous` on the index.
#[test]
fn a_name_one_file_resolves_two_ways_falls_back_to_the_syntax_leg() {
    let edges = type_edges(&run(true));
    assert!(
        !edges.iter().any(|(owner, _, name, origin)| owner == "Mixed"
            && name == "Config"
            && origin == "checker"),
        "the collapsed name reaches no checker edge, got {edges:?}"
    );
    assert_eq!(
        edges
            .iter()
            .filter(|(owner, _, name, origin)| owner == "Mixed"
                && name == "Config"
                && origin == "module_plane")
            .count(),
        2,
        "both spellings still bind through the module plane, got {edges:?}"
    );
}
