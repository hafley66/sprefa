//! THE `data` FAMILY, one case per grammar, byte-diffed against a committed
//! golden and cross-checked against the fixture bytes at one span each.
//!
//! FAIL-FIRST, recorded before the family existed: `extract --family data x.json`
//! printed `Error: "--family 'data' is not a mask family; per-file families are
//! cst, type, call, df, cfg"` and exited 1, and `.toml` produced zero lines under
//! every flag.
//!
//! SABOTAGE RECEIPT: dropping the `[[table]]` element index from the toml arm of
//! `entries` makes `bin.0.name` and `bin.1.name` both read `bin.name`, and the
//! toml golden goes red on four lines.
// @comment-ok: the fail-first and sabotage lines are TEST-header receipts

use std::process::Command;
use std::time::Instant;

const BIN: &str = env!("CARGO_BIN_EXE_extract");

struct Case {
    source: &'static str,
    golden: &'static str,
    docs: usize,
    values: usize,
    /// One (byte start, byte end, expected slice) triple, checked against the
    /// fixture's own bytes so a span is proven to address the value it claims.
    span: (usize, usize, &'static str),
}

const CASES: &[Case] = &[
    Case {
        source: "tests/fixtures/data/nested.json",
        golden: include_str!("fixtures/data/goldens/nested.json.jsonl"),
        docs: 1,
        values: 14,
        span: (151, 160, "list_pets"),
    },
    Case {
        source: "tests/fixtures/data/stream.yaml",
        golden: include_str!("fixtures/data/goldens/stream.yaml.jsonl"),
        docs: 2,
        values: 21,
        span: (204, 211, "get_pet"),
    },
    Case {
        source: "tests/fixtures/data/tables.toml",
        golden: include_str!("fixtures/data/goldens/tables.toml.jsonl"),
        docs: 1,
        values: 18,
        span: (219, 235, "src/bin/probe.rs"),
    },
    Case {
        source: "tests/fixtures/data/lines.jsonl",
        golden: include_str!("fixtures/data/goldens/lines.jsonl.jsonl"),
        docs: 3,
        values: 16,
        span: (134, 142, "squirtle"),
    },
];

fn run(args: &[&str]) -> String {
    let output = Command::new(BIN)
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("JSONL is UTF-8")
}

#[test]
fn every_grammar_answers_its_golden_byte_for_byte() {
    for case in CASES {
        let facts = run(&["--family", "data", case.source]);
        assert_eq!(facts, case.golden, "{} rows", case.source);
        assert_eq!(
            facts
                .lines()
                .filter(|line| line.contains(r#""record":"data_doc""#))
                .count(),
            case.docs,
            "{} data_doc rows",
            case.source
        );
        assert_eq!(
            facts
                .lines()
                .filter(|line| line.contains(r#""record":"data_value""#))
                .count(),
            case.values,
            "{} data_value rows",
            case.source
        );
    }
}

#[test]
fn every_asserted_span_addresses_the_value_it_claims() {
    for case in CASES {
        let bytes = std::fs::read(case.source).expect("fixture reads");
        let (start, end, expected) = case.span;
        assert_eq!(
            std::str::from_utf8(&bytes[start..end]).expect("fixture is UTF-8"),
            expected,
            "{} span {start}..{end}",
            case.source
        );
        let facts = run(&["--family", "data", case.source]);
        assert!(
            facts.contains(&format!(r#""span":{{"start":{start},"end":{end}}}"#)),
            "{} emits no row at {start}..{end}",
            case.source
        );
    }
}

/// The `.json` and `.yaml` extensions produced ast-grep cst rows before the data
/// family took over their roster slot, so the data source delegates that plane
/// back and the rows must be unchanged.
#[test]
fn taking_the_roster_slot_kept_the_cst_plane() {
    let facts = run(&["--family", "cst", "tests/fixtures/data/nested.json"]);
    assert!(
        facts.lines().count() > 0 && facts.contains(r#""family":"cst""#),
        "the json cst plane went missing when the data family took the roster slot"
    );
    assert!(
        !facts.contains(r#""family":"data""#),
        "--family cst must not turn the data plane on"
    );
}

/// The corpus spec: 100 operations, every one reached through a `paths.*.*`
/// dotted path. The golden is the operationId slice, not the whole 9822-row
/// stream, so the diff stays readable.
#[test]
fn the_pokeapi_spec_answers_every_operation_id_under_the_ten_second_law() {
    let source = "../dl/fixtures/pokeapi.openapi.yml";
    let started = Instant::now();
    let facts = run(&["--family", "data", source]);
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "extracting the corpus spec took {elapsed:?}, over the 10-second law"
    );
    assert_eq!(
        facts
            .lines()
            .filter(|line| line.contains(r#""record":"data_doc""#))
            .count(),
        1,
        "the spec is one yaml document"
    );
    let operation_ids: String = facts
        .lines()
        .filter(|line| line.contains(".operationId\""))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(
        operation_ids,
        include_str!("fixtures/data/goldens/pokeapi.operation_ids.jsonl"),
        "the spec's operationId rows"
    );
}

/// A `data_doc` row carries the document as a json VALUE, which is the column
/// dl6's `decode/2` brace pattern reads. Nothing else in the stream does.
#[test]
fn the_document_row_carries_a_readable_json_value() {
    let facts = run(&["--family", "data", "tests/fixtures/data/stream.yaml"]);
    let docs: Vec<serde_json::Value> = facts
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("row is json"))
        .filter(|row| row["record"] == "data_doc")
        .collect();
    assert_eq!(docs.len(), 2);
    assert_eq!(
        docs[0]["doc"]["paths"]["/pets"]["get"]["operationId"],
        serde_json::json!("list_pets")
    );
    assert_eq!(
        docs[1]["doc"]["paths"]["/pets/{id}"]["get"]["operationId"],
        serde_json::json!("get_pet")
    );
    assert_eq!(docs[1]["doc"]["count"], serde_json::json!(7));
    assert_eq!(docs[1]["doc"]["enabled"], serde_json::json!(true));
    assert_eq!(docs[1]["doc"]["missing"], serde_json::Value::Null);
}

/// An unknown `--family` name is a named stop, never a silent empty mask.
#[test]
fn an_unknown_family_name_still_stops_by_name() {
    let output = Command::new(BIN)
        .args(["--family", "datum", "tests/fixtures/data/nested.json"])
        .output()
        .expect("extract binary runs");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cst, type, call, df, data, cfg"),
        "the mask error must name the data family: {stderr}"
    );
}
