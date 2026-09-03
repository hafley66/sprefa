//! `scip_relationship.is_implementation`, which the extractor decoded and no
//! resolve ever read, as `resolved_type_edge` rows and `tsi.conforms` facts.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main
//! `9d49a35e4` the same `--resolve --family type --scip-index` run over
//! `tests/fixtures/scip_relationship/shapes.go` emits ZERO rows of kind
//! `implements`, zero of kind `overrides` and zero `tsi.conforms` facts, so
//! every case below sees an empty set. The whole-corpus receipt is
//! `go.oracle.type.kinds.tsv` kind=implements: 1.2% recall before, 92.36%
//! after (`plans/extract-bench-2026-08-29/OPEN-PROBLEMS.md` row 15).
//!
//! The fixture's `index.scip` is committed and built once with scip-go 0.2.7:
//! `cd tests/fixtures/scip_relationship && scip-go --output index.scip .`

use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const FIXTURE: &str = "tests/fixtures/scip_relationship";

fn fixture_dir() -> String {
    format!("{}/{FIXTURE}", env!("CARGO_MANIFEST_DIR"))
}

fn extract(args: &[&str]) -> Vec<FlatFact> {
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
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("every line is a FlatFact"))
        .collect()
}

/// The informed run: the fixture's own index in the loop.
fn informed(extra: &[&str]) -> Vec<FlatFact> {
    let dir = fixture_dir();
    let index = format!("{dir}/index.scip");
    let source = format!("{dir}/shapes.go");
    let mut args = vec![
        "--resolve",
        "--family",
        "type",
        "--project-root",
        &dir,
        "--scip-index",
        &index,
    ];
    args.extend_from_slice(extra);
    args.push(&source);
    extract(&args)
}

/// (owner_name, target_name) for every type edge of one kind.
fn edges_of_kind(rows: &[FlatFact], want: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = rows
        .iter()
        .filter_map(|row| match row {
            FlatFact::ResolvedTypeEdge {
                owner_name,
                target_name,
                kind,
                ..
            } if kind == want => Some((
                owner_name.clone().unwrap_or_default(),
                target_name.clone().unwrap_or_default(),
            )),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn facts_of(rows: &[FlatFact], relation: &str) -> Vec<FactOut> {
    rows.iter()
        .filter_map(|row| match row {
            FlatFact::Fact(fact) if fact.relation == relation => Some(fact.clone()),
            _ => None,
        })
        .collect()
}

/// The name a `tsi.name` row binds to an id, for printing a conforms pair.
fn name_of(rows: &[FlatFact], id: u32) -> Option<String> {
    facts_of(rows, "tsi.name").into_iter().find_map(|fact| {
        match (fact.args.first(), fact.args.get(1)) {
            (Some(Arg::Id(named)), Some(Arg::Text(text))) if *named == id => Some(text.clone()),
            _ => None,
        }
    })
}

#[test]
fn both_implementors_reach_the_interface() {
    let rows = informed(&[]);
    assert_eq!(
        edges_of_kind(&rows, "implements"),
        vec![
            ("Cat".to_string(), "Speaker".to_string()),
            ("Dog".to_string(), "Speaker".to_string()),
        ]
    );
}

#[test]
fn a_type_satisfying_nothing_mints_no_row() {
    let rows = informed(&[]);
    assert!(
        !edges_of_kind(&rows, "implements")
            .iter()
            .any(|(owner, _)| owner == "Mute"),
        "Mute satisfies no interface and must not appear"
    );
}

/// A CALLABLE pair is one method satisfying another. The go/types type-edge
/// oracle carries only the type-level pairs, so the two kinds stay apart.
#[test]
fn method_pairs_take_the_overrides_kind() {
    let rows = informed(&[]);
    let overrides = edges_of_kind(&rows, "overrides");
    assert_eq!(overrides, vec![("Speak".to_string(), "Speak".to_string())]);
    assert!(
        !edges_of_kind(&rows, "implements").contains(&("Speak".to_string(), "Speak".to_string())),
        "a method pair is never an implements row"
    );
}

#[test]
fn every_implements_row_names_the_scip_leg() {
    let rows = informed(&[]);
    let origins: Vec<&String> = rows
        .iter()
        .filter_map(|row| match row {
            FlatFact::ResolvedTypeEdge {
                kind,
                resolution_origin,
                ..
            } if kind == "implements" || kind == "overrides" => Some(resolution_origin),
            _ => None,
        })
        .collect();
    assert!(!origins.is_empty());
    assert!(origins.iter().all(|origin| *origin == "scip"), "{origins:?}");
}

#[test]
fn the_owner_span_is_the_type_name_as_written() {
    let rows = informed(&[]);
    let bytes = std::fs::read(format!("{}/shapes.go", fixture_dir())).expect("the fixture reads");
    for row in &rows {
        let FlatFact::ResolvedTypeEdge {
            owner_name,
            owner_start,
            owner_end,
            kind,
            ..
        } = row
        else {
            continue;
        };
        if kind != "implements" {
            continue;
        }
        let slice = &bytes[*owner_start as usize..*owner_end as usize];
        assert_eq!(
            std::str::from_utf8(slice).expect("the span is UTF-8"),
            owner_name.clone().unwrap_or_default(),
            "the span must cut the name the row carries"
        );
    }
}

#[test]
fn witness_mode_carries_the_pairs_as_tsi_conforms() {
    let rows = informed(&["--witness"]);
    let mut pairs: Vec<(String, String, String)> = facts_of(&rows, "tsi.conforms")
        .into_iter()
        .filter_map(|fact| match (fact.args.first(), fact.args.get(1), fact.args.get(2)) {
            (Some(Arg::Id(owner)), Some(Arg::Id(target)), Some(Arg::Atom(leg))) => Some((
                name_of(&rows, *owner)?,
                name_of(&rows, *target)?,
                leg.clone(),
            )),
            _ => None,
        })
        .collect();
    pairs.sort();
    pairs.dedup();
    assert_eq!(
        pairs,
        vec![
            ("Cat".to_string(), "Speaker".to_string(), "scip".to_string()),
            ("Dog".to_string(), "Speaker".to_string(), "scip".to_string()),
        ]
    );
}

/// Every id a `tsi.conforms` row names is DECLARED by a `tsi.type` row of its
/// own: an argument naming an id nothing declares is an unclosed stream.
#[test]
fn every_conforms_id_is_declared() {
    let rows = informed(&["--witness"]);
    let declared: Vec<u32> = facts_of(&rows, "tsi.type")
        .into_iter()
        .filter_map(|fact| match fact.args.first() {
            Some(Arg::Id(id)) => Some(*id),
            _ => None,
        })
        .collect();
    let conforms = facts_of(&rows, "tsi.conforms");
    assert!(!conforms.is_empty());
    for fact in conforms {
        for arg in &fact.args {
            if let Arg::Id(id) = arg {
                assert!(declared.contains(id), "id {id} is named and never declared");
            }
        }
    }
}

/// No index in the loop, no implements rows: the leg answers from the index
/// alone and never guesses.
#[test]
fn a_plain_resolve_mints_no_implements_row() {
    let dir = fixture_dir();
    let source = format!("{dir}/shapes.go");
    let rows = extract(&["--resolve", "--family", "type", &source]);
    assert!(edges_of_kind(&rows, "implements").is_empty());
    assert!(edges_of_kind(&rows, "overrides").is_empty());
    assert!(facts_of(&rows, "tsi.conforms").is_empty());
}
