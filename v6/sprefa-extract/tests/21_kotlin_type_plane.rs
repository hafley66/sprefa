//! Kotlin type_edge parity: `Resolve<TypeF>` over the kotlin
//! fixture corpus, twin-normalized to the oracle's
//! `type_edge\t{owner}\t{to}\t{kind}` text shape via
//! `KotlinSource::type_edge_candidates` (the same zip discipline as
//! golden_parity's ts/go/rust legs). The candidate row IS the parity target;
//! the resolve output pairs 1:1, in order, with the candidates. The kotlin leg
//! lives here rather than in golden_parity.rs, which this card does not own;
//! folding it into the shared parity test is a named follow-up.

use std::collections::BTreeSet;
use std::sync::Arc;

use sprefa_extract::{
    build_def_index, content_id_of, dispatch, ContentId, ExtractOutput, FamilyMask, FileSet,
    IndexBag, KotlinSource, ManifestMap, ProjectCx, ProjectDigest, Resolve, Span, TypeF,
};

struct Case {
    name: &'static str,
    path: &'static str,
    fixture: &'static [u8],
    baseline: &'static str,
    fixture_dir: &'static str,
}

const CASES: &[Case] = &[Case {
    name: "kotlin_sample",
    path: "v6/sprefa-extract/tests/fixtures/kotlin/sample.kt",
    fixture: include_bytes!("fixtures/kotlin/sample.kt"),
    baseline: include_str!("fixtures/kotlin/sample.v5.jsonl"),
    fixture_dir: "kotlin",
}];

fn facet_of(line: &str) -> &str {
    line.split('\t').next().unwrap_or("")
}

/// The phase-2 corpus: the case's ExtractOutput + its real blake3 blob hash,
/// the DefIndex folded over all of them (the resolution universe), and a
/// borrowed ProjectCx. Mirror of golden_parity's shared helper.
fn with_resolve_cx<R>(
    f: impl FnOnce(&ProjectCx, &[(ContentId, Arc<ExtractOutput>, &'static Case)]) -> R,
) -> R {
    let corpus: Vec<(ContentId, Arc<ExtractOutput>, &'static Case)> = CASES
        .iter()
        .map(|case| {
            let out = dispatch(case.path, case.fixture, FamilyMask::ALL).expect("source");
            (content_id_of(case.fixture), out, case)
        })
        .collect();
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

/// The entity name at a candidate's owner span (the from-leg of the oracle's
/// text shape). A miss is a collection bug, rendered loud, not skipped.
fn owner_name(out: &ExtractOutput, span: Span) -> String {
    out.types
        .as_ref()
        .and_then(|types| types.nodes.iter().find(|node| node.span == span))
        .and_then(|node| node.name)
        .map(|id| out.strings.lookup(id).to_string())
        .unwrap_or_else(|| format!("<no entity at {}..{}>", span.start, span.end()))
}

/// Kotlin type_edge PARITY: `Resolve<TypeF>` over the kotlin fixture corpus,
/// twin-normalized to the oracle's text shape via
/// `KotlinSource::type_edge_candidates` (the zip discipline: edge i resolves
/// candidate i). v5 kotlin emits field/impl/generic/variant only (the sample
/// carries all four, 10 rows).
#[test]
fn type_edge_resolve_parity_kotlin() {
    with_resolve_cx(|cx, corpus| {
        for (_blob, out, case) in corpus {
            if case.fixture_dir != "kotlin" {
                continue;
            }
            let edges = Resolve::<TypeF>::resolve(&KotlinSource, out, cx);
            let candidates = KotlinSource::type_edge_candidates(out);
            let mut v6: BTreeSet<String> = edges
                .iter()
                .zip(candidates.iter())
                .map(|(_edge, cand)| {
                    format!(
                        "type_edge\t{}\t{}\t{}",
                        owner_name(out, cand.owner),
                        out.strings.lookup(cand.to),
                        cand.kind.as_str()
                    )
                })
                .collect();
            if edges.len() != candidates.len() {
                v6.insert(format!(
                    "ZIP_MISMATCH edges={} candidates={}",
                    edges.len(),
                    candidates.len()
                ));
            }
            let v5: BTreeSet<String> = case
                .baseline
                .lines()
                .filter(|line| facet_of(line) == "type_edge")
                .map(str::to_owned)
                .collect();
            let only_v5: Vec<&String> = v5.difference(&v6).collect();
            let only_v6: Vec<&String> = v6.difference(&v5).collect();
            assert!(
                only_v5.is_empty() && only_v6.is_empty(),
                "[{}] type_edge parity diff vs v5 oracle:\n  missing from v6 ({}):\n{}\n  only in v6 ({}):\n{}",
                case.name,
                only_v5.len(),
                only_v5.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
                only_v6.len(),
                only_v6.iter().map(|s| format!("    {s}")).collect::<Vec<_>>().join("\n"),
            );
            eprintln!(
                "[{}] type_edge parity: {} rows compared, 0 divergence",
                case.name,
                v5.len()
            );
        }
    });
}
