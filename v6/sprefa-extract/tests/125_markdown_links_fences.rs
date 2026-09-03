//! Markdown `doc_node` link, image and code_block rows, pinned against two
//! hand-counted fixtures. Every expected row is a literal: spans are byte
//! offsets into the fixture, counted by hand.
//!
//! tree-sitter-md node kinds read (`tree-sitter-md-0.5.3`, node-types.json):
//! inline grammar `inline_link`, `full_reference_link`,
//! `collapsed_reference_link`, `shortcut_link`, `uri_autolink`,
//! `email_autolink`, `image` (with `image_description`, `link_destination`,
//! `link_label`, `link_title`); block grammar `link_reference_definition`,
//! `fenced_code_block` (`info_string` > `language`, `code_fence_content`),
//! `indented_code_block`.

use std::collections::BTreeSet;

use sprefa_extract::{dispatch, flatten_jsonl, FamilyMask};

const LINKS_PATH: &str = "v6/sprefa-extract/tests/fixtures/markdown/links.md";
const LINKS: &[u8] = include_bytes!("fixtures/markdown/links.md");
const FENCES_PATH: &str = "v6/sprefa-extract/tests/fixtures/markdown/fences.md";
const FENCES: &[u8] = include_bytes!("fixtures/markdown/fences.md");

/// Markdown projects `doc_nodes` onto the types plane only when the raw cst
/// plane is not requested (`22_doc_node.rs`).
const TYPES_ONLY: FamilyMask = FamilyMask {
    cst: false,
    types: true,
    call: false,
    df: false,
    data: false,
};

/// links.md by hand: 10 links, 2 images, 0 rows for the undefined `[nope][missing]`.
/// Inline (one, two), reference full/collapsed/shortcut (three, ref-b, ref-c,
/// the `[REF-C]` definition matching case-insensitively), autolinks (four,
/// five: name and target both the bracketed text), the outer link around an
/// image (seven, name = the raw image markup), a link inside a heading
/// (eight), a reference link inside a list item (nine). Images: six and the
/// nested seven, each with its own row. `title` appears only when written.
const EXPECTED_LINK_ROWS: &[&str] = &[
    r#"{"record":"doc_node","family":"type","span":{"start":16,"end":42},"kind":"link","name":"one","parent":"Links","target":"https://one.example"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":47,"end":69},"kind":"link","name":"two","parent":"Links","target":"two.md","title":"Second"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":87,"end":101},"kind":"link","name":"three","parent":"Links","target":"https://a.example"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":113,"end":122},"kind":"link","name":"ref-b","parent":"Links","target":"https://b.example","title":"B Title"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":133,"end":140},"kind":"link","name":"ref-c","parent":"Links","target":"https://c.example"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":180,"end":202},"kind":"link","name":"https://four.example","parent":"Links","target":"https://four.example"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":207,"end":226},"kind":"link","name":"mail@five.example","parent":"Links","target":"mail@five.example"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":235,"end":260},"kind":"image","name":"alt six","parent":"Links","target":"six.png","title":"Six"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":278,"end":326},"kind":"link","name":"[alt seven](seven.png)","parent":"Links","target":"https://seven.example"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":279,"end":302},"kind":"image","name":"alt seven","parent":"Links","target":"seven.png"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":339,"end":356},"kind":"link","name":"eight","parent":"Nested [eight](eight.md)","target":"eight.md"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":365,"end":378},"kind":"link","name":"nine","parent":"Nested [eight](eight.md)","target":"https://a.example"}"#,
];

/// fences.md by hand: 7 code_block rows. Backtick and tilde fences with a
/// language (rust, python) and without (two empty names), one indented block
/// (empty name, body = the whole block including its trailing blank line, the
/// span tree-sitter-md gives `indented_code_block`), a fence whose info string
/// carries words after the language (`js title` -> `js`), and an empty fence
/// with no `body`.
const EXPECTED_FENCE_ROWS: &[&str] = &[
    r#"{"record":"doc_node","family":"type","span":{"start":10,"end":34},"kind":"code_block","name":"rust","parent":"Fences","body":{"start":18,"end":30}}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":35,"end":62},"kind":"code_block","name":"python","parent":"Fences","body":{"start":45,"end":58}}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":63,"end":77},"kind":"code_block","name":"","parent":"Fences","body":{"start":67,"end":73}}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":78,"end":91},"kind":"code_block","name":"","parent":"Fences","body":{"start":82,"end":87}}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":92,"end":126},"kind":"code_block","name":"","parent":"Fences","body":{"start":92,"end":126}}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":126,"end":146},"kind":"code_block","name":"js","parent":"Fences","body":{"start":138,"end":142}}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":147,"end":155},"kind":"code_block","name":"","parent":"Fences"}"#,
];

fn rows_of_kind(path: &str, content: &[u8], kinds: &[&str]) -> BTreeSet<String> {
    let out = dispatch(path, content, TYPES_ONLY).expect("markdown source");
    flatten_jsonl(&out)
        .into_iter()
        .filter(|row| {
            kinds
                .iter()
                .any(|kind| row.contains(&format!("\"kind\":\"{kind}\"")))
        })
        .collect()
}

fn count_kind(rows: &BTreeSet<String>, kind: &str) -> usize {
    rows.iter()
        .filter(|row| row.contains(&format!("\"kind\":\"{kind}\"")))
        .count()
}

#[test]
fn link_and_image_rows_match_hand_count() {
    let rows = rows_of_kind(LINKS_PATH, LINKS, &["link", "image"]);
    assert_eq!(count_kind(&rows, "link"), 10, "links.md carries 10 links");
    assert_eq!(count_kind(&rows, "image"), 2, "links.md carries 2 images");
    let expected: BTreeSet<String> = EXPECTED_LINK_ROWS.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        rows, expected,
        "link/image row set diverges from the hand-derived expectation"
    );
}

/// The undefined reference `[nope][missing]` mints no row: its bytes 143..157
/// appear in no link span.
#[test]
fn undefined_reference_mints_no_row() {
    let rows = rows_of_kind(LINKS_PATH, LINKS, &["link", "image"]);
    assert!(
        !rows.iter().any(|row| row.contains("\"name\":\"nope\"")),
        "an undefined reference label projected a link row:\n{rows:?}"
    );
}

#[test]
fn code_block_rows_match_hand_count() {
    let rows = rows_of_kind(FENCES_PATH, FENCES, &["code_block"]);
    assert_eq!(rows.len(), 7, "fences.md carries 7 code blocks");
    let expected: BTreeSet<String> = EXPECTED_FENCE_ROWS.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        rows, expected,
        "code_block row set diverges from the hand-derived expectation"
    );
}

/// Heading rows stay byte-identical to their pre-link shape: no `target`,
/// `title` or `body` key is serialized when absent.
#[test]
fn heading_rows_carry_no_optional_keys() {
    let rows = rows_of_kind(LINKS_PATH, LINKS, &["heading"]);
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert!(
            !row.contains("\"target\"") && !row.contains("\"title\"") && !row.contains("\"body\""),
            "heading row grew an optional key: {row}"
        );
    }
}
