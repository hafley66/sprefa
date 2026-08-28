//! Prolog meta-call edges: `once/1` argument 1 and `catch/3` arguments 1 and 3
//! are statically callable goals, so their named compounds become call sites
//! and resolved edges. Ordinary compound data in term arguments (`helper(X)`
//! under `process/1`) stays a `term_arg` reference with no call edge.
//!
//! The unit half pins the site and reference positions of
//! `fixtures/prolog/2_metacall.pl`; the CLI half pins the cross-file resolve
//! shape over `3_metacall_caller.pl` -> `4_metacall_target.pl`.

use sprefa_extract::{flatten, FamilyMask, PrologSource, Source};
use std::process::Command;

const METACALL: &str = "tests/fixtures/prolog/2_metacall.pl";
const METACALL_BYTES: &[u8] = include_bytes!("fixtures/prolog/2_metacall.pl");
const CALLER: &str = "tests/fixtures/prolog/3_metacall_caller.pl";
const TARGET: &str = "tests/fixtures/prolog/4_metacall_target.pl";

fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

#[test]
fn metacall_arguments_become_call_sites_in_clause_order() {
    let output = PrologSource.extract(METACALL, METACALL_BYTES, FamilyMask::ALL);
    let facts = flatten(&output);
    let sites: Vec<&str> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Site { callee, .. } => Some(callee.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        sites,
        [
            "helper/0",
            "once/1",
            "once/1",
            "helper/0",
            "catch/3",
            "helper/0",
            "true/0",
            "catch/3",
            "fail/0",
            "helper/0",
            "process/1",
        ]
    );
}

#[test]
fn metacall_data_arguments_stay_term_arg_references() {
    let output = PrologSource.extract(METACALL, METACALL_BYTES, FamilyMask::ALL);
    let facts = flatten(&output);
    let helper0: Vec<&str> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Reference {
                functor, position, ..
            } if functor == "helper/0" => Some(position.as_str()),
            _ => None,
        })
        .collect();
    // direct, nested once, catch goal, catch recovery: four goal positions.
    assert_eq!(helper0, ["goal", "goal", "goal", "goal"]);
    let helper1: Vec<&str> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Reference {
                functor, position, ..
            } if functor == "helper/1" => Some(position.as_str()),
            _ => None,
        })
        .collect();
    // `process(helper(X))` is compound data, never a goal.
    assert_eq!(helper1, ["term_arg"]);
}

#[test]
fn resolve_emits_metacall_edges_intra_file() {
    let stdout = run(&["--resolve", "--family", "call", METACALL]);
    let edges: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            if value.get("record")?.as_str()? != "resolved_edge" {
                return None;
            }
            Some(format!(
                "{} -> {}",
                value.get("caller_name")?.as_str()?,
                value.get("callee_name")?.as_str()?
            ))
        })
        .collect();
    // Only goals with in-file definitions resolve; `once/1`, `catch/3`,
    // `true/0`, `fail/0`, and `process/1` have no local clauses.
    assert_eq!(
        edges,
        [
            "catch_protect/0 -> helper/0",
            "catch_recover/0 -> helper/0",
            "direct/0 -> helper/0",
            "nested_once/0 -> helper/0",
        ]
    );
}

#[test]
fn resolve_carries_metacall_edges_across_files() {
    let stdout = run(&["--resolve", "--family", "call", CALLER, TARGET]);
    let edges: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            if value.get("record")?.as_str()? != "resolved_edge" {
                return None;
            }
            Some(format!(
                "{}:{} -> {}:{}",
                value.get("caller_path")?.as_str()?,
                value.get("caller_name")?.as_str()?,
                value.get("callee_path")?.as_str()?,
                value.get("callee_name")?.as_str()?
            ))
        })
        .collect();
    assert_eq!(
        edges,
        ["tests/fixtures/prolog/3_metacall_caller.pl:cross/0 -> \
          tests/fixtures/prolog/4_metacall_target.pl:target/0"
            .replace("          ", "")]
    );
}
