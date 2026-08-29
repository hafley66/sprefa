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
