//! The `--scip-facts` and `--file-fact` contracts.
//!
//! These two modes exist to close v5 relations that were inexpressible from a v6
//! index (spelunk section 3): the ten-relation `scip_*` family, and `file_lines`
//! plus the size half of `content`.
//!
//! Both goldens PIN JSONL field names. The v6 host decodes by top-level key, so
//! a rename here is a breaking change and has to show up as a diff.
//!
//! The SCIP test runs the real scip-typescript, matching the ratchet law in
//! golden_parity.rs: a missing indexer fails loudly rather than skipping to a
//! green that means nothing.

use std::process::Command;

const SCIP_REL_ROOT: &str = "tests/fixtures/scip_rel";
const SCIP_REL_SOURCE: &str = "tests/fixtures/scip_rel/animal.ts";
const SCIP_REL_GOLDEN: &str = include_str!("fixtures/scip_rel/expected.jsonl");

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

/// The whole `--scip-facts` stream over the scip.proto worked example, minus
/// the one machine-dependent row. The golden pins the field names of every
/// record kind the fixture produces.
///
/// The two scip_relationship rows are the point. The diet DROPPED relationships
/// before this lane, which is why v5's `scip_impl` and the `scip_edge` family
/// had no v6 input at all. `Dog#` implements `Animal#`, and `Dog#sound()` both
/// references and implements `Animal#sound()`; that is the exact pair the spec
/// documents.
#[test]
fn scip_facts_stream_occurrences_symbols_and_relationships() {
    let facts = scip_rel_facts(&[]);
    assert_eq!(without_metadata(&facts), SCIP_REL_GOLDEN);

    let relationships = facts
        .lines()
        .filter(|line| line.contains("\"record\":\"scip_relationship\""))
        .count();
    assert_eq!(
        relationships, 2,
        "the implements pair from scip.proto's own worked example must survive \
         the decode; dropping relationships is what made scip_impl inexpressible"
    );
}

/// PASSTHROUGH, asserted field by field on the occurrence row. Before this
/// lane the row carried `roles` and `definition` and nothing else, so six of
/// scip.proto's seven SymbolRole bits, the syntax kind and the enclosing range
/// were unreachable from the language no matter what a rule asked for.
///
/// The enclosing range is the discriminating one: `Dog#`'s definition encloses
/// bytes 48..136, the whole class including its body, which is a fact no other
/// row in the stream carries.
#[test]
fn occurrence_rows_carry_every_role_bit_and_the_enclosing_range() {
    let facts = scip_rel_facts(&[]);
    let dog = facts
        .lines()
        .find(|line| line.contains("`animal.ts`/Dog#\"") && line.contains("scip_occurrence"))
        .expect("the Dog# definition occurrence");
    for field in [
        "\"definition\":true",
        "\"import\":false",
        "\"write_access\":false",
        "\"read_access\":false",
        "\"generated\":false",
        "\"test\":false",
        "\"forward_definition\":false",
        "\"syntax_kind\":0",
        "\"enclosing_start\":48",
        "\"enclosing_end\":136",
    ] {
        assert!(dog.contains(field), "missing {field} in {dog}");
    }
}

/// The metadata row is asserted here rather than in the golden because
/// `project_root` is an absolute path and would pin the checkout location.
/// Everything else about it is machine-independent and IS pinned: the tool
/// identity is what a ledger entry needs to say which indexer release produced
/// a fact, and it reached no consumer at all before this lane.
#[test]
fn scip_facts_carry_the_index_metadata_row() {
    let facts = scip_rel_facts(&[]);
    let metadata = facts
        .lines()
        .find(|line| line.contains("\"record\":\"scip_metadata\""))
        .expect("exactly one metadata row");
    for field in [
        "\"tool_name\":\"scip-typescript\"",
        "\"tool_version\":\"0.4.0\"",
        "\"tool_arguments\":[]",
        "\"text_document_encoding\":1",
    ] {
        assert!(metadata.contains(field), "missing {field} in {metadata}");
    }
    assert!(
        metadata.contains("tests/fixtures/scip_rel"),
        "project_root must name the indexed corpus, got {metadata}"
    );
}

/// Symbol documentation is passthrough too: scip-typescript renders a hover
/// block per symbol and the diet dropped all of it. One row per entry with a
/// `pos` keeps multi-entry documentation in order.
#[test]
fn scip_facts_carry_symbol_documentation() {
    let facts = scip_rel_facts(&[]);
    assert!(
        facts.contains(
            r#"{"record":"scip_documentation","symbol":"scip-typescript npm . . `animal.ts`/Dog#","pos":0,"text":"```ts\nclass Dog\n```"}"#
        ),
        "the Dog# docstring must reach the wire: {facts}"
    );
}

/// `--scip-record` is the demand-side lever for the measured cost of full
/// passthrough. It filters BEFORE the rows are built, and the assertion that
/// matters is that asking for one kind yields only that kind.
#[test]
fn scip_record_narrows_the_stream_to_the_requested_kinds() {
    let narrowed = scip_rel_facts(&["--scip-record", "scip_relationship"]);
    assert_eq!(narrowed.lines().count(), 2, "got: {narrowed}");
    assert!(narrowed
        .lines()
        .all(|line| line.contains("\"record\":\"scip_relationship\"")));

    let two = scip_rel_facts(&["--scip-record", "scip_metadata,scip_document"]);
    assert_eq!(two.lines().count(), 2, "got: {two}");
}

/// An unknown record kind is an ERROR. A silent drop would print an empty
/// stream, which reads as "this index has no such facts" and is a worse lie
/// than a failure.
#[test]
fn an_unknown_scip_record_kind_is_a_named_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args([
            "--scip-facts",
            "--scip-record",
            "scip_occurrances",
            "--project-root",
            SCIP_REL_ROOT,
            "--scip-build",
            SCIP_REL_SOURCE,
        ])
        .output()
        .expect("extract binary runs");
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        message.contains("scip_occurrances") && message.contains("scip_occurrence"),
        "the error must name the typo and the known kinds, got: {message}"
    );
}

/// Diagnostics and signature documentation are the two passthrough legs
/// scip-typescript never exercises, so they are pinned against a hand-built
/// index instead of an indexer run. Without this the field names would ship
/// unasserted and could drift silently.
///
/// A signature's occurrence ranges are relative to the SIGNATURE TEXT, not to
/// any document, which is why they land on their own record. `sound(): string`
/// has `string` at bytes 9..15 of that text, and that is what is asserted.
#[test]
fn diagnostics_and_signatures_reach_the_wire() {
    use sprefa_extract::{
        flatten_scip, OccurrenceRole, ScipDiagnostic, ScipDocument, ScipIndex, ScipOccurrence,
        ScipSignature, ScipSymbolInfo,
    };

    let occurrence = ScipOccurrence {
        symbol: "Dog#sound().".to_string(),
        range: [0, 0, 0, 5],
        roles: OccurrenceRole::DEFINITION,
        syntax_kind: 7,
        enclosing_range: None,
        override_documentation: vec!["narrowed to string".to_string()],
        diagnostics: vec![ScipDiagnostic {
            severity: 2,
            code: "TS7006".to_string(),
            message: "implicitly has an any type".to_string(),
            source: "typescript".to_string(),
            tags: vec![1],
        }],
    };
    let symbol = ScipSymbolInfo {
        symbol: "Dog#sound().".to_string(),
        display_name: "sound".to_string(),
        kind: 26,
        relationships: Vec::new(),
        documentation: Vec::new(),
        signature: Some(ScipSignature {
            language: "typescript".to_string(),
            text: "sound(): string".to_string(),
            occurrences: vec![ScipOccurrence {
                symbol: "string#".to_string(),
                range: [0, 9, 0, 15],
                roles: OccurrenceRole::default(),
                syntax_kind: 0,
                enclosing_range: None,
                override_documentation: Vec::new(),
                diagnostics: Vec::new(),
            }],
        }),
        enclosing_symbol: "Dog#".to_string(),
    };
    let index = ScipIndex {
        documents: vec![ScipDocument {
            relative_path: "animal.ts".to_string(),
            occurrences: vec![occurrence],
            symbols: vec![symbol],
            ..ScipDocument::default()
        }],
        ..ScipIndex::default()
    };
    let reader = |_: &str| Some(b"sound(): string\n".to_vec());
    let lines: Vec<String> = flatten_scip(&index, &reader)
        .iter()
        .map(|fact| serde_json::to_string(fact).expect("serializable"))
        .collect();
    let stream = lines.join("\n");

    for expected in [
        r#"{"record":"scip_occurrence_doc","path":"animal.ts","start":0,"end":5,"pos":0,"text":"narrowed to string"}"#,
        r#"{"record":"scip_diagnostic","path":"animal.ts","start":0,"end":5,"severity":2,"code":"TS7006","message":"implicitly has an any type","source":"typescript","tags":[1]}"#,
        r#"{"record":"scip_signature","symbol":"Dog#sound().","language":"typescript","text":"sound(): string"}"#,
        r#"{"record":"scip_signature_occurrence","symbol":"Dog#sound().","ref_symbol":"string#","start":9,"end":15,"roles":0}"#,
        r#""enclosing_symbol":"Dog#""#,
        r#""syntax_kind":7"#,
    ] {
        assert!(stream.contains(expected), "missing {expected} in {stream}");
    }
}

/// `--scip-facts` over the relationship fixture, with extra flags appended.
fn scip_rel_facts(extra: &[&str]) -> String {
    let mut args = vec!["--scip-facts"];
    args.extend_from_slice(extra);
    args.extend_from_slice(&[
        "--project-root",
        SCIP_REL_ROOT,
        "--scip-build",
        SCIP_REL_SOURCE,
    ]);
    run(&args)
}

/// The stream minus the metadata row, whose `project_root` is absolute.
fn without_metadata(facts: &str) -> String {
    facts
        .lines()
        .filter(|line| !line.contains("\"record\":\"scip_metadata\""))
        .map(|line| format!("{line}\n"))
        .collect()
}

/// `--scip-facts` without an index is a named error, never an empty success.
/// An empty stream would read as "this project has no symbols".
#[test]
fn scip_facts_without_an_index_is_a_named_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args([
            "--scip-facts",
            "--project-root",
            SCIP_REL_ROOT,
            SCIP_REL_SOURCE,
        ])
        .output()
        .expect("extract binary runs");
    assert!(!output.status.success());
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        message.contains("--scip-index") && message.contains("--scip-build"),
        "the error must name what is missing, got: {message}"
    );
}

/// `--file-fact` prepends one `file` row and leaves the rest of the stream
/// alone. The line count is v5's `file_lines`; the digest is the same content
/// key every resolved edge is already keyed on.
#[test]
fn file_fact_prepends_one_row_without_disturbing_the_stream() {
    let plain = run(&[SCIP_REL_SOURCE]);
    let with_row = run(&["--file-fact", SCIP_REL_SOURCE]);

    let (first, rest) = with_row.split_once('\n').expect("at least the file row");
    assert_eq!(
        first,
        r#"{"record":"file","path":"tests/fixtures/scip_rel/animal.ts","digest":"blake3:baafb5a5209830bda1b0dd4a9265e014b895256daee5e9252e3cb71bff3e3968","bytes":137,"lines":9}"#
    );
    assert_eq!(rest, plain, "the file row must not perturb the fact stream");
}

/// The line-count convention, stated as a test because "how many lines" has
/// three defensible answers and callers need to know which one this is: the
/// count a text editor shows. A file with no trailing newline still counts its
/// last partial line; an empty file has zero lines.
#[test]
fn file_fact_counts_lines_the_way_an_editor_does() {
    let dir = std::env::temp_dir().join(format!("sprefa-file-fact-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases: &[(&str, &str, u32, u32)] = &[
        ("empty.ts", "", 0, 0),
        ("one_terminated.ts", "let a = 1;\n", 11, 1),
        ("one_unterminated.ts", "let a = 1;", 10, 1),
        ("three.ts", "let a = 1;\nlet b = 2;\nlet c = 3;\n", 33, 3),
        ("blank_last.ts", "let a = 1;\n\n", 12, 2),
    ];
    for (name, content, bytes, lines) in cases {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        let row = run(&["--file-fact", "--family", "cst", path.to_str().unwrap()]);
        let row = row.lines().next().expect("the file row");
        assert!(
            row.contains(&format!("\"bytes\":{bytes},\"lines\":{lines}")),
            "{name}: expected bytes {bytes} lines {lines}, got {row}"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// `--scip-deps` folds the index into file-to-file edges. Two crossings in the
/// fixture set: gamma imports from alpha (beta exports a same-named symbol
/// nobody imports, so a name match would be ambiguous), and delta imports the
/// class whose method only a typed receiver reaches.
///
/// v6 has NO TypeScript module resolver. The spelunk named that as the blocker
/// for a native dep(src,dst); this is the bypass, and the edge below is produced
/// with no resolver anywhere in the crate.
///
/// `kind` IS `unknown` HERE AND THAT IS THE INDEX'S PROPERTY, not a gap: the row
/// is a symbol crossing, and an occurrence says nothing about the import form
/// that bound the name. `--deps` reads the specifier and fills it.
// @comment-ok: the unknown column is a contract claim the assertion alone cannot show
#[test]
fn scip_deps_folds_the_index_into_file_edges() {
    let edges = run(&[
        "--scip-deps",
        "--project-root",
        "tests/fixtures/ts",
        "--scip-build",
        "tests/fixtures/ts/scip/alpha.ts",
    ]);
    assert_eq!(
        edges,
        "{\"record\":\"file_edge\",\"src_path\":\"scip/delta.ts\",\
         \"dst_path\":\"scip/epsilon.ts\",\"kind\":\"unknown\",\"symbols\":3}\n\
         {\"record\":\"file_edge\",\"src_path\":\"scip/gamma.ts\",\
         \"dst_path\":\"scip/alpha.ts\",\"kind\":\"unknown\",\"symbols\":2}\n"
    );
    // beta.ts exports a symbol with the same name as alpha's and nothing imports
    // it. An edge into beta would mean the fold joined on names rather than on
    // the indexer's resolution.
    assert!(!edges.contains("beta.ts"));
}

/// Local symbols are document-scoped: scip reuses `local 0` in every file, so
/// joining on them would mint an edge between every pair of files that happens
/// to have a local. The ts fixture root has 50+ local occurrences and the fold
/// produces exactly the two real import crossings, which is the assertion.
#[test]
fn scip_deps_never_joins_on_document_scoped_local_symbols() {
    let facts = run(&[
        "--scip-facts",
        "--project-root",
        "tests/fixtures/ts",
        "--scip-build",
        "tests/fixtures/ts/scip/alpha.ts",
    ]);
    let locals = facts
        .lines()
        .filter(|line| line.contains("\"symbol\":\"local "))
        .count();
    assert!(
        locals > 50,
        "the fixture root should be dense with local symbols, found {locals}"
    );

    let edges = run(&[
        "--scip-deps",
        "--project-root",
        "tests/fixtures/ts",
        "--scip-build",
        "tests/fixtures/ts/scip/alpha.ts",
    ]);
    assert_eq!(
        edges.lines().count(),
        2,
        "{locals} local occurrences must contribute no edges, got: {edges}"
    );
}
