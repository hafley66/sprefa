//! Markdown `doc_node` extraction + the `doc_ref` bridge. The fixture names two
//! corpus entities: `Engine` (unique) and `Point` (ambiguous, so no edge).

use std::collections::BTreeSet;
use std::sync::Arc;

use sprefa_extract::{
    build_def_index, content_id_of, dispatch, ContentId, ExtractOutput, FamilyMask, FileSet,
    IndexBag, ManifestMap, MarkdownSource, NodeRef, ProjectCx, ProjectDigest, Resolve, Span,
    TypeEdgeKind, TypeF,
};

const MD_PATH: &str = "v6/sprefa-extract/tests/fixtures/markdown/doc_node.md";
const MD: &[u8] = include_bytes!("fixtures/markdown/doc_node.md");
const RUST_SAMPLE: &[u8] = include_bytes!("fixtures/rust/sample.rs");
const TS_SAMPLE: &[u8] = include_bytes!("fixtures/ts/sample.ts");
const TS_DOCS: &[u8] = include_bytes!("fixtures/ts/docs.ts");

/// A types-only mask: markdown projects `doc_nodes` onto the types plane only
/// when the raw cst plane is not requested.
const TYPES_ONLY: FamilyMask = FamilyMask {
    cst: false,
    types: true,
    call: false,
    df: false,
    data: false,
};
const CST_ONLY: FamilyMask = FamilyMask {
    cst: true,
    types: false,
    call: false,
    df: false,
    data: false,
};

/// The exact `doc_node` row set the fixture projects, hand-derived byte spans.
/// The fence without a language carries an empty name.
const EXPECTED_DOC_NODES: &[&str] = &[
    r#"{"record":"doc_node","family":"type","span":{"start":0,"end":9},"kind":"heading","name":"Engine","parent":null}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":10,"end":19},"kind":"heading","name":"Start","parent":"Engine"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":20,"end":32},"kind":"heading","name":"Details","parent":"Start"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":33,"end":50},"kind":"heading","name":"Configuration","parent":"Engine"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":51,"end":60},"kind":"heading","name":"Point","parent":"Engine"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":61,"end":88},"kind":"code_block","name":"rust","parent":"Point"}"#,
    r#"{"record":"doc_node","family":"type","span":{"start":89,"end":109},"kind":"code_block","name":"","parent":"Point"}"#,
];

/// The corpus: the markdown fixture plus the rust/ts fixtures that declare the
/// names its headings name. `Point` is declared twice, `Engine` once.
fn with_resolve_cx<R>(
    f: impl FnOnce(&ProjectCx, &[(ContentId, Arc<ExtractOutput>, &'static str)]) -> R,
) -> R {
    let corpus: Vec<(ContentId, Arc<ExtractOutput>, &'static str)> = vec![
        (
            content_id_of(MD),
            dispatch(MD_PATH, MD, TYPES_ONLY).expect("md"),
            "md",
        ),
        (
            content_id_of(RUST_SAMPLE),
            dispatch(
                "v6/sprefa-extract/tests/fixtures/rust/sample.rs",
                RUST_SAMPLE,
                FamilyMask::ALL,
            )
            .expect("rust"),
            "rust",
        ),
        (
            content_id_of(TS_SAMPLE),
            dispatch(
                "v6/sprefa-extract/tests/fixtures/ts/sample.ts",
                TS_SAMPLE,
                FamilyMask::ALL,
            )
            .expect("ts"),
            "ts",
        ),
        (
            content_id_of(TS_DOCS),
            dispatch(
                "v6/sprefa-extract/tests/fixtures/ts/docs.ts",
                TS_DOCS,
                FamilyMask::ALL,
            )
            .expect("ts"),
            "ts",
        ),
    ];
    let pairs: Vec<(ContentId, &ExtractOutput)> = corpus
        .iter()
        .map(|(hash, out, _)| (hash.clone(), out.as_ref()))
        .collect();
    let file_set = FileSet;
    let manifest_map = ManifestMap;
    let cx = ProjectCx {
        files: &file_set,
        manifests: &manifest_map,
        reader: None,
        digest: ProjectDigest::default(),
        indexes: IndexBag::default(),
        witness: false,
    };
    cx.indexes
        .def_index
        .set(build_def_index(&pairs))
        .expect("fresh OnceLock");
    f(&cx, &corpus)
}

/// The exact `doc_node` row set, every span a hand-derived literal. The level-2
/// siblings parent to Engine, never to each other; fences parent to Point.
#[test]
fn doc_node_rows_match_heading_stack() {
    let out = dispatch(MD_PATH, MD, TYPES_ONLY).expect("markdown source");
    let rows: BTreeSet<String> = sprefa_extract::flatten_jsonl(&out).into_iter().collect();
    let expected: BTreeSet<String> = EXPECTED_DOC_NODES.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        rows, expected,
        "doc_node row set diverges from the hand-derived expectation"
    );
}

/// The cst plane for the same fixture is unchanged: 35 node rows, the count
/// measured at the base sha. Pins that the doc work added no cst rows.
#[test]
fn cst_plane_unchanged_from_base_sha() {
    let out = dispatch(MD_PATH, MD, CST_ONLY).expect("markdown source");
    let node_rows = sprefa_extract::flatten_jsonl(&out)
        .into_iter()
        .filter(|line| line.contains("\"record\":\"node\""))
        .count();
    assert_eq!(node_rows, 35, "cst node count drifted from the base sha");
}

/// The heading `Engine` names one corpus entity, so it bridges to exactly one
/// `doc_ref` edge; the ambiguous `Point` heading bridges to none.
#[test]
fn doc_ref_bridges_unique_heading_only() {
    with_resolve_cx(|cx, corpus| {
        let (md_blob, md_out, _) = corpus.iter().find(|(_, _, tag)| *tag == "md").unwrap();
        let (rust_blob, _, _) = corpus.iter().find(|(_, _, tag)| *tag == "rust").unwrap();
        let edges = Resolve::<TypeF>::resolve(&MarkdownSource, md_out, cx);

        // Engine is the first heading in the fixture, so src is doc_nodes[0].
        assert_eq!(
            edges.len(),
            1,
            "expected exactly one doc_ref edge, got {}",
            edges.len()
        );
        let edge = &edges[0];
        assert_eq!(edge.src, NodeRef(0), "src indexes the doc_nodes row");
        assert_eq!(edge.kind, TypeEdgeKind::DocRef);
        assert_eq!(
            &edge.dst_blob, rust_blob,
            "Engine bridges to rust/sample.rs"
        );
        // rust/sample.rs `pub struct Engine` spans bytes 169..175.
        assert_eq!(edge.dst_span, Span { start: 169, len: 6 });
        let _ = md_blob;
    });
}
