//! CLI defects from the 2026-08-28 corpus crawls (go/rust REPORT kinks).
//! One test per defect; each ran red before the fix that closed it.

use std::process::Command;

fn temp_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("extract50-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp root creates");
    dir
}

/// Defect 1: `--scip-facts --project-root X --scip-index Y X` exits 2 with
/// "is a directory" because `check_file_paths` ran for a mode whose PATH is a
/// root. The dir arg must reach the library (rc != 2, no "is a directory").
#[test]
fn scip_facts_takes_a_root_directory() {
    let root = temp_root("scip-facts-root");
    std::fs::write(root.join("a.go"), "package a\nfunc A() {}\n").unwrap();
    let index = root.join("index.scip");
    std::fs::write(&index, b"not a real index").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args([
            "--scip-facts",
            "--project-root",
            root.to_str().unwrap(),
            "--scip-index",
            index.to_str().unwrap(),
            root.to_str().unwrap(),
        ])
        .output()
        .expect("extract binary runs");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_ne!(
        output.status.code(),
        Some(2),
        "dir arg must not hit the file-path check; stderr: {stderr}"
    );
    assert!(!stderr.contains("is a directory"), "stderr: {stderr}");
}

/// Defect 1: `--scip-deps` has the same PATH-is-a-root contract.
#[test]
fn scip_deps_takes_a_root_directory() {
    let root = temp_root("scip-deps-root");
    std::fs::write(root.join("a.go"), "package a\nfunc A() {}\n").unwrap();
    let index = root.join("index.scip");
    std::fs::write(&index, b"not a real index").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args([
            "--scip-deps",
            "--project-root",
            root.to_str().unwrap(),
            "--scip-index",
            index.to_str().unwrap(),
            root.to_str().unwrap(),
        ])
        .output()
        .expect("extract binary runs");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_ne!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(!stderr.contains("is a directory"), "stderr: {stderr}");
}

/// Defect 1: modes whose PATH is a FILE keep the check. `--resolve` on a
/// directory must still stop with exit 2.
#[test]
fn resolve_on_a_directory_still_exits_2() {
    let root = temp_root("resolve-dir");
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(["--resolve", root.to_str().unwrap()])
        .output()
        .expect("extract binary runs");
    assert_eq!(output.status.code(), Some(2));
}

/// Defect 2: `extract <file> | head -1` panicked on the closed pipe
/// ("failed printing to stdout", rc 101). Early close is a clean exit 0 with
/// nothing on stderr, from the BufWriter path AND the println! rows.
#[test]
fn broken_pipe_exits_0_silently() {
    let root = temp_root("broken-pipe");
    let file = root.join("pipe.ts");
    let mut body = String::new();
    for i in 0..5000 {
        body.push_str(&format!(
            "export function f{i}(n: number): number {{ return n + {i}; }}\n"
        ));
    }
    std::fs::write(&file, body).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg(file.to_str().unwrap())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("extract binary runs");
    {
        let mut stdout = child.stdout.take().expect("stdout piped");
        use std::io::Read;
        let mut first = [0u8; 1];
        stdout.read_exact(&mut first).expect("first stdout byte");
    }
    // Dropping the read end: the child's next write hits EPIPE.
    let stderr = child
        .stderr
        .take()
        .map(|mut s| {
            let mut text = String::new();
            use std::io::Read;
            let _ = s.read_to_string(&mut text);
            text
        })
        .unwrap_or_default();
    let status = child.wait().expect("child waits");
    assert_eq!(status.code(), Some(0), "stderr: {stderr}");
    assert!(stderr.is_empty(), "stderr: {stderr}");
}
