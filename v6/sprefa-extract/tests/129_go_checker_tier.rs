//! The go CHECKER tier: go/types answers destinations the syntax leg
//! name-matched, and the TSI semantic rows arrive from the same walk.
//!
//! SABOTAGE RECEIPT (this branch, before the tier landed): every case below
//! ran against a binary with no `--go-checker` flag and clap exited 2 on the
//! unknown argument; with the flag inert (no `go-checker` feature) the origin
//! assertions read `same_file`/`module_plane` and `checker_rows_need_the_flag`
//! is the one guarding the other direction.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const PROBE_ROOT: &str = "tests/fixtures/go_probe";
const PROBE_MAIN: &str = "tests/fixtures/go_probe/main.go";
const PROBE_SHAPES: &str = "tests/fixtures/go_probe/shapes/shapes.go";

fn facts(args: &[&str], path: Option<&PathBuf>) -> Vec<Value> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_extract"));
    command.current_dir(env!("CARGO_MANIFEST_DIR")).args(args);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    let output = command.output().expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("one json record per line"))
        .collect()
}

fn of_record<'a>(rows: &'a [Value], record: &str) -> Vec<&'a Value> {
    rows.iter().filter(|row| row["record"] == record).collect()
}

fn word(row: &Value, key: &str) -> String {
    row[key].as_str().unwrap_or_default().to_string()
}

/// (record, resolution_origin) -> count, the census the PR table carries.
fn origins(rows: &[Value]) -> BTreeMap<(String, String), usize> {
    let mut census = BTreeMap::new();
    for row in rows {
        let record = word(row, "record");
        if !record.starts_with("resolved_") {
            continue;
        }
        *census
            .entry((record, word(row, "resolution_origin")))
            .or_insert(0) += 1;
    }
    census
}

fn relations(rows: &[Value]) -> BTreeMap<String, usize> {
    let mut census = BTreeMap::new();
    for row in of_record(rows, "fact") {
        *census.entry(word(row, "relation")).or_insert(0) += 1;
    }
    census
}

fn probe_args() -> Vec<&'static str> {
    vec![
        "--witness",
        "--resolve",
        "--family",
        "call,type",
        "--project-root",
        PROBE_ROOT,
        "--go-checker",
        PROBE_MAIN,
        PROBE_SHAPES,
    ]
}

/// A scratch directory that exists and holds nothing, named per case so two
/// cases never race on one path.
fn empty_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sprefa_129_{label}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The flag is the whole switch: off it, no row on the wire says `checker`.
#[test]
fn checker_rows_need_the_flag() {
    let mut args = probe_args();
    let flag = args
        .iter()
        .position(|arg| *arg == "--go-checker")
        .expect("the flag is in the arg list");
    args.remove(flag);
    let census = origins(&facts(&args, None));
    let checker: usize = census
        .iter()
        .filter(|((_, origin), _)| origin == "checker")
        .map(|(_, count)| *count)
        .sum();
    assert_eq!(checker, 0, "the syntax leg alone names no checker: {census:?}");
}

#[cfg(feature = "go-checker")]
#[test]
fn the_tier_answers_call_and_type() {
    let rows = facts(&probe_args(), None);
    let census = origins(&rows);
    let calls = census
        .get(&("resolved_edge".to_string(), "checker".to_string()))
        .copied()
        .unwrap_or(0);
    let types = census
        .get(&("resolved_type_edge".to_string(), "checker".to_string()))
        .copied()
        .unwrap_or(0);
    assert!(calls > 0, "the tier answered no call site: {census:?}");
    assert!(types > 0, "the tier answered no type reference: {census:?}");

    let semantic: Vec<String> = of_record(&rows, "run")
        .into_iter()
        .filter(|run| run["mode"] == "semantic")
        .map(|run| word(run, "tool"))
        .collect();
    assert_eq!(semantic, vec!["go-types".to_string()], "one semantic run");
}

/// The walk's rows, not the parse's: an interface a struct satisfies is a
/// relation only a type checker can answer.
#[cfg(feature = "go-checker")]
#[test]
fn the_walk_emits_the_semantic_tsi_rows() {
    let rows = facts(&probe_args(), None);
    let census = relations(&rows);
    for relation in ["tsi.type", "tsi.name", "tsi.conforms", "tsi.origin"] {
        assert!(
            census.get(relation).copied().unwrap_or(0) > 0,
            "{relation} is missing from the walk: {census:?}"
        );
    }
    // Square and Circle both satisfy Drawer, and nothing else does.
    assert_eq!(
        census.get("tsi.conforms").copied().unwrap_or(0),
        2,
        "two structs satisfy the probe's one interface: {census:?}"
    );
    let names: Vec<String> = of_record(&rows, "fact")
        .into_iter()
        .filter(|row| word(row, "relation") == "tsi.name")
        .filter_map(|row| row["args"].as_array()?.get(1)?["text"].as_str().map(str::to_string))
        .collect();
    for spelling in ["Drawer", "Square", "Circle", "Tag"] {
        assert!(
            names.contains(&spelling.to_string()),
            "the checker spells every named type: {names:?}"
        );
    }
}

/// The tier that could not run says so IN THE STREAM, the shape
/// `tests/104_tier_decline_diagnostic.rs` fixed for its ts and rust twins.
#[test]
fn go_tier_off_path_is_a_diagnostic() {
    let empty = empty_dir("no_go");
    let rows = facts(&probe_args(), Some(&empty));
    let declined: Vec<&Value> = of_record(&rows, "diagnostic")
        .into_iter()
        .filter(|row| word(row, "relation").starts_with("tier."))
        .collect();
    assert_eq!(declined.len(), 1, "one declined tier, one row: {declined:?}");
    assert_eq!(declined[0]["run"], 0, "a decline is the syntax run's news");
    assert_eq!(word(declined[0], "relation"), "tier.go-types");
    let detail = word(declined[0], "detail");
    #[cfg(feature = "go-checker")]
    assert!(detail.contains("go"), "the reason names the driver: {detail}");
    #[cfg(not(feature = "go-checker"))]
    assert!(
        detail.contains("--features go-checker"),
        "the reason names the missing build: {detail}"
    );

    let semantic: Vec<&Value> = of_record(&rows, "run")
        .into_iter()
        .filter(|run| run["mode"] == "semantic")
        .collect();
    assert!(
        semantic.is_empty(),
        "a declined tier mints no run: {semantic:?}"
    );
}

/// A go corpus the tier answered keeps every row the syntax leg resolved: the
/// checker replaces destinations, it never deletes a corpus edge the compiler
/// also binds inside the corpus.
#[cfg(feature = "go-checker")]
#[test]
fn the_tier_loses_no_corpus_edge() {
    let mut args = probe_args();
    let flag = args
        .iter()
        .position(|arg| *arg == "--go-checker")
        .expect("the flag is in the arg list");
    args.remove(flag);
    let syntax = origins(&facts(&args, None));
    let checked = origins(&facts(&probe_args(), None));
    let total = |census: &BTreeMap<(String, String), usize>, record: &str| -> usize {
        census
            .iter()
            .filter(|((kind, _), _)| kind == record)
            .map(|(_, count)| *count)
            .sum()
    };
    for record in ["resolved_edge", "resolved_type_edge"] {
        assert_eq!(
            total(&checked, record),
            total(&syntax, record),
            "{record}: syntax {syntax:?} vs checker {checked:?}"
        );
    }
}
