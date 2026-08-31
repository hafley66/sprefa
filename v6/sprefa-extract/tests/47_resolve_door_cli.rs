//! The `--resolve` DOOR: what the binary reaches, and what it says when it
//! cannot. Every assertion here is a CLI-level contract the library already
//! satisfied, so each one pins a place the binary fell behind `sprefa_extract`.

use std::process::{Command, Output};
use std::sync::Arc;

use sprefa_extract::{
    build_def_index, content_id_of, dispatch, ContentId, ExtractOutput, FamilyMask, FileSet,
    IndexBag, ManifestMap, MarkdownSource, ProjectCx, ProjectDigest, Resolve, TypeF,
};

const MD: &str = "tests/fixtures/markdown/doc_node.md";
const RUST_SAMPLE: &str = "tests/fixtures/rust/sample.rs";
const TS_SAMPLE: &str = "tests/fixtures/ts/sample.ts";
const TS_DOCS: &str = "tests/fixtures/ts/docs.ts";
const TS_UNRESOLVED: &str = "tests/fixtures/ts_unresolved/unresolved.ts";
const PY_CLASS: &str = "tests/fixtures/python/corpus_8.py";
const PY_CALLER: &str = "tests/fixtures/python/corpus_9.py";

fn extract(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs")
}

fn stdout_of(args: &[&str]) -> String {
    let output = extract(args);
    assert!(
        output.status.success(),
        "{args:?} exited {:?} stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// The `doc_ref` edges the library answers for the same four-file corpus,
/// through `Resolve<TypeF>` on `MarkdownSource` directly. The oracle the CLI
/// run below is graded against.
fn library_doc_ref_count() -> usize {
    let read = |path: &str| std::fs::read(path).expect("fixture reads");
    let corpus: Vec<(String, Vec<u8>, FamilyMask)> = vec![
        (MD.to_string(), read(MD), FamilyMask::ALL),
        (RUST_SAMPLE.to_string(), read(RUST_SAMPLE), FamilyMask::ALL),
        (TS_SAMPLE.to_string(), read(TS_SAMPLE), FamilyMask::ALL),
        (TS_DOCS.to_string(), read(TS_DOCS), FamilyMask::ALL),
    ];
    // The markdown doc plane exists only with cst off, so the md entry is
    // dispatched under the types-only mask the library's own test uses.
    let types_only = FamilyMask {
        cst: false,
        types: true,
        call: false,
        df: false,
        data: false,
    };
    let outputs: Vec<(ContentId, Arc<ExtractOutput>)> = corpus
        .iter()
        .map(|(path, bytes, mask)| {
            let mask = if path == MD { types_only } else { *mask };
            (
                content_id_of(bytes),
                dispatch(path, bytes, mask).expect("a Source matches the fixture"),
            )
        })
        .collect();
    let pairs: Vec<(ContentId, &ExtractOutput)> = outputs
        .iter()
        .map(|(blob, out)| (blob.clone(), out.as_ref()))
        .collect();
    let files = FileSet;
    let manifests = ManifestMap;
    let cx = ProjectCx {
        files: &files,
        manifests: &manifests,
        reader: None,
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
    };
    cx.indexes
        .def_index
        .set(build_def_index(&pairs))
        .expect("fresh OnceLock");
    Resolve::<TypeF>::resolve(&MarkdownSource, &outputs[0].1, &cx).len()
}

/// FAIL-FIRST RECEIPT: 0 doc_ref rows from the binary against 1 from the
/// library. `read_inputs` dispatched every path under `FamilyMask::ALL`, and
/// markdown projects its doc plane only when cst is OFF
/// (`src/lang/markdown/_0_source.rs:142`), so the CLI handed the `doc_ref` arm
/// an empty types bundle and it resolved nothing.
#[test]
fn resolve_cli_reaches_the_markdown_doc_ref_arm() {
    let expected = library_doc_ref_count();
    assert!(expected > 0, "the library oracle must have edges to match");
    let rows = stdout_of(&[
        "--resolve",
        "--family",
        "type",
        MD,
        RUST_SAMPLE,
        TS_SAMPLE,
        TS_DOCS,
    ]);
    let doc_refs: Vec<&str> = rows
        .lines()
        .filter(|line| line.contains(r#""kind":"doc_ref""#))
        .collect();
    assert_eq!(
        doc_refs.len(),
        expected,
        "the CLI must emit the library's doc_ref edges:\n{rows}"
    );
    assert!(
        doc_refs[0].contains(r#""owner_name":"Engine""#),
        "the doc_ref owner is the heading that named the entity: {}",
        doc_refs[0]
    );
    assert!(
        doc_refs[0].contains(&format!(r#""target_path":"{RUST_SAMPLE}""#)),
        "Engine bridges to the rust declaration: {}",
        doc_refs[0]
    );
}

/// FAIL-FIRST RECEIPT (2026-08-31): `"callee_name":null` on the `Widget()` row
/// was the original defect, pinned dead by the no-null assertion below. The
/// callee ruling flipped in the same test: a constructor call resolves to the
/// class's `__init__` (PyCG oracle semantics), not the class TypeF def — the
/// class-name row was 37 of 136 bench rows of false precision.
#[test]
fn resolve_cli_names_a_class_constructor_callee() {
    let rows = stdout_of(&["--resolve", PY_CLASS, PY_CALLER]);
    assert!(
        !rows.contains(r#""callee_name":null"#),
        "every resolved edge must name its callee:\n{rows}"
    );
    assert!(
        rows.contains(r#""callee_name":"__init__""#),
        "the constructor edge must name __init__:\n{rows}"
    );
    assert!(
        !rows.contains(r#""callee_name":"Widget""#),
        "a constructor edge never names the class def itself:\n{rows}"
    );
}

/// FAIL-FIRST RECEIPT: `Error: Read("tests/fixtures/ts", Custom { kind: Other,
/// error: "read /abs/path" })`, a Debug dump of the error type with no reading
/// for a human.
#[test]
fn resolve_cli_names_a_directory_plainly() {
    let output = extract(&["--resolve", "tests/fixtures/ts"]);
    assert_eq!(output.status.code(), Some(2), "an argument error exits 2");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("tests/fixtures/ts") && stderr.contains("is a directory"),
        "the message must name the path and the cause: {stderr}"
    );
    assert!(
        !stderr.contains("Custom {"),
        "no Debug dump of the error type: {stderr}"
    );
    assert!(
        stderr.contains("--resolve takes files"),
        "the message must say what to pass instead: {stderr}"
    );
}

/// One path is a legitimate resolve universe: same-file edges resolve inside
/// it, which `tests/1_resolve_cli.rs:52` pins for kotlin. The help text said
/// "Needs two or more paths", so the docs and the binary disagreed.
#[test]
fn resolve_cli_accepts_one_path_and_says_so() {
    let rows = stdout_of(&["--resolve", TS_SAMPLE]);
    assert!(
        rows.contains(r#""record":"resolved_edge""#),
        "one path resolves its own same-file edges:\n{rows}"
    );
    let help = stdout_of(&["--help"]);
    assert!(
        !help.contains("Needs two or more paths"),
        "the help must not promise a minimum the binary does not enforce"
    );
    assert!(
        help.contains("One path is a legal universe"),
        "the help must state what one path means:\n{help}"
    );
}

/// The PHASE-1 `unresolved` rows (dynamic import, computed member, spread
/// args) have no path field (`src/wire.rs:290`), so they cannot name their
/// file in a multi-file phase-2 stream: `--resolve` never carries them. The
/// phase-2 drops channel DOES emit `unresolved` rows (path included) when a
/// resolve arm declines a site it traced, same discipline as the go arm.
#[test]
fn resolve_cli_documents_the_phase_one_records_it_drops() {
    let rows = stdout_of(&["--resolve", TS_UNRESOLVED, TS_SAMPLE]);
    assert!(
        !rows.contains(r#""reason":"dynamic-import""#),
        "phase-1 rows never ride --resolve:\n{rows}"
    );
    let per_file = stdout_of(&[TS_UNRESOLVED]);
    assert!(
        per_file.contains(r#""reason":"dynamic-import""#),
        "the per-file door is where the phase-1 record lives:\n{per_file}"
    );
    let help = stdout_of(&["--help"]);
    assert!(
        help.contains("never the per-file phase-1 records"),
        "the help must say --resolve drops them:\n{help}"
    );
    assert!(
        help.contains("unresolved"),
        "the help must name the record by name:\n{help}"
    );
}
