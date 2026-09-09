use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use sprefa_extract::{
    content_id_of, diet_scip, diet_scip_with_raw, dispatch, file_fact, flatten, resolve_project,
    resolve_project_with_raw, FamilyMask, FlatFact, ResolveArms, ResolveRequest,
    ResolveWithRawError, ScipMode, ScipRecords,
};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn fixture() -> (PathBuf, Vec<PathBuf>, Vec<u8>) {
    let root = std::env::temp_dir().join(format!(
        "sprefa_extract_resolve_raw_{}_{}",
        std::process::id(),
        TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let content = b"pub fn same() -> u8 { 1 }\n".to_vec();
    let paths = [root.join("0_same.rs"), root.join("1_same.rs")];
    for path in &paths {
        std::fs::write(path, &content).unwrap();
    }
    (root, paths.into(), content)
}

fn request(paths: &[PathBuf]) -> ResolveRequest<'_> {
    ResolveRequest {
        paths,
        arms: ResolveArms {
            call: true,
            types: true,
            flow: false,
        },
        scip: ScipMode::Off,
        project_root: None,
        scip_records: ScipRecords::all(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        go_checker: None,
        witness: false,
    }
}

fn json(facts: Vec<FlatFact>) -> Vec<String> {
    facts
        .iter()
        .map(|fact| serde_json::to_string(fact).unwrap())
        .collect()
}

#[test]
fn raw_sink_gets_file_and_syntax_facts_from_the_resolve_inputs() {
    let (root, paths, content) = fixture();
    let expected_resolved = json(resolve_project(&request(&paths)).unwrap());
    let digest = content_id_of(&content).to_string();
    let mut expected_raw = Vec::new();
    for path in &paths {
        let path = path.to_string_lossy().to_string();
        let output = dispatch(&path, &content, FamilyMask::ALL).unwrap();
        expected_raw.push((
            path.clone(),
            digest.clone(),
            serde_json::to_string(&file_fact(&path, &content)).unwrap(),
        ));
        expected_raw.extend(flatten(&output).into_iter().map(|fact| {
            (
                path.clone(),
                digest.clone(),
                serde_json::to_string(&fact).unwrap(),
            )
        }));
    }
    let mut raw = Vec::new();

    let resolved = resolve_project_with_raw(&request(&paths), &mut |row| {
        raw.push((
            row.path.to_string(),
            row.content_id.to_string(),
            serde_json::to_string(&row.fact).unwrap(),
        ));
        Ok::<(), std::convert::Infallible>(())
    })
    .unwrap();

    assert_eq!(json(resolved), expected_resolved);
    assert_eq!(raw, expected_raw);
    assert_ne!(raw[0].0, raw[expected_raw.len() / 2].0);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn diet_raw_wrapper_preserves_the_resolved_answer_and_sink_errors() {
    let (root, paths, _) = fixture();
    let expected = json(diet_scip(&paths).unwrap());
    let mut rows = 0usize;
    let actual = diet_scip_with_raw(&paths, &mut |_| {
        rows += 1;
        Ok::<(), std::convert::Infallible>(())
    })
    .unwrap();
    assert_eq!(json(actual), expected);
    assert!(rows > paths.len());

    let mut attempts = 0;
    let failure = resolve_project_with_raw(&request(&paths), &mut |_| {
        attempts += 1;
        Err("closed")
    });
    assert!(matches!(
        failure,
        Err(ResolveWithRawError::RawSink("closed"))
    ));
    assert_eq!(attempts, 1);

    std::fs::remove_dir_all(root).unwrap();
}
