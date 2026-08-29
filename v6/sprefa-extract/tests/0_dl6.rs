// Fail-first paste:
// running 1 test
// test dl6_extraction_exact_nodes_edges_and_spans ... FAILED
// failures:
//     dl6_extraction_exact_nodes_edges_and_spans
// test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

use sprefa_extract::{
    build_def_index, content_id_of, flatten, CallEdgeKind, CallF, DlSource, FamilyMask, FamilyTag,
    FileSet, IndexBag, ManifestMap, ProjectCx, ProjectDigest, Resolve, Source, Span,
};

const SAMPLE_PATH: &str = "tests/fixtures/dl6/0_sample.dl6";
const SAMPLE_SRC: &[u8] = include_bytes!("fixtures/dl6/0_sample.dl6");

const CALLER_PATH: &str = "tests/fixtures/dl6/1_caller.dl6";
const CALLER_SRC: &[u8] = include_bytes!("fixtures/dl6/1_caller.dl6");

const CALLEE_PATH: &str = "tests/fixtures/dl6/2_callee.dl6";
const CALLEE_SRC: &[u8] = include_bytes!("fixtures/dl6/2_callee.dl6");

#[test]
fn dl6_extraction_exact_nodes_edges_and_spans() {
    let output = DlSource.extract(SAMPLE_PATH, SAMPLE_SRC, FamilyMask::ALL);
    let facts = flatten(&output);

    let definitions: Vec<_> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Node {
                family: FamilyTag::Call,
                name: Some(name),
                span,
                ..
            } => Some((name.as_str(), span.start, span.end)),
            _ => None,
        })
        .collect();

    let sites: Vec<_> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Site {
                callee,
                callee_path,
                span,
                ..
            } => Some((
                callee.as_str(),
                callee_path.as_deref(),
                span.start,
                span.end,
            )),
            _ => None,
        })
        .collect();

    let specifiers: Vec<_> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Specifier {
                kind,
                module,
                name,
                span,
                ..
            } => Some((
                kind.as_str(),
                module.as_deref(),
                name.as_str(),
                span.start,
                span.end,
            )),
            _ => None,
        })
        .collect();

    let type_entities: Vec<_> = facts
        .iter()
        .filter_map(|fact| match fact {
            sprefa_extract::FlatFact::Node {
                family: FamilyTag::Type,
                name: Some(name),
                span,
                ..
            } => Some((name.as_str(), span.start, span.end)),
            _ => None,
        })
        .collect();

    assert_eq!(
        specifiers,
        [
            ("side_effect", None, "catalog", 0, 14),
            ("named", Some("graph"), "g", 15, 32),
        ]
    );

    assert_eq!(type_entities, [("edge", 34, 65), ("path", 66, 97),]);

    assert_eq!(
        definitions,
        [
            ("edge", 99, 114),
            ("edge", 115, 130),
            ("path", 132, 157),
            ("path", 158, 195),
        ]
    );

    assert_eq!(
        sites,
        [
            ("edge", None, 146, 156),
            ("edge", None, 172, 182),
            ("path", None, 184, 194),
            ("path", None, 199, 209),
        ]
    );

    assert!(output.cst.as_ref().is_some_and(|b| !b.nodes.is_empty()));
}

#[test]
fn dl6_call_and_type_resolution() {
    let caller_out = DlSource.extract(CALLER_PATH, CALLER_SRC, FamilyMask::ALL);
    let callee_out = DlSource.extract(CALLEE_PATH, CALLEE_SRC, FamilyMask::ALL);

    let caller_blob = content_id_of(CALLER_SRC);
    let callee_blob = content_id_of(CALLEE_SRC);

    let index = build_def_index(&[
        (caller_blob, &caller_out),
        (callee_blob.clone(), &callee_out),
    ]);
    let indexes = IndexBag::default();
    indexes.def_index.set(index).unwrap();
    let files = FileSet;
    let manifests = ManifestMap;
    let cx = ProjectCx {
        files: &files,
        manifests: &manifests,
        reader: None,
        digest: ProjectDigest::default(),
        indexes,
        own: std::cell::RefCell::new(None),
    };

    let edges = Resolve::<CallF>::resolve(&DlSource, &caller_out, &cx);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, CallEdgeKind::NameResolve);
    assert_eq!(edges[0].dst_blob, callee_blob);
    assert_eq!(edges[0].dst_span, Span { start: 31, len: 17 });
    assert_eq!(edges[0].call_site, Some(Span { start: 89, len: 16 }));
}
