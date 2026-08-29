//! Prolog meta-call closure and goal arguments: SWI `meta_predicate` knowledge
//! drives both the call sites and the `reference` positions inside meta-predicate
//! arguments. The fixture `corpus_1_meta_closures.pl` is the corpus repro: all
//! four meta slots (`maplist/3`, `call/3`, `forall/2`, `findall/3`) must reach
//! `double/2`, and a `--resolve` run over the split def/use pair must mint the
//! four go/1 -> double/2 edges.

use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/prolog/corpus_1_meta_closures.pl";
const DEF: &str = "tests/fixtures/prolog/corpus_2_meta_def.pl";
const USE: &str = "tests/fixtures/prolog/corpus_2_meta_use.pl";
const DIRECTIVE: &str = "tests/fixtures/prolog/corpus_3_meta_directive.pl";

fn raw(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs")
}

fn run(args: &[&str]) -> String {
    let output = raw(args);
    assert!(
        output.status.success(),
        "{args:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn rows(stdout: &str, record: &str, key: &str, value: &str) -> Vec<(String, String)> {
    stdout
        .lines()
        .filter_map(|line| {
            let value_json: serde_json::Value = serde_json::from_str(line).ok()?;
            if value_json.get("record")?.as_str()? != record {
                return None;
            }
            if value_json.get(key)?.as_str()? != value {
                return None;
            }
            Some((
                value_json
                    .get("position")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string(),
                value_json
                    .get("span")
                    .and_then(|s| s.get("start"))
                    .and_then(|s| s.as_u64())
                    .unwrap_or(0)
                    .to_string(),
            ))
        })
        .collect()
}

#[test]
fn closure_slots_mint_sites_and_closure_references_in_clause_order() {
    let stdout = run(&["--family", "call", FIXTURE]);
    let sites = rows(&stdout, "site", "callee", "double/2");
    assert_eq!(sites.len(), 4, "all four meta slots mint a double/2 site");
    let mut starts: Vec<u64> = sites.iter().map(|(_, s)| s.parse().unwrap()).collect();
    assert!(starts.windows(2).all(|w| w[0] < w[1]), "distinct spans in clause order: {starts:?}");

    let refs = rows(&stdout, "reference", "functor", "double/2");
    let positions: Vec<&str> = refs.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        positions, ["closure", "closure", "goal", "goal"],
        "maplist and call slots are closures, forall and findall slots are goals"
    );
}

#[test]
fn goal_slots_recursively_emit_goal_references() {
    let stdout = run(&["--family", "call", FIXTURE]);
    let member = rows(&stdout, "reference", "functor", "member/2");
    assert_eq!(
        member.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
        ["goal"],
        "forall/2 arg 1 is a goal"
    );
    let maplist = rows(&stdout, "reference", "functor", "maplist/3");
    assert_eq!(maplist.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(), ["goal"]);
}

#[test]
fn resolve_mints_go_to_double_edges_across_files() {
    let stdout = run(&["--resolve", DEF, USE]);
    let edges: Vec<(String, String)> = stdout
        .lines()
        .filter_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            if value.get("record")?.as_str()? != "resolved_edge" {
                return None;
            }
            Some((
                value.get("caller_name")?.as_str()?.to_string(),
                value.get("callee_name")?.as_str()?.to_string(),
            ))
        })
        .collect();
    let go_to_double = edges
        .iter()
        .filter(|(caller, callee)| caller == "go/1" && callee == "double/2")
        .count();
    assert_eq!(
        go_to_double, 4,
        "go/1 -> double/2 through maplist, call, forall, findall: {edges:?}"
    );
}

#[test]
fn file_meta_predicate_directive_drives_closure_slots() {
    let stdout = run(&["--family", "call", DIRECTIVE]);
    let sites = rows(&stdout, "site", "callee", "double2/2");
    assert_eq!(
        sites.len(),
        1,
        "apply_twice/3's `2` slot mints double2/2 from the bare atom"
    );
    let refs = rows(&stdout, "reference", "functor", "double2/2");
    assert_eq!(
        refs.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
        ["closure"],
        "the atom in the file-declared closure slot is a closure reference"
    );
}

#[test]
fn caret_wrapped_setof_goals_unwrap() {
    let stdout = run(&["--family", "call", DIRECTIVE]);
    let parent = rows(&stdout, "reference", "functor", "parent/2");
    assert_eq!(
        parent.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
        ["goal"],
        "setof arg 1 unwraps Template^Goal to a goal"
    );
}
