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

/// The whole `--scip-facts` stream over the scip.proto worked example. The
/// golden carries all three records, so it pins the field names of each.
///
/// The two scip_relationship rows are the point. The diet DROPPED relationships
/// before this lane, which is why v5's `scip_impl` and the `scip_edge` family
/// had no v6 input at all. `Dog#` implements `Animal#`, and `Dog#sound()` both
/// references and implements `Animal#sound()`; that is the exact pair the spec
/// documents.
#[test]
fn scip_facts_stream_occurrences_symbols_and_relationships() {
    let facts = run(&[
        "--scip-facts",
        "--project-root",
        SCIP_REL_ROOT,
        "--scip-build",
        SCIP_REL_SOURCE,
    ]);
    assert_eq!(facts, SCIP_REL_GOLDEN);

    let relationships = facts
        .lines()
        .filter(|line| line.contains("\"record\":\"scip_relationship\""))
        .count();
    assert_eq!(
        relationships, 2,
        "the implements pair from scip.proto's own worked example must survive \
         the diet; dropping relationships is what made scip_impl inexpressible"
    );
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
        r#"{"record":"file","path":"tests/fixtures/scip_rel/animal.ts","digest":"baafb5a5209830bda1b0dd4a9265e014","bytes":137,"lines":9}"#
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

/// `--scip-deps` folds the index into file-to-file edges. The fixture trio is the
/// minimal case that matters: gamma imports from alpha, and beta exports a
/// same-named symbol nobody imports, so a name-match would be ambiguous and bind
/// nothing. The indexer resolves it, which is the whole reason this path exists.
///
/// v6 has NO TypeScript module resolver. The spelunk named that as the blocker
/// for a native dep(src,dst); this is the bypass, and the edge below is produced
/// with no resolver anywhere in the crate.
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
        "{\"record\":\"file_edge\",\"src_path\":\"scip/gamma.ts\",\
         \"dst_path\":\"scip/alpha.ts\",\"symbols\":2}\n"
    );
    // beta.ts exports a symbol with the same name as alpha's and nothing imports
    // it. An edge into beta would mean the fold joined on names rather than on
    // the indexer's resolution.
    assert!(!edges.contains("beta.ts"));
}

/// Local symbols are document-scoped: scip reuses `local 0` in every file, so
/// joining on them would mint an edge between every pair of files that happens
/// to have a local. The ts fixture root has 100+ local occurrences and the fold
/// produces exactly one edge, which is the assertion.
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
        1,
        "{locals} local occurrences must contribute no edges, got: {edges}"
    );
}
