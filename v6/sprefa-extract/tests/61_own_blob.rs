//! `own_blob` identity. The seam answer (`cx.own`) wins outright; the hand-built
//! fallback is deterministic: highest span-match count wins, an exact tie is
//! None. Two fixture files whose only named def sits at the SAME byte span are
//! the case the old first-match hash-order scan answered by coin flip.

use std::sync::Arc;

use sprefa_extract::{
    build_def_index, content_id_of, dispatch, own_blob, ContentId, ExtractOutput, FamilyMask,
    FileSet, IndexBag, ManifestMap, ProjectCx, ProjectDigest,
};

const A_PATH: &str = "tests/fixtures/own_blob/own_blob_a.rs";
const B_PATH: &str = "tests/fixtures/own_blob/own_blob_b.rs";
const SRC_A: &[u8] = include_bytes!("fixtures/own_blob/own_blob_a.rs");
const SRC_B: &[u8] = include_bytes!("fixtures/own_blob/own_blob_b.rs");

type Corpus = Vec<(ContentId, Arc<ExtractOutput>)>;

/// Two rust outputs, each with its own blob key. `SRC_A` declares two fns,
/// `SRC_B` one, and the first fn of each sits at the same byte span.
fn corpus() -> Corpus {
    vec![
        (
            content_id_of(SRC_A),
            dispatch(A_PATH, SRC_A, FamilyMask::ALL).expect("a extracts"),
        ),
        (
            content_id_of(SRC_B),
            dispatch(B_PATH, SRC_B, FamilyMask::ALL).expect("b extracts"),
        ),
    ]
}

fn with_fallback_cx<R>(
    pairs: &[(ContentId, &ExtractOutput)],
    f: impl FnOnce(&ProjectCx<'_>) -> R,
) -> R {
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
        .set(build_def_index(pairs))
        .expect("fresh def index");
    f(&cx)
}

/// Named CallF def spans of one output, sorted and deduped.
fn named_spans(output: &ExtractOutput) -> Vec<sprefa_extract::Span> {
    let mut spans: Vec<sprefa_extract::Span> = output
        .call
        .as_ref()
        .map(|call| {
            call.nodes
                .iter()
                .filter(|n| n.name.is_some())
                .map(|n| n.span)
                .collect()
        })
        .unwrap_or_default();
    spans.sort();
    spans.dedup();
    spans
}

/// The fixture premise the tests below stand on: the files share one named span.
#[test]
fn fixtures_share_one_named_span() {
    let corpus = corpus();
    let a = named_spans(&corpus[0].1);
    let b = named_spans(&corpus[1].1);
    assert!(!a.is_empty() && !b.is_empty(), "fixtures must name defs");
    let shared: Vec<_> = a.iter().filter(|s| b.contains(s)).collect();
    assert_eq!(shared.len(), 1, "exactly one shared named span expected");
}

/// With `own` set, the seam answer IS the blob: no span search runs, so even
/// the file whose span set matches the OTHER blob's sites resolves to itself.
#[test]
fn own_set_wins_over_span_search() {
    let corpus = corpus();
    let pairs: Vec<(ContentId, &ExtractOutput)> = corpus
        .iter()
        .map(|(blob, out)| (blob.clone(), out.as_ref()))
        .collect();
    with_fallback_cx(&pairs, |cx| {
        sprefa_extract::types::set_own(Some(corpus[1].0.clone()));
        assert_eq!(
            own_blob(cx, &corpus[0].1),
            Some(corpus[1].0.clone()),
            "the seam answer must win even against the span evidence"
        );
    });
}

/// Fallback with the shared span plus a second named span: two matches for A,
/// one for B, so the max-count rule picks A deterministically.
#[test]
fn fallback_max_count_breaks_the_shared_span_tie() {
    let corpus = corpus();
    let pairs: Vec<(ContentId, &ExtractOutput)> = corpus
        .iter()
        .map(|(blob, out)| (blob.clone(), out.as_ref()))
        .collect();
    with_fallback_cx(&pairs, |cx| {
        assert_eq!(
            own_blob(cx, &corpus[0].1),
            Some(corpus[0].0.clone()),
            "the second named span must break the tie toward the richer file"
        );
    });
}

/// Fallback with ONLY the shared span: both blobs match once, an exact tie,
/// so the answer is None (the index cannot tell the files apart).
#[test]
fn fallback_exact_tie_is_none() {
    let corpus = corpus();
    let pairs: Vec<(ContentId, &ExtractOutput)> = corpus
        .iter()
        .map(|(blob, out)| (blob.clone(), out.as_ref()))
        .collect();
    with_fallback_cx(&pairs, |cx| {
        assert_eq!(
            own_blob(cx, &corpus[1].1),
            None,
            "an exact span-match tie must resolve to None, never to a coin flip"
        );
    });
}
