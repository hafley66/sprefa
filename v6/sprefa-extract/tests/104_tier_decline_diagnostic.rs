//! A checker tier that was asked and answered nothing says so IN THE STREAM.
//!
//! SABOTAGE RECEIPT (base sha 39a5211a1): on it, `--witness --resolve
//! --ts-checker tests/fixtures/tsi/probe.ts` with `PATH` pointed at a directory
//! holding no `node` emits ZERO `record=diagnostic` rows and a plain syntax
//! stream. `load_ts_checker` returned `Option<Index>`, so the reason reached
//! `tracing::info!("ts checker tier off: {err}")` (src/project.rs:643, silent at
//! the default level) and nowhere else. Every case below sees an empty
//! diagnostic set there; `no_witness_emits_no_diagnostic` is the one guarding
//! the other direction.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const TSI_DIR: &str = "tests/fixtures/tsi";
const TS_PROBE: &str = "tests/fixtures/tsi/probe.ts";
const RUST_PROBE: &str = "tests/fixtures/tsi/probe.rs";

/// A scratch directory that exists and holds nothing, named per case so two
/// cases never race on one path.
fn empty_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sprefa_104_{label}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// One `extract` run from the crate root. `path` replaces `PATH` wholesale,
/// which is how a tier that shells out to `node` is taken off the machine.
fn facts(args: &[&str], path: Option<&PathBuf>, typescript: Option<String>) -> Vec<Value> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_extract"));
    command.current_dir(env!("CARGO_MANIFEST_DIR")).args(args);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    if let Some(typescript) = typescript {
        command.env("SPREFA_TS_CHECKER_TYPESCRIPT", typescript);
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

fn of_record<'a>(facts: &'a [Value], record: &str) -> Vec<&'a Value> {
    facts
        .iter()
        .filter(|fact| fact["record"] == record)
        .collect()
}

fn word(row: &Value, key: &str) -> String {
    row[key].as_str().unwrap_or_default().to_string()
}

fn ts_args() -> Vec<&'static str> {
    vec![
        "--witness",
        "--resolve",
        "--family",
        "type",
        "--project-root",
        TSI_DIR,
        "--ts-checker",
        TS_PROBE,
    ]
}

/// The hand-use case of 2026-09-03: `--ts-checker` with `node` off PATH used to
/// emit a plain syntax stream and say nothing about the tier it was asked for.
#[test]
fn ts_tier_off_path_is_a_diagnostic() {
    let empty = empty_dir("no_node");
    let facts = facts(&ts_args(), Some(&empty), None);
    let declined = of_record(&facts, "diagnostic");
    assert_eq!(
        declined.len(),
        1,
        "one declined tier, one row: {declined:?}"
    );
    assert_eq!(declined[0]["run"], 0, "a decline is the syntax run's news");
    assert_eq!(word(declined[0], "relation"), "tier.tsc");
    let detail = word(declined[0], "detail");
    #[cfg(feature = "ts-checker")]
    assert!(
        detail.contains("node"),
        "the reason names the driver: {detail}"
    );
    #[cfg(not(feature = "ts-checker"))]
    assert!(
        detail.contains("--features ts-checker"),
        "the reason names the missing build: {detail}"
    );

    let semantic: Vec<&Value> = of_record(&facts, "run")
        .into_iter()
        .filter(|run| run["mode"] == "semantic")
        .collect();
    assert!(
        semantic.is_empty(),
        "a declined tier mints no run: {semantic:?}"
    );
}

/// The rust twin. A root with no `Cargo.toml` above it is the cheapest decline
/// the tier can be forced into; without the feature it declines before that.
#[test]
fn rust_tier_off_is_a_diagnostic() {
    let root = empty_dir("no_cargo");
    let facts = facts(
        &[
            "--witness",
            "--resolve",
            "--family",
            "type",
            "--project-root",
            root.to_str().expect("utf8 path"),
            "--rust-checker",
            RUST_PROBE,
        ],
        None,
        None,
    );
    let declined = of_record(&facts, "diagnostic");
    assert_eq!(
        declined.len(),
        1,
        "one declined tier, one row: {declined:?}"
    );
    assert_eq!(declined[0]["run"], 0);
    assert_eq!(word(declined[0], "relation"), "tier.rust-analyzer");
    let detail = word(declined[0], "detail");
    #[cfg(feature = "rust-checker")]
    assert!(
        detail.contains("no cargo workspace"),
        "the reason names the missing workspace: {detail}"
    );
    #[cfg(not(feature = "rust-checker"))]
    assert!(
        detail.contains("--features rust-checker"),
        "the reason names the missing build: {detail}"
    );
}

/// The wire every consumer is already on: a decline is envelope news, so the
/// flag-off stream carries no record that was not there before.
#[test]
fn no_witness_emits_no_diagnostic() {
    let empty = empty_dir("no_node_flag_off");
    let mut args = ts_args();
    args.remove(0);
    let facts = facts(&args, Some(&empty), None);
    assert!(
        of_record(&facts, "diagnostic").is_empty(),
        "off --witness the reason reaches tracing and nowhere else"
    );
    assert!(
        of_record(&facts, "protocol").is_empty(),
        "off --witness there is no envelope to file a decline in"
    );
}

/// `diagnostic` was already on the wire, so the reverse door takes the decline
/// row without a decoder change and hands it back.
#[test]
fn the_declined_stream_survives_the_reverse_door() {
    let empty = empty_dir("no_node_ingest");
    let stream: Vec<String> = facts(&ts_args(), Some(&empty), None)
        .iter()
        .map(|row| serde_json::to_string(row).expect("a row re-serializes"))
        .collect();
    let raw = empty_dir("ingest").join("stream.jsonl");
    std::fs::write(&raw, stream.join("\n") + "\n").expect("write the stream");
    let landed = facts(&["--ingest", raw.to_str().expect("utf8 path")], None, None);
    let declined = of_record(&landed, "diagnostic");
    assert_eq!(declined.len(), 1, "the door kept the decline: {declined:?}");
    assert_eq!(word(declined[0], "relation"), "tier.tsc");
}

/// A `typescript` the driver can load, the way `tests/98_resolve_witness.rs`
/// finds one: a checkout's `lib/typescript.js` is the built compiler.
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

/// The other direction: a tier that LOADED is a semantic run, never a decline.
#[cfg(feature = "ts-checker")]
#[test]
fn a_loaded_tier_files_no_decline() {
    let facts = facts(&ts_args(), None, Some(typescript()));
    let tiers: Vec<String> = of_record(&facts, "diagnostic")
        .into_iter()
        .map(|row| word(row, "relation"))
        .filter(|relation| relation.starts_with("tier."))
        .collect();
    assert!(
        tiers.is_empty(),
        "a loaded tier declined nothing: {tiers:?}"
    );
    let semantic: Vec<String> = of_record(&facts, "run")
        .into_iter()
        .filter(|run| run["mode"] == "semantic")
        .map(|run| word(run, "tool"))
        .collect();
    assert_eq!(semantic, vec!["tsc".to_string()], "the tier answered");
}
