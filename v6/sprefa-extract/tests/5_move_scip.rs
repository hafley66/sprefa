//! `verify_import_refs` over `tests/fixtures/scip_move`: a committed
//! scip-typescript index cross-checked against `Rehome for TsSource`.
//!
//! @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
//! SABOTAGE: asserting only `disagreements.is_empty()` measured green against a
//! build whose site collection returned `Vec::new()` for every document, so the
//! green case ALSO asserts the four sites by importer and text, and the two
//! disagreement cases perturb that same set by exactly one row.
//!
//! MEASURED, scip-typescript 0.4.0: not one occurrence in this index carries
//! `SymbolRole.Import` (0x2) — `dist/src/FileIndexer.js:80` and `:214` are its
//! only `symbol_roles` writes and both write `Definition`. An IMPORT-role-only
//! rule reads ZERO sites here and `missed_by_impl` could never fire, which is
//! what `a_role_less_index_still_verifies` pins.

use std::path::{Path, PathBuf};

use sprefa_extract::{
    scip_import_sites, verify_import_refs, ImportRef, ImportRefKind, MoveCx, Rehome, ScipIndex,
    ScipSource, ScipTypescript, Span, TsSource, MISSED_BY_IMPL, UNKNOWN_TO_SCIP,
};

const MOVED: &str = "src/util.ts";
const DESTINATION: &str = "src/moved/util.ts";

/// Importer, the literal as written, and the target the index resolves it to.
/// `src/unrelated.ts -> ./other` is deliberately absent: neither end moves, so
/// the batch scope drops it on BOTH sides.
const EXPECTED_SITES: [(&str, &str, &str); 4] = [
    ("src/app.ts", "\"./util\"", "src/util.ts"),
    ("src/deep/nested.ts", "\"../util\"", "src/util.ts"),
    ("src/unicode.ts", "\"./util\"", "src/util.ts"),
    ("src/util.ts", "\"./shared\"", "src/shared.ts"),
];

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/scip_move")
        .canonicalize()
        .expect("fixture root")
}

/// The corpus view every case shares: one walk, one batch, no writes.
fn open() -> MoveCx {
    MoveCx::open(&fixture_root())
        .expect("open fixture")
        .with_batch(
            [(MOVED.to_string(), DESTINATION.to_string())]
                .into_iter()
                .collect(),
            false,
        )
}

fn index() -> ScipIndex {
    ScipTypescript
        .load(&fixture_root().join("fixture.scip"))
        .expect("load committed index")
}

fn report(disagreements: &[sprefa_extract::ScipDisagreement]) -> Vec<String> {
    disagreements
        .iter()
        .map(|row| {
            format!(
                "{} {}..{} {} {}",
                row.importer,
                row.span.start,
                row.span.end(),
                row.kind,
                row.detail
            )
        })
        .collect()
}

#[test]
fn the_index_and_the_ts_impl_agree_on_the_batch() {
    let cx = open();
    let index = index();

    let sites: Vec<(String, String, String)> = scip_import_sites(&cx, &index)
        .into_iter()
        .filter(|site| {
            site.target
                .as_deref()
                .is_some_and(|target| target == MOVED || site.importer == MOVED)
        })
        .map(|site| {
            (
                site.importer,
                site.text,
                site.target.unwrap_or_else(|| "?".to_string()),
            )
        })
        .collect();
    let expected: Vec<(String, String, String)> = EXPECTED_SITES
        .iter()
        .map(|(importer, text, target)| {
            (importer.to_string(), text.to_string(), target.to_string())
        })
        .collect();
    assert_eq!(sites, expected, "the index's in-batch specifier sites");

    let refs = TsSource.import_refs(&cx);
    let disagreements = verify_import_refs(&cx, &index, &refs);
    assert_eq!(report(&disagreements), Vec::<String>::new());
}

/// The committed index carries NO IMPORT bit and the check still reads all four
/// refs. Pins the arm the role bit cannot reach; an index that ever does set
/// IMPORT here fails this and stops pinning it.
/// @comment-ok: the property under test is an indexer fact no assert names
#[test]
fn a_role_less_index_still_verifies() {
    let index = index();
    let roles: Vec<i32> = index
        .documents
        .iter()
        .flat_map(|document| &document.occurrences)
        .map(|occurrence| occurrence.roles.0)
        .filter(|roles| !matches!(roles, 0 | 1))
        .collect();
    assert_eq!(roles, Vec::<i32>::new(), "roles outside {{0, 1}}");

    let cx = open();
    let refs = TsSource.import_refs(&cx);
    assert_eq!(scip_import_sites(&cx, &index).len(), 5, "every quoted site");
    assert_eq!(
        report(&verify_import_refs(&cx, &index, &refs)),
        Vec::<String>::new()
    );
}

#[test]
fn a_dropped_ref_reads_as_missed_by_impl() {
    let cx = open();
    let index = index();
    let mut refs = TsSource.import_refs(&cx);
    let dropped = refs
        .iter()
        .position(|reference| reference.importer == "src/deep/nested.ts")
        .expect("the nested importer's ref");
    let dropped = refs.remove(dropped);

    let disagreements = verify_import_refs(&cx, &index, &refs);
    assert_eq!(disagreements.len(), 1, "{:?}", report(&disagreements));
    assert_eq!(disagreements[0].kind, MISSED_BY_IMPL);
    assert_eq!(disagreements[0].importer, "src/deep/nested.ts");
    assert_eq!(disagreements[0].span.start, dropped.literal.start);
    assert!(
        disagreements[0]
            .detail
            .starts_with("\"../util\" -> src/util.ts"),
        "{}",
        disagreements[0].detail
    );
}

#[test]
fn a_ref_no_occurrence_covers_reads_as_unknown_to_scip() {
    let cx = open();
    let index = index();
    let mut refs = TsSource.import_refs(&cx);
    // The `import` keyword's own bytes: in an indexed document, in the batch,
    // and overlapping no occurrence the index carries.
    refs.push(ImportRef {
        importer: "src/app.ts".to_string(),
        literal: Span { start: 0, len: 6 },
        text: "\"./ghost\"".to_string(),
        target: MOVED.to_string(),
        kind: ImportRefKind::Import,
    });

    let disagreements = verify_import_refs(&cx, &index, &refs);
    assert_eq!(disagreements.len(), 1, "{:?}", report(&disagreements));
    assert_eq!(disagreements[0].kind, UNKNOWN_TO_SCIP);
    assert_eq!(disagreements[0].importer, "src/app.ts");
    assert_eq!(disagreements[0].span.start, 0);
    assert_eq!(disagreements[0].detail, "import \"./ghost\" -> src/util.ts");
}

#[test]
fn a_utf16_column_past_a_multibyte_char_lands_on_the_literal_bytes() {
    let cx = open();
    let index = index();
    let text = cx
        .text("src/unicode.ts")
        .expect("read the unicode importer");
    let at = text.find("\"./util\"").expect("the specifier literal");
    // The index writes columns in UTF-16 code units; four astral/CJK chars sit
    // before this literal, so a column read as bytes lands nine bytes short.
    let utf16_column = text[..at].encode_utf16().count();
    assert_eq!((at, utf16_column), (62, 53));

    let site = scip_import_sites(&cx, &index)
        .into_iter()
        .find(|site| site.importer == "src/unicode.ts")
        .expect("a site in the unicode importer");
    assert_eq!(site.span.start as usize, at);
    assert_eq!(site.text, "\"./util\"");
    assert_eq!(site.target.as_deref(), Some(MOVED));
}
