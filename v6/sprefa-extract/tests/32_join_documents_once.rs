//! COUNT test for the per-project document join. FAIL-PRE-FIX: with
//! `join_documents` inside each resolve arm, 3 files over 5 documents read 15
//! times; measured 15 before the OnceLock and 5 after.

use std::sync::atomic::{AtomicUsize, Ordering};

use sprefa_extract::{
    build_def_index, content_id_of, CallF, ExtractOutput, FamilyMask, FileSet, IndexBag,
    ManifestMap, ProjectCx, ProjectDigest, Resolve, RustSource, ScipDocument, ScipIndex, Source,
};

const FILES: [(&str, &str); 3] = [
    ("a.rs", "fn one() { two(); }\nfn two() {}\n"),
    ("b.rs", "fn three() { one(); }\n"),
    ("c.rs", "fn four() { three(); }\n"),
];

const DOCUMENTS: [&str; 5] = ["a.rs", "b.rs", "c.rs", "vendor/d.rs", "vendor/e.rs"];

fn index_over(paths: &[&str]) -> ScipIndex {
    ScipIndex {
        documents: paths
            .iter()
            .map(|path| ScipDocument {
                relative_path: (*path).to_string(),
                ..ScipDocument::default()
            })
            .collect(),
        ..ScipIndex::default()
    }
}

#[test]
fn the_document_join_reads_each_document_once_per_project() {
    let extracted: Vec<(&str, ExtractOutput)> = FILES
        .iter()
        .map(|(path, text)| {
            (
                *path,
                RustSource.extract(path, text.as_bytes(), FamilyMask::ALL),
            )
        })
        .collect();
    let pairs: Vec<_> = extracted
        .iter()
        .zip(FILES.iter())
        .map(|((_, output), (_, text))| (content_id_of(text.as_bytes()), output))
        .collect();

    let indexes = IndexBag::default();
    indexes.def_index.set(build_def_index(&pairs)).ok();
    indexes.scip_index.set(index_over(&DOCUMENTS)).ok();

    let reads = AtomicUsize::new(0);
    let counting_reader = |relative: &str| -> Option<Vec<u8>> {
        reads.fetch_add(1, Ordering::SeqCst);
        FILES
            .iter()
            .find(|(path, _)| *path == relative)
            .map(|(_, text)| text.as_bytes().to_vec())
    };
    let reader: &(dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync) = &counting_reader;
    let files = FileSet;
    let manifests = ManifestMap;
    let cx = ProjectCx {
        files: &files,
        manifests: &manifests,
        reader: Some(reader),
        digest: ProjectDigest::default(),
        indexes,
    };

    for (_, output) in &extracted {
        let _ = Resolve::<CallF>::resolve(&RustSource, output, &cx);
    }

    assert_eq!(
        reads.load(Ordering::SeqCst),
        DOCUMENTS.len(),
        "the join runs once per project; {} files x {} documents is the quadratic it replaced",
        FILES.len(),
        DOCUMENTS.len()
    );
}
