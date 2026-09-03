//! The syntax tier's type graph rides `--witness --resolve`, not the per-file
//! `--family` stream alone.
//!
//! SABOTAGE RECEIPT (base sha 39a5211a1): on it, `--witness --resolve --family
//! type --project-root tests/fixtures/tsi tests/fixtures/tsi/probe.ts` (no
//! checker flag) emits ZERO `tsi.*` facts. The A4 rows were written into
//! `bundle.aux.tsi` and read only by `flatten_type` (src/wire.rs:296), so a
//! resolve stream with the checker tier declined carried no type graph at all.
//! Every case below sees an empty `tsi.*` set there.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const TSI_DIR: &str = "tests/fixtures/tsi";
const TS_PROBE: &str = "tests/fixtures/tsi/probe.ts";
const RUST_PROBE: &str = "tests/fixtures/tsi/probe.rs";

fn lines(args: &[&str]) -> Vec<String> {
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
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// Every line as the typed record it is. A line the decoder rejects is a
/// producer defect, so the parse panics rather than skipping.
fn extract(args: &[&str]) -> Vec<FlatFact> {
    lines(args)
        .iter()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("line does not decode: {line}\n{error}"))
        })
        .collect()
}

/// The whole-project door over one file, with no checker flag: the case the
/// hand use hit.
fn resolve_args(fixture: &str) -> Vec<&str> {
    vec![
        "--witness",
        "--resolve",
        "--family",
        "type",
        "--project-root",
        TSI_DIR,
        fixture,
    ]
}

fn resolved(fixture: &str) -> Vec<FlatFact> {
    extract(&resolve_args(fixture))
}

/// The per-file door over the same file, which is where these rows already rode.
fn per_file(fixture: &str) -> Vec<FlatFact> {
    extract(&["--witness", "--family", "type", fixture])
}

/// The `tsi.*` and language-native rows as a set, ordinals dropped: a fact's
/// ordinal is stream-local and the two doors number different row sets.
fn tsi_set(rows: &[FlatFact]) -> BTreeSet<String> {
    rows.iter()
        .filter_map(|row| match row {
            FlatFact::Fact(fact) => Some(fact),
            _ => None,
        })
        .map(|fact| {
            format!(
                "{}({})",
                fact.relation,
                fact.args
                    .iter()
                    .map(|arg| serde_json::to_string(arg).expect("an arg serializes"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect()
}

fn coverage(rows: &[FlatFact]) -> BTreeMap<String, (u32, bool)> {
    rows.iter()
        .filter_map(|row| match row {
            FlatFact::Coverage(claim) => {
                Some((claim.relation.clone(), (claim.run, claim.complete)))
            }
            _ => None,
        })
        .collect()
}

/// The claim of the arc: the resolve door carries exactly the type graph the
/// per-file door carries, span digests and ids included.
#[test]
fn resolve_carries_the_syntax_tsi_rows() {
    for fixture in [TS_PROBE, RUST_PROBE] {
        let door = tsi_set(&resolved(fixture));
        let file = tsi_set(&per_file(fixture));
        assert!(!file.is_empty(), "{fixture} spells no tsi row at all");
        let missing: Vec<&String> = file.difference(&door).collect();
        let extra: Vec<&String> = door.difference(&file).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{fixture}\n  only per-file: {missing:?}\n  only resolve: {extra:?}"
        );
    }
}

/// A row with no ordinal is a row no witness can name, and the door renumbers
/// from the ordinals it is handed.
#[test]
fn every_syntax_tsi_fact_carries_an_ordinal() {
    for fixture in [TS_PROBE, RUST_PROBE] {
        // A witness names another row's ordinal in the same key, so the record
        // word is what separates a numbered row from a reference to one.
        let mut ordinals: Vec<u64> = lines(&resolve_args(fixture))
            .iter()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("a row is JSON"))
            .filter(|row| row["record"] != "witness")
            .filter_map(|row| row["fact"].as_u64())
            .collect();
        ordinals.sort_unstable();
        let dense: Vec<u64> = (1..=ordinals.len() as u64).collect();
        assert_eq!(ordinals, dense, "{fixture}: the ordinals are not 1..n");
    }
}

/// A resolve enumerates no relation, so every relation it names is partial and
/// rides the syntax run it came out of.
#[test]
fn syntax_tsi_coverage_is_partial() {
    for fixture in [TS_PROBE, RUST_PROBE] {
        let rows = resolved(fixture);
        let covered = coverage(&rows);
        let named: BTreeSet<String> = rows
            .iter()
            .filter_map(|row| match row {
                FlatFact::Fact(fact) => Some(fact.relation.clone()),
                _ => None,
            })
            .collect();
        assert!(!named.is_empty(), "{fixture} spells no tsi row at all");
        for relation in &named {
            assert_eq!(
                covered.get(relation),
                Some(&(0u32, false)),
                "{fixture}: {relation} has no partial run-0 coverage row"
            );
        }
        for (relation, (run, complete)) in &covered {
            assert!(!complete, "{fixture}: a parse claimed complete {relation}");
            assert_eq!(*run, 0, "{fixture}: {relation} is covered by run {run}");
            if relation.starts_with("extract.") {
                continue;
            }
            assert!(
                named.contains(relation),
                "{fixture}: coverage for unemitted {relation}"
            );
        }
    }
}

/// The reverse door is the consumer that enforces the registry and the id
/// closure, so a row it rejects is a producer defect.
#[test]
fn the_resolve_stream_survives_the_reverse_door() {
    for (label, fixture) in [("ts", TS_PROBE), ("rust", RUST_PROBE)] {
        let scratch = std::env::temp_dir().join("sprefa_105_resolve_tsi");
        std::fs::create_dir_all(&scratch).expect("scratch dir");
        let raw = scratch.join(format!("{label}.jsonl"));
        std::fs::write(&raw, lines(&resolve_args(fixture)).join("\n") + "\n")
            .expect("write the stream");
        let once = lines(&["--ingest", raw.to_str().expect("utf8 path")]);
        assert!(
            !tsi_set(&extract(&["--ingest", raw.to_str().expect("utf8 path")])).is_empty(),
            "{fixture}: the door dropped every tsi row"
        );
        let canonical = scratch.join(format!("{label}_once.jsonl"));
        std::fs::write(&canonical, once.join("\n") + "\n").expect("write the canonical form");
        let twice = lines(&["--ingest", canonical.to_str().expect("utf8 path")]);
        assert_eq!(once, twice, "{fixture}: the reverse door is not idempotent");
    }
}

/// Two files in one resolve is where the per-file id spaces would collide: an
/// id is a stream coordinate, so no two types may take one number.
#[test]
fn two_files_never_share_one_type_id() {
    let rows = extract(&[
        "--witness",
        "--resolve",
        "--family",
        "type",
        "--project-root",
        TSI_DIR,
        TS_PROBE,
        RUST_PROBE,
    ]);
    let facts: Vec<&FactOut> = rows
        .iter()
        .filter_map(|row| match row {
            FlatFact::Fact(fact) => Some(fact),
            _ => None,
        })
        .collect();
    let mut declared: BTreeMap<u32, usize> = BTreeMap::new();
    for fact in &facts {
        if fact.relation != "tsi.origin" {
            continue;
        }
        if let Some(Arg::Id(id)) = fact.args.first() {
            *declared.entry(*id).or_default() += 1;
        }
    }
    assert!(!declared.is_empty(), "the two files declare no type at all");
    let shared: Vec<(&u32, &usize)> = declared.iter().filter(|(_, seen)| **seen > 1).collect();
    assert!(shared.is_empty(), "one id, two origins: {shared:?}");
    let ts = tsi_set(&resolved(TS_PROBE)).len();
    let rust = tsi_set(&resolved(RUST_PROBE)).len();
    assert!(
        facts.len() >= ts + rust,
        "the pair stream lost rows: {} rows for {ts} + {rust}",
        facts.len()
    );
}
