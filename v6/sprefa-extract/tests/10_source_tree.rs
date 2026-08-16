use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use sprefa_extract::{diet_scip, BlobSource, SourcePattern, SourceRevision, SourceTreeBlobSource};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// A temp dir unique across the parallel tests in this file (the clock's
/// `as_nanos` is not: macOS rounds it coarsely enough that two threads collide
/// on `git init`, which then fails copying `.git/info/exclude`).
fn unique_temp(tag: &str) -> std::path::PathBuf {
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "sprefa_extract_source_tree_{tag}_{}_{seq}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn fixture() -> std::path::PathBuf {
    let root = unique_temp("plain");
    std::fs::create_dir_all(root.join("src")).unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "sprefa-extract")
            .env("GIT_AUTHOR_EMAIL", "sprefa-extract@example.invalid")
            .env("GIT_COMMITTER_NAME", "sprefa-extract")
            .env("GIT_COMMITTER_EMAIL", "sprefa-extract@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-q"]);
    std::fs::write(root.join("src/lib.rs"), "pub const VERSION: u8 = 1;\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "first"]);
    std::fs::write(root.join("src/lib.rs"), "pub const VERSION: u8 = 2;\n").unwrap();
    root
}

#[test]
fn extractor_reads_the_revision_selected_by_source_tree() {
    let root = fixture();
    let source = SourceTreeBlobSource::open(
        &root,
        SourceRevision::Named(Arc::from("HEAD")),
        &[SourcePattern("**/*.rs".into())],
    )
    .unwrap();
    let entries: Vec<_> = source.entries().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source.path.0.as_ref(), "src/lib.rs");
    assert_eq!(
        source.blob("src/lib.rs").unwrap(),
        b"pub const VERSION: u8 = 1;\n"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn worktree_source_reads_dirty_and_untracked_files() {
    let root = fixture();
    // `fixture` leaves src/lib.rs dirty (= 2) against a committed = 1, and now
    // adds a file Git has never tracked.
    std::fs::write(root.join("src/untracked.rs"), "pub const EXTRA: u8 = 9;\n").unwrap();

    let source =
        SourceTreeBlobSource::open_worktree(&root, &[SourcePattern("**/*.rs".into())]).unwrap();
    let mut paths: Vec<String> = source
        .entries()
        .map(|entry| entry.source.path.0.as_ref().to_string())
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        vec!["src/lib.rs".to_string(), "src/untracked.rs".to_string()]
    );

    // The default worktree mode reads current disk: dirty content, not HEAD.
    assert_eq!(
        source.blob("src/lib.rs").unwrap(),
        b"pub const VERSION: u8 = 2;\n"
    );
    // The untracked file is visible and readable through the fs-glob snapshot.
    assert_eq!(
        source.blob("src/untracked.rs").unwrap(),
        b"pub const EXTRA: u8 = 9;\n"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// The FAIL-PRE-FIX receipt: soopy's tracked-file surface (`git ls-files`)
/// omits the untracked file, which is exactly why the default must be the
/// fs-glob worktree snapshot, not the tracked enumeration.
#[test]
fn git_files_enumeration_misses_untracked_files() {
    let root = fixture();
    std::fs::write(root.join("src/untracked.rs"), "pub const EXTRA: u8 = 9;\n").unwrap();
    let output = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "-z"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let listed = String::from_utf8(output.stdout).unwrap();
    assert!(
        listed.contains("src/lib.rs"),
        "tracked file must be listed: {listed}"
    );
    assert!(
        !listed.contains("untracked.rs"),
        "a tracked-file enumeration sees the untracked file, so it cannot be the FAIL-PRE-FIX: {listed}"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn worktree_source_normalizes_a_subdirectory_project_root() {
    let root = monorepo_fixture();
    // The project root is a subdirectory of the Git root, the normal monorepo
    // SCIP shape. The reader is addressed by project-relative path.
    let source =
        SourceTreeBlobSource::open_worktree(root.join("pkg"), &[SourcePattern("**/*.rs".into())])
            .unwrap();
    assert_eq!(
        source.blob("src/lib.rs").unwrap(),
        b"pub const PKG: u8 = 1;\n"
    );
    // A file outside the project root is not in the corpus.
    assert_eq!(source.blob("other.rs"), None);
    std::fs::remove_dir_all(root).unwrap();
}

fn monorepo_fixture() -> std::path::PathBuf {
    let root = unique_temp("monorepo");
    std::fs::create_dir_all(root.join("pkg/src")).unwrap();
    std::fs::create_dir_all(root.join("other/src")).unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "sprefa-extract")
            .env("GIT_AUTHOR_EMAIL", "sprefa-extract@example.invalid")
            .env("GIT_COMMITTER_NAME", "sprefa-extract")
            .env("GIT_COMMITTER_EMAIL", "sprefa-extract@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-q"]);
    std::fs::write(root.join("pkg/src/lib.rs"), "pub const PKG: u8 = 1;\n").unwrap();
    std::fs::write(root.join("other/src/lib.rs"), "pub const OTHER: u8 = 2;\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "first"]);
    root
}

/// A corpus outside any Git repository has no revision coordinate for soopy to
/// enumerate, so `read_inputs` falls back to the plain filesystem read. This is
/// the input-domain regression guard: extraction must not start requiring Git.
#[test]
fn plain_directory_corpus_without_git_still_extracts() {
    let root = unique_temp("nogit");
    std::fs::write(
        root.join("0_caller.ts"),
        "export function run() {\n  return helper();\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("1_callee.ts"),
        "export function helper() {\n  return 7;\n}\n",
    )
    .unwrap();
    let facts = diet_scip(&[root.join("0_caller.ts"), root.join("1_callee.ts")]).unwrap();
    assert!(
        !facts.is_empty(),
        "a plain (non-Git) directory must still resolve to facts"
    );
    std::fs::remove_dir_all(root).unwrap();
}
