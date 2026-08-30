//! THE SCIP FAMILY CARRIES THE RELATIONSHIP TABLE.
//!
//! `scip_impl` is the interface hop the v5 family already projected; the raw
//! `scip_relationship` row is the UNPROJECTION: every relationship with its
//! four flags, including the `is_reference` + `is_implementation` override
//! pairs scip.proto's own worked example carries. The three live resolve arms
//! read none of it today (measured, SCIP.REPORT.md); the family stream is the
//! contract that lets a dl rule or a future arm consume it without touching the
//! decode.
//!
//! The fixture's `fixture.scip` is COMMITTED, on the scip_move precedent:
//! `Metadata.project_root` and the symbols bake in whoever ran the indexer, so
//! the test asserts symbol SUFFIXES (the descriptor tail scip itself defines),
//! never whole symbols. Rebuild from this directory:
//!
//! ```
//! scip-typescript index --output fixture.scip
//! ```
//!
//! | indexer | version | when |
//! |---|---|---|
//! | scip-typescript | 0.4.0 | 2026-08-29 |
//!
//! The test routes the committed index through the CLI's own discovery seam:
//! `SPREFA_SCIP_INDEX` names the file, `--family scip` reuses it and never
//! shells out to an indexer, so this test is hermetic and offline.

use std::path::PathBuf;
use std::process::Command;

const ROOT: &str = "tests/fixtures/scip_relationship";
const FIXTURE: &str = "tests/fixtures/scip_relationship/fixture.scip";

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sprefa-scip-relationship-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn records<'a>(stream: &'a str, kind: &str) -> Vec<&'a str> {
    let tag = format!("{{\"record\":\"{kind}\",");
    stream
        .lines()
        .filter(|line| line.starts_with(&tag))
        .collect()
}

fn scip_family() -> String {
    let cache = scratch();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--family")
        .arg("scip")
        .arg("--scip-cache")
        .arg(&cache)
        .arg(ROOT)
        .env("SPREFA_SCIP_INDEX", FIXTURE)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

#[test]
fn the_family_stream_emits_the_relationship_rows_the_impl_hop_flattens() {
    let stream = scip_family();
    let rels = records(&stream, "scip_relationship");
    assert!(
        !rels.is_empty(),
        "--family scip emitted no scip_relationship rows; the relationship \
         table is being flattened into scip_impl before the wire"
    );
    // scip.proto's worked example: Dog implements Animal, and the method pair
    // carries BOTH flags. The suffixes are the descriptor tails the spec
    // defines, so the assertion survives the fixture's absolute-path prefixes.
    let dog_sound = rels
        .iter()
        .any(|row| row.contains("Dog#sound()") && row.contains("Animal#sound()"));
    assert!(
        dog_sound,
        "the Dog#sound() -> Animal#sound() relationship row is missing: {rels:?}"
    );
    let both_flags = rels.iter().any(|row| {
        row.contains("\"is_implementation\":true") && row.contains("\"is_reference\":true")
    });
    assert!(
        both_flags,
        "no scip_relationship row carries the override pair (is_reference + is_implementation)"
    );
}
