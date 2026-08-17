use std::process::Command;

fn run_in(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("extract binary runs")
}

fn git(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("git runs")
}

fn temp_repo() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "blobdoor_query_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    dir
}

#[test]
fn query_reads_a_staged_blob_through_the_batch_door() {
    let dir = temp_repo();
    let source = dir.join("sample.rs");
    std::fs::write(
        &source,
        "pub fn trim(value: String) -> String {\n    value\n}\n",
    )
    .unwrap();
    let hash = git(&dir, &["hash-object", "-w", source.to_str().unwrap()]);
    let oid = String::from_utf8(hash.stdout).unwrap().trim().to_string();

    let output = run_in(
        &dir,
        &[
            "query",
            "--lang",
            "rust",
            "--query",
            "(function_item name: (identifier) @name) @item",
            "--digest",
            &oid,
            "label.rs",
        ],
    );
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"name\":\"trim\""), "stdout: {stdout}");
}

#[test]
fn query_rejects_a_non_blob_oid_with_one_stderr_line() {
    let dir = temp_repo();
    let source = dir.join("sample.rs");
    std::fs::write(
        &source,
        "pub fn trim(value: String) -> String {\n    value\n}\n",
    )
    .unwrap();
    git(&dir, &["add", "."]);
    let tree = git(&dir, &["write-tree"]);
    let tree_oid = String::from_utf8(tree.stdout).unwrap().trim().to_string();

    let output = run_in(
        &dir,
        &[
            "query",
            "--lang",
            "rust",
            "--query",
            "(identifier) @name",
            "--digest",
            &tree_oid,
            "label.rs",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 1, "stderr: {stderr}");
    assert!(stderr.contains("git cat-file blob"), "stderr: {stderr}");
}
