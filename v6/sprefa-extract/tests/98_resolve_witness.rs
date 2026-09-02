//! `--witness` over `--resolve`: every leg that named a target is a witness on
//! the fact, and a leg that named another def is a fact of its own.
//!
//! SABOTAGE RECEIPT (base sha 8e050ed82): `--witness --resolve` was a clap
//! conflict, `error: the argument '--resolve' cannot be used with '--witness'`,
//! rc=2, and `ProjectEdge` carried no `witnesses` field, so the four checker
//! folds dropped every syntax leg that answered beside the checker.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const RESOLVE_DIR: &str = "tests/fixtures/resolve";
#[cfg(feature = "ts-checker")]
const TSI_DIR: &str = "tests/fixtures/tsi";

/// One `extract` run from the crate root, stdout as raw lines.
fn lines(args: &[&str], typescript: Option<String>) -> Vec<String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_extract"));
    command.current_dir(env!("CARGO_MANIFEST_DIR")).args(args);
    if let Some(path) = typescript {
        command.env("SPREFA_TS_CHECKER_TYPESCRIPT", path);
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
        .map(str::to_string)
        .collect()
}

fn facts(args: &[&str], typescript: Option<String>) -> Vec<Value> {
    lines(args, typescript)
        .iter()
        .map(|line| serde_json::from_str(line).expect("one json fact per line"))
        .collect()
}

fn of_record<'a>(facts: &'a [Value], record: &str) -> Vec<&'a Value> {
    facts
        .iter()
        .filter(|fact| fact["record"] == record)
        .collect()
}

/// The two ts files the syntax-only cases resolve. `helper` is corpus-unique
/// there, so the name-match leg is the only one that answers.
const SYNTAX_ARGS: &[&str] = &[
    "--resolve",
    "--family",
    "call",
    "--project-root",
    RESOLVE_DIR,
    "tests/fixtures/resolve/0_caller.ts",
    "tests/fixtures/resolve/1_callee.ts",
];

fn witnessed_syntax_args() -> Vec<&'static str> {
    let mut args = vec!["--witness"];
    args.extend_from_slice(SYNTAX_ARGS);
    args
}

#[test]
fn the_flag_off_stream_is_the_committed_golden() {
    let golden = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/resolve/2_resolved_edges.jsonl"),
    )
    .expect("the golden is committed");
    let produced = lines(SYNTAX_ARGS, None).join("\n") + "\n";
    assert_eq!(produced, golden, "a resolve with no --witness is unchanged");
}

#[test]
fn the_protocol_row_opens_a_witnessed_resolve() {
    let rows = lines(&witnessed_syntax_args(), None);
    let head: Vec<Value> = rows[..2]
        .iter()
        .map(|line| serde_json::from_str(line).expect("json"))
        .collect();
    assert_eq!(head[0]["record"], "protocol", "got {}", rows[0]);
    assert_eq!(head[1]["record"], "run", "got {}", rows[1]);
    assert_eq!(head[1]["run"], 0);
    assert_eq!(head[1]["mode"], "syntax");
    assert_eq!(head[1]["tool"], "extract");
    let decoded: Vec<Value> = rows
        .iter()
        .map(|line| serde_json::from_str(line).expect("json"))
        .collect();
    let semantic: Vec<&Value> = of_record(&decoded, "run")
        .into_iter()
        .filter(|run| run["mode"] == "semantic")
        .collect();
    assert!(
        semantic.is_empty(),
        "no checker flag means no semantic run, got {semantic:?}"
    );
}

/// Every numbered row carries exactly the legs that named it, and with no
/// checker loaded that is one leg: the row's own `resolution_origin`.
#[test]
fn one_witness_per_leg_on_a_syntax_run() {
    let facts = facts(&witnessed_syntax_args(), None);
    let edges = of_record(&facts, "resolved_edge");
    let witnesses = of_record(&facts, "witness");
    assert!(!edges.is_empty(), "the fixture resolves at least one call");
    assert_eq!(
        edges.len(),
        witnesses.len(),
        "one leg per row on a syntax-only run"
    );
    for edge in &edges {
        let ordinal = edge["fact"].as_u64().expect("every resolved row is numbered");
        let mine: Vec<&&Value> = witnesses
            .iter()
            .filter(|witness| witness["fact"].as_u64() == Some(ordinal))
            .collect();
        assert_eq!(mine.len(), 1, "fact {ordinal} carries one witness");
        assert_eq!(mine[0]["run"], 0);
        assert_eq!(
            mine[0]["method"], edge["resolution_origin"],
            "the method IS the leg the row names"
        );
    }
}

#[test]
fn the_syntax_run_covers_both_families_partially() {
    let facts = facts(&witnessed_syntax_args(), None);
    let mut covered: Vec<(u64, String, String)> = of_record(&facts, "coverage")
        .iter()
        .map(|row| {
            (
                row["run"].as_u64().unwrap_or_default(),
                row["relation"].as_str().unwrap_or_default().to_string(),
                row["coverage"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    covered.sort();
    assert_eq!(
        covered,
        vec![
            (0, "extract.call".to_string(), "partial".to_string()),
            (0, "extract.type".to_string(), "partial".to_string()),
        ],
        "a resolve enumerates no relation, so neither family is complete"
    );
    assert!(
        of_record(&facts, "diagnostic").is_empty(),
        "partial coverage carries no diagnostic"
    );
}

#[test]
fn the_witnessed_stream_survives_the_reverse_door() {
    let scratch = std::env::temp_dir().join("sprefa_a2_resolve_witness");
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let raw = scratch.join("stream.jsonl");
    std::fs::write(&raw, lines(&witnessed_syntax_args(), None).join("\n") + "\n")
        .expect("write the stream");
    let once = lines(&["--ingest", raw.to_str().expect("utf8 path")], None);
    let canonical = scratch.join("once.jsonl");
    std::fs::write(&canonical, once.join("\n") + "\n").expect("write the canonical form");
    let twice = lines(&["--ingest", canonical.to_str().expect("utf8 path")], None);
    assert_eq!(once, twice, "the reverse door is idempotent");
}

// ════════════════════════════════════════════════════════════════════════════
// The CHECKER tier: two tiers, so a fact can carry two witnesses.
// ════════════════════════════════════════════════════════════════════════════

/// A `typescript` the driver can load, the way `tests/92_ts_checker.rs` finds
/// one: a checkout's `lib/typescript.js` is the built compiler.
#[cfg(feature = "ts-checker")]
fn typescript() -> String {
    if let Ok(pinned) = std::env::var("SPREFA_TS_CHECKER_TYPESCRIPT") {
        return pinned;
    }
    let root = std::env::var("RATCHET_TS_ROOT")
        .unwrap_or_else(|_| "/Users/chrishafley/projects/TypeScript-5.9".to_string());
    let built = PathBuf::from(&root).join("lib/typescript.js");
    assert!(
        built.is_file(),
        "no typescript for the checker tier: set SPREFA_TS_CHECKER_TYPESCRIPT to a \
         typescript.js, or RATCHET_TS_ROOT to a TypeScript checkout (tried {})",
        built.display()
    );
    built.to_string_lossy().into_owned()
}

/// `agree.ts` imports what it calls and what it extends, so the module plane
/// and the checker land on the same definition at every site.
#[cfg(feature = "ts-checker")]
fn agree_facts() -> Vec<Value> {
    facts(
        &[
            "--witness",
            "--ts-checker",
            "--resolve",
            "--family",
            "call,type",
            "--project-root",
            TSI_DIR,
            "tests/fixtures/tsi/agree.ts",
            "tests/fixtures/tsi/agree_callee.ts",
        ],
        Some(typescript()),
    )
}

#[cfg(feature = "ts-checker")]
#[test]
fn a_loaded_checker_mints_its_own_run() {
    let facts = agree_facts();
    let mut runs: Vec<(u64, String, String)> = of_record(&facts, "run")
        .iter()
        .map(|row| {
            (
                row["run"].as_u64().unwrap_or_default(),
                row["mode"].as_str().unwrap_or_default().to_string(),
                row["tool"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    runs.sort();
    assert_eq!(
        runs,
        vec![
            (0, "syntax".to_string(), "extract".to_string()),
            (1, "semantic".to_string(), "tsc".to_string()),
        ],
        "one run per tier that ran"
    );
    // The resolve legs enumerate no relation, so run 0 covers only the two
    // families partially; the checker WALK is a claim of its own and rides run 1.
    for row in of_record(&facts, "coverage") {
        let relation = row["relation"].as_str().unwrap_or_default();
        let expected = if relation.starts_with("extract.") { 0 } else { 1 };
        assert_eq!(row["run"], expected, "{relation} is covered by the wrong run");
    }
}

/// `run` calls the imported `helper`. The checker owns the row, the module
/// plane reached the same definition, so ONE fact carries TWO witnesses.
#[cfg(feature = "ts-checker")]
#[test]
fn the_checker_and_a_syntax_leg_witness_one_fact() {
    let facts = agree_facts();
    let edge = of_record(&facts, "resolved_edge")
        .into_iter()
        .find(|row| row["caller_name"] == "run" && row["callee_name"] == "helper")
        .expect("the agreeing call row is in the stream");
    assert_eq!(edge["resolution_origin"], "checker");
    let ordinal = edge["fact"].as_u64().expect("the row is numbered");
    let mut mine: Vec<(u64, String)> = of_record(&facts, "witness")
        .iter()
        .filter(|row| row["fact"].as_u64() == Some(ordinal))
        .map(|row| {
            (
                row["run"].as_u64().unwrap_or_default(),
                row["method"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    mine.sort();
    assert_eq!(
        mine,
        vec![
            (0, "module_plane".to_string()),
            (1, "checker".to_string()),
        ],
        "the import binding rides the syntax run, the checker its own"
    );
}

/// `drive` declares a `render` that shadows the import it also carries. The
/// checker names the local one; the module plane names the imported one.
#[cfg(feature = "ts-checker")]
#[test]
fn a_disagreeing_leg_is_a_fact_of_its_own() {
    let facts = facts(
        &[
            "--witness",
            "--ts-checker",
            "--resolve",
            "--family",
            "call",
            "--project-root",
            TSI_DIR,
            "tests/fixtures/tsi/disagree.ts",
            "tests/fixtures/tsi/disagree_callee.ts",
        ],
        Some(typescript()),
    );
    let edges = of_record(&facts, "resolved_edge");
    let mut sited: Vec<(String, String)> = edges
        .iter()
        .filter(|row| row["caller_site_start"].as_u64() == Some(152))
        .map(|row| {
            (
                row["resolution_origin"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                row["callee_path"]
                    .as_str()
                    .unwrap_or_default()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    sited.sort();
    assert_eq!(
        sited,
        vec![
            (
                "checker".to_string(),
                "disagree.ts".to_string()
            ),
            (
                "module_plane".to_string(),
                "disagree_callee.ts".to_string()
            ),
        ],
        "one site, two definitions, two rows; a hosts.rs consumer reads each \
         row's own resolution_origin"
    );
    let ordinals: Vec<u64> = edges
        .iter()
        .filter(|row| row["caller_site_start"].as_u64() == Some(152))
        .filter_map(|row| row["fact"].as_u64())
        .collect();
    assert_eq!(ordinals.len(), 2, "two rows, two ordinals");
    assert_ne!(ordinals[0], ordinals[1], "disagreement is never one fact");
    for ordinal in ordinals {
        let mine = of_record(&facts, "witness")
            .into_iter()
            .filter(|row| row["fact"].as_u64() == Some(ordinal))
            .count();
        assert_eq!(mine, 1, "a leg that agreed with nobody witnesses alone");
    }
}
