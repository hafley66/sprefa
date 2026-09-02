//! `--witness`: the TSI envelope on the per-file wire, and `FlatFact` decoding.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main `15e95de83`
//! this file does not compile. `round_trip_every_golden_line` calls
//! `serde_json::from_str::<FlatFact>`, and `FlatFact` derived `Serialize`
//! alone, so the decode half of the wire had no implementation to call. The
//! other cases fail at run time on the same tree: `--witness` is not a flag,
//! so clap exits 2 and every stream below is empty.
//!
//! `flag_off_is_the_old_wire` is the one that catches a regression in the
//! other direction: it asserts that with the flag off no row carries a `fact`
//! key and the row set is the same one the goldens under
//! `tests/fixtures/resolve/` were recorded from.

use std::collections::BTreeMap;
use std::process::Command;

use serde_json::Value;
use sprefa_extract::FlatFact;

/// The fixture: one ts file whose type family is a single declaration, so the
/// envelope is readable in a failure print.
const FIXTURE: &str = "tests/fixtures/resolve/0_caller.ts";

fn run(args: &[&str]) -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
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
        .map(str::to_string)
        .collect()
}

fn rows(args: &[&str]) -> Vec<Value> {
    run(args)
        .iter()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

/// Object keys sorted at every depth, so a comparison is over content and not
/// over whichever order two serializers picked.
fn key_sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> = map
                .iter()
                .map(|(key, inner)| (key.clone(), key_sorted(inner)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(key_sorted).collect()),
        other => other.clone(),
    }
}

fn record(row: &Value) -> &str {
    row["record"].as_str().expect("every row is tagged")
}

/// Criterion 1: a consumer learns the protocol before it reads a fact.
#[test]
fn protocol_is_the_first_row() {
    let rows = rows(&["--witness", "--family", "type", FIXTURE]);
    let first: FlatFact =
        serde_json::from_value(rows[0].clone()).expect("the first row decodes as a flat fact");
    assert!(
        matches!(first, FlatFact::Protocol { version: 1 }),
        "first row was {:?}",
        rows[0]
    );
}

/// Criterion 4, the identify half: the run names its mode, its tool and the
/// bytes it read.
#[test]
fn run_is_the_second_row() {
    let rows = rows(&["--witness", "--family", "type", FIXTURE]);
    let second = &rows[1];
    assert_eq!(record(second), "run");
    assert_eq!(second["mode"], "syntax");
    assert_eq!(second["tool"], "extract");
    let scope = second["scope"].as_array().expect("scope is a list");
    assert!(!scope.is_empty(), "run row carries no scope: {second}");
    assert!(
        second["version"].as_str().is_some_and(|v| !v.is_empty()),
        "run row carries no version: {second}"
    );
}

/// The flag-off path is the wire every consumer is already on. Two claims: no
/// row carries a `fact` key, and the rows are the same ones the envelope wraps.
#[test]
fn flag_off_is_the_old_wire() {
    let plain = rows(&[FIXTURE]);
    for row in &plain {
        assert!(
            row.get("fact").is_none(),
            "the flag is off and a row carries a fact key: {row}"
        );
    }
    let envelope = ["protocol", "run", "witness", "coverage", "diagnostic"];
    let witnessed: Vec<Value> = rows(&["--witness", FIXTURE])
        .into_iter()
        .filter(|row| !envelope.contains(&record(row)))
        .map(|mut row| {
            row.as_object_mut()
                .expect("a row is an object")
                .remove("fact");
            row
        })
        .collect();
    assert_eq!(plain, witnessed);
}

/// Criterion 2. Every golden line and every envelope line decodes to a
/// `FlatFact` and re-encodes to the same object.
#[test]
fn round_trip_every_golden_line() {
    let mut lines = run(&["--witness", FIXTURE]);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/resolve");
    let mut goldens = 0usize;
    for entry in std::fs::read_dir(&dir).expect("the resolve fixture dir is readable") {
        let path = entry.expect("a dir entry").path();
        if path.extension().is_none_or(|ext| ext != "jsonl") {
            continue;
        }
        goldens += 1;
        let body = std::fs::read_to_string(&path).expect("a golden is readable");
        lines.extend(body.lines().map(str::to_string));
    }
    assert!(goldens > 0, "no golden jsonl under {}", dir.display());
    for line in &lines {
        let decoded: FlatFact = serde_json::from_str(line)
            .unwrap_or_else(|error| panic!("line does not decode: {line}\n{error}"));
        let before: Value = serde_json::from_str(line).expect("a golden line is JSON");
        let after = serde_json::to_value(&decoded).expect("a flat fact re-encodes");
        assert_eq!(key_sorted(&before), key_sorted(&after), "line: {line}");
    }
}

/// Every numbered row is claimed by exactly one witness, and a syntax run's
/// only method is the parse.
#[test]
fn one_witness_per_numbered_row() {
    let rows = rows(&["--witness", FIXTURE]);
    let numbered: Vec<u64> = rows
        .iter()
        .filter_map(|row| row.get("fact").filter(|_| record(row) != "witness"))
        .map(|fact| fact.as_u64().expect("a fact ordinal is a number"))
        .collect();
    let witnesses: Vec<&Value> = rows.iter().filter(|row| record(row) == "witness").collect();
    assert!(
        !numbered.is_empty(),
        "the fixture produced no numbered rows"
    );
    assert_eq!(numbered.len(), witnesses.len());
    for witness in &witnesses {
        let fact = witness["fact"].as_u64().expect("a witness names a fact");
        assert!(
            numbered.contains(&fact),
            "witness names fact {fact}, which no row carries"
        );
        assert_eq!(witness["method"], "parse");
        assert_eq!(witness["run"], 0);
    }
}

/// Criterion 4, the coverage half: a parse enumerates nothing exhaustively, so
/// every relation it touched is partial and there is nothing to diagnose.
#[test]
fn syntax_coverage_is_partial_and_undiagnosed() {
    let rows = rows(&["--witness", FIXTURE]);
    let coverage: Vec<&Value> = rows
        .iter()
        .filter(|row| record(row) == "coverage")
        .collect();
    assert!(
        !coverage.is_empty(),
        "no coverage row in a witnessed stream"
    );
    for row in &coverage {
        assert_eq!(row["coverage"], "partial", "row: {row}");
        assert!(
            row["relation"]
                .as_str()
                .is_some_and(|name| name.contains('.')),
            "a coverage relation is <ns>.<name>: {row}"
        );
    }
    assert!(
        !rows.iter().any(|row| record(row) == "diagnostic"),
        "a syntax run emitted a diagnostic"
    );
}

/// `--schema` is the contract a foreign producer reads. The six envelope
/// records are spelled there exactly as the protocol spells them.
#[test]
fn schema_declares_the_envelope_records() {
    let schema = run(&["--schema"]).join("\n");
    for line in [
        "record=protocol  version=<u32>",
        "record=run       run=<u32> mode=syntax|semantic tool=<slug> version=<string> scope=[<digest>...]",
        "record=fact      fact=<u32> relation=<ns.name> args=[<arg>...]",
        "record=witness   fact=<u32> run=<u32> method=<slug>",
        "record=coverage  run=<u32> relation=<ns.name> coverage=partial|complete",
        "record=diagnostic run=<u32> relation=<ns.name> detail=<string>",
    ] {
        assert!(schema.contains(line), "--schema is missing `{line}`");
    }
}

/// The cfg plane is derived after the flatten, so its rows would sit past the
/// coverage rows unnumbered. A named stop beats a half-witnessed stream.
#[test]
fn cfg_under_witness_is_a_named_stop() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--witness", "--family", "cfg", FIXTURE])
        .output()
        .expect("extract binary runs");
    assert!(!output.status.success(), "--witness --family cfg succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--witness does not cover --family cfg"),
        "stop is unnamed: {stderr}"
    );
}
