//! THE PARITY MATRIX, AS A TEST.
//!
//! `docs/v5-extraction-parity.md` maps every v5 built-in relation to a v6
//! record tag. Nothing kept that mapping from rotting: a renamed record, a
//! deleted one, or a language quietly gaining a plane all left the document
//! saying something that was true last month.
//!
//! v5's side is the CHECKED-IN oracle under `tests/fixtures/**/*.v5.jsonl`,
//! captured by `cargo run --example v5_normalize` in the v5 crate. THE v5
//! BINARY IS NEVER RUN HERE, and that is deliberate: "I DO NOT WANT TO RUN V5
//! ANYTHING ANYMORE" (CLAUDE.md). `golden_parity.rs` asserts the FACT equality
//! against those captures; this file asserts the mapping's shape.
//!
//! THREE LEGS.
//!
//!  1. SCHEMA REACH. Every record tag the matrix names must appear in this
//!     crate's own `SCHEMA` contract text. Renaming a record without touching
//!     the matrix turns this red.
//!
//!  2. PER-FILE EMISSION. Over one fixture per plane, `flatten(dispatch(..))`
//!     must really emit every record tag the matrix maps for the per-file
//!     families. A record that is documented and never produced is a lie the
//!     matrix would repeat.
//!
//!  3. ORACLE COVERAGE, enforced against the LIVE roster. Every `Source` in
//!     `sources()` sits in exactly one of two lists: it is graded against a
//!     captured v5 oracle, or it is v6-only with a written reason. A new
//!     `Source` belongs to neither until someone says which, so registering
//!     `PythonSource` (the one open language gap, @extract-python-arm) cannot
//!     land without either a captured oracle or a stated waiver.
//!
//! SABOTAGE RECEIPTS (all three run 2026-08-21, all three red, then reverted):
//!  - renaming `df_lit` to `df_literal` in one MATRIX row -> legs 1 and 2 red,
//!    "matrix row v5 `df_lit` names record tag `df_literal`, absent from SCHEMA".
//!  - deleting the `("rust", "rust")` row from `V5_ORACLE_LANGS` -> leg 3 red,
//!    "roster Source [\"rust\"] is in neither V5_ORACLE_LANGS nor
//!    V6_ONLY_LANGS. Capture a v5 oracle for it, or add a row with the reason
//!    it has no v5 twin."
//!  - swapping the rust and kotlin doc fixtures for one empty file -> leg 2
//!    red, "per-file plane never emitted these mapped records: [\"doc\",
//!    \"doc_tag\"]".

use std::collections::BTreeSet;
use std::path::PathBuf;

use sprefa_extract::{dispatch, flatten, sources, FamilyMask, SCHEMA};

fn repo_file(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

// ════════════════════════════════════════════════════════════════════════════
// THE MATRIX. One row per v5 built-in relation that docs/v5-extraction-parity.md
// scores `identical` or `superset` in an extraction plane, paired with the v6
// record tag that answers it. Rows the matrix scores `subset`, `missing` or
// `n/a` are NOT here: this table is the set of claims that must stay true.
// ════════════════════════════════════════════════════════════════════════════

/// `(v5 relation, v6 record tag, per-file)`. `per_file` marks the rows leg 2
/// can reach with `dispatch` alone; the rest need a whole-project mode and are
/// covered by leg 1 plus `4_capability_parity.rs`.
struct Row {
    v5: &'static str,
    record: &'static str,
    per_file: bool,
}

const fn per_file(v5: &'static str, record: &'static str) -> Row {
    Row {
        v5,
        record,
        per_file: true,
    }
}

const fn project(v5: &'static str, record: &'static str) -> Row {
    Row {
        v5,
        record,
        per_file: false,
    }
}

const MATRIX: &[Row] = &[
    // type plane
    per_file("type_entity", "node"),
    per_file("type_sig", "sig"),
    per_file("doc_comment", "doc"),
    per_file("doc_tag", "doc_tag"),
    per_file("const_value", "const"),
    project("type_edge", "resolved_type_edge"),
    // call plane
    per_file("call_def", "node"),
    per_file("call_site", "site"),
    per_file("call_name", "node"),
    project("call_edge", "resolved_edge"),
    // dataflow plane
    per_file("df_node", "node"),
    per_file("df_edge", "edge"),
    per_file("df_param", "param"),
    per_file("df_arg", "arg"),
    per_file("df_field", "df_field"),
    per_file("df_lit", "df_lit"),
    per_file("loop_over", "df_loop"),
    per_file("nest", "df_nest"),
    per_file("allocates", "df_allocates"),
    // module plane
    per_file("module_import", "specifier"),
    per_file("module_binding", "specifier"),
    project("module_edge", "file_edge"),
    project("module_unresolved", "file_unresolved"),
    project("crate_edge", "package_edge"),
    // scip plane
    project("scip_def", "scip_def"),
    project("scip_name", "scip_name"),
    project("scip_ref", "scip_ref"),
    project("scip_edge", "scip_edge"),
    project("scip_fn_edge", "scip_fn_edge"),
    project("scip_callee_type", "scip_callee_type"),
    project("scip_local", "scip_local"),
    project("scip_impl", "scip_impl"),
    project("scip_occurrence", "scip_occurrence"),
    project("scip_binding", "scip_occurrence"),
    // doc plane
    per_file("doc_node", "doc_node"),
    project("doc_ref", "resolved_type_edge"),
    // cst / spine plane
    per_file("node", "node"),
    per_file("child", "edge"),
    per_file("unresolved", "unresolved"),
    // corpus plane
    per_file("content", "file"),
    per_file("file_lines", "file"),
];

// ════════════════════════════════════════════════════════════════════════════
// LEG 1: every mapped record tag is in the crate's own contract text.
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn every_matrix_record_tag_is_in_the_schema_text() {
    let mut missing: Vec<String> = Vec::new();
    for row in MATRIX {
        let tag = format!("record={}", row.record);
        if !SCHEMA.contains(&tag) {
            missing.push(format!(
                "matrix row v5 `{}` names record tag `{}`, absent from SCHEMA",
                row.v5, row.record
            ));
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

// ════════════════════════════════════════════════════════════════════════════
// LEG 2: the per-file planes really emit what the matrix maps.
// ════════════════════════════════════════════════════════════════════════════

/// Every family off except the ones a row needs. `doc_node` is projected only
/// when the raw cst plane is NOT requested (`22_doc_node.rs:19-20`), so the
/// markdown row cannot ride `FamilyMask::ALL`.
const TYPES_ONLY: FamilyMask = FamilyMask {
    cst: false,
    types: true,
    call: false,
    df: false,
    data: false,
};

/// The fixture set is chosen so the UNION of its records covers every per-file
/// row: rust carries type/call/df/doc/const, `rust_modules` carries
/// `specifier`, kotlin carries `doc_tag`, ts carries the df aux rows and
/// `unresolved`, markdown carries `doc_node`.
const EMISSION_FIXTURES: &[(&str, FamilyMask)] = &[
    ("tests/fixtures/rust/sample.rs", FamilyMask::ALL),
    ("tests/fixtures/rust/docs.rs", FamilyMask::ALL),
    ("tests/fixtures/rust_modules/sample.rs", FamilyMask::ALL),
    ("tests/fixtures/kotlin/docs.kt", FamilyMask::ALL),
    ("tests/fixtures/ts/sample.ts", FamilyMask::ALL),
    ("tests/fixtures/ts/consts.ts", FamilyMask::ALL),
    ("tests/fixtures/ts/lambdas.ts", FamilyMask::ALL),
    ("tests/fixtures/df_loops/sample.ts", FamilyMask::ALL),
    ("tests/fixtures/df_loops/sample.rs", FamilyMask::ALL),
    (
        "tests/fixtures/ts_unresolved/unresolved.ts",
        FamilyMask::ALL,
    ),
    ("tests/fixtures/markdown/doc_node.md", TYPES_ONLY),
];

fn record_tags_over(fixtures: &[(&str, FamilyMask)]) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for (relative, mask) in fixtures {
        let path = repo_file(relative);
        let content =
            std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let Some(out) = dispatch(relative, &content, *mask) else {
            panic!("no Source claimed {relative}");
        };
        for fact in flatten(&out) {
            let value = serde_json::to_value(&fact).expect("a fact serializes");
            if let Some(tag) = value.get("record").and_then(serde_json::Value::as_str) {
                seen.insert(tag.to_string());
            }
        }
    }
    seen
}

#[test]
fn the_per_file_planes_emit_every_mapped_record() {
    let seen = record_tags_over(EMISSION_FIXTURES);
    let wanted: BTreeSet<&str> = MATRIX
        .iter()
        .filter(|row| row.per_file)
        .map(|row| row.record)
        // `file` rides --file-fact rather than the family mask, so it is
        // covered by 4_capability_parity.rs's binary leg, not by dispatch.
        .filter(|record| *record != "file")
        .collect();
    let absent: Vec<&&str> = wanted
        .iter()
        .filter(|record| !seen.contains(**record))
        .collect();
    assert!(
        absent.is_empty(),
        "per-file plane never emitted these mapped records: {absent:?}\nseen: {seen:?}"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// LEG 3: oracle coverage, enforced against the live roster.
// ════════════════════════════════════════════════════════════════════════════

/// Languages graded against a CAPTURED v5 oracle. Each name must have at least
/// one `tests/fixtures/<dir>/*.v5.jsonl`.
const V5_ORACLE_LANGS: &[(&str, &str)] = &[
    ("rust", "rust"),
    ("ts", "ts"),
    ("go", "go"),
    ("kotlin", "kotlin"),
];

/// Languages with NO v5 twin, each with the reason written down.
const V6_ONLY_LANGS: &[(&str, &str)] = &[
    (
        "markdown",
        "v5 had no markdown front-end; doc_node/doc_ref were engine-side",
    ),
    ("prolog", "v5 had no prolog front-end at all"),
    (
        "data",
        "v5 answered json/yaml/toml through the `json`/`jsonp` OPS, not a language",
    ),
    (
        "dl6",
        "v5's `dl` grammar was cst-only; the type/call planes are v6-native",
    ),
    (
        "astgrep",
        "the cst-only fallback; v5's `sg` roster has no per-language oracle",
    ),
];

#[test]
fn every_roster_source_is_graded_or_waived() {
    let graded: BTreeSet<&str> = V5_ORACLE_LANGS.iter().map(|(name, _)| *name).collect();
    let waived: BTreeSet<&str> = V6_ONLY_LANGS.iter().map(|(name, _)| *name).collect();

    let overlap: Vec<&&str> = graded.intersection(&waived).collect();
    assert!(
        overlap.is_empty(),
        "a Source is both graded and waived: {overlap:?}"
    );

    let unclassified: Vec<&'static str> = sources()
        .iter()
        .map(|source| source.name())
        .filter(|name| !graded.contains(name) && !waived.contains(name))
        .collect();
    assert!(
        unclassified.is_empty(),
        "roster Source {unclassified:?} is in neither V5_ORACLE_LANGS nor \
         V6_ONLY_LANGS. Capture a v5 oracle for it, or add a row with the \
         reason it has no v5 twin."
    );

    let live: BTreeSet<&str> = sources().iter().map(|source| source.name()).collect();
    let stale: Vec<&&str> = graded
        .union(&waived)
        .filter(|name| !live.contains(**name))
        .collect();
    assert!(stale.is_empty(), "these names left the roster: {stale:?}");
}

#[test]
fn every_graded_lang_has_a_captured_v5_oracle() {
    let mut empty: Vec<String> = Vec::new();
    for (name, dir) in V5_ORACLE_LANGS {
        let fixtures = repo_file(&format!("tests/fixtures/{dir}"));
        let captures = std::fs::read_dir(&fixtures)
            .unwrap_or_else(|error| panic!("read {}: {error}", fixtures.display()))
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".v5.jsonl"))
            .count();
        if captures == 0 {
            empty.push(format!("{name}: no *.v5.jsonl under tests/fixtures/{dir}"));
        }
    }
    assert!(empty.is_empty(), "{}", empty.join("\n"));
}
