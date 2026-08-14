use std::process::Command;
use std::sync::Arc;

use sprefa_extract::{BlobSource, SourcePattern, SourceRevision, SourceTreeBlobSource};

fn fixture() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "sprefa_extract_source_tree_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
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
