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

/// Defect 3: `--scip-build` ran `scip-go .` (root package only: a 158-byte
/// index on typescript-go against 106 MB for `./...`). The go argv must
/// enumerate the whole tree; the other indexers already walk it.
#[test]
fn go_indexer_argv_enumerates_all_packages() {
    use sprefa_extract::scip::{GO_SPEC, PYTHON_SPEC, TS_SPEC};
    assert_eq!(GO_SPEC.args.last(), Some(&"./..."));
    assert!(TS_SPEC.args.contains(&"index"), "ts indexes the project");
    assert!(PYTHON_SPEC.args.contains(&"."), "python indexes the tree");
}

/// Defect 4: `--scip-build` ignored `--scip-timeout`; the load_scip budget
/// came from the env only, so a slow indexer ran to the 600 s default. A fake
/// sleeping indexer under a 1 s budget must produce a timed-out skip row
/// (`--family scip`) and a fail-fast error (`--scip-build`), never a hang.
#[test]
fn scip_timeout_caps_the_family_scip_build() {
    let root = temp_root("scip-timeout-family");
    std::fs::write(root.join("go.mod"), "module fake\n\ngo 1.21\n").unwrap();
    std::fs::write(root.join("main.go"), "package main\nfunc main() {}\n").unwrap();
    let bin = fake_sleeper(&root);
    let cache = root.join("cache");

    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .env("PATH", fake_path(&bin))
        .args([
            "--family",
            "scip",
            "--scip-cache",
            cache.to_str().unwrap(),
            "--scip-timeout",
            "1",
            root.to_str().unwrap(),
        ])
        .output()
        .expect("extract binary runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(r#""record":"scip_skip""#) && stdout.contains("timed_out"),
        "expected a scip_skip timed_out row; stdout: {stdout}"
    );
}

#[test]
fn scip_timeout_caps_the_scip_build_flag() {
    let root = temp_root("scip-timeout-build");
    std::fs::write(root.join("main.go"), "package main\nfunc main() {}\n").unwrap();
    std::fs::write(root.join("go.mod"), "module fake\n\ngo 1.21\n").unwrap();
    let bin = fake_sleeper(&root);
    let file = root.join("main.go");

    let mut child = Command::new(env!("CARGO_BIN_EXE_extract"))
        .env("PATH", fake_path(&bin))
        .args([
            "--scip-facts",
            "--scip-build",
            "--project-root",
            root.to_str().unwrap(),
            "--scip-timeout",
            "1",
            file.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("extract binary runs");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut stderr = String::new();
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => {
                use std::io::Read;
                if let Some(mut s) = child.stderr.take() {
                    let _ = s.read_to_string(&mut stderr);
                }
                assert!(
                    !status.success(),
                    "a timed-out build cannot yield facts; stderr: {stderr}"
                );
                assert!(
                    stderr.contains("exceeded the 1s budget"),
                    "expected the timed-out skip detail; stderr: {stderr}"
                );
                return;
            }
            None => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("--scip-build ignored --scip-timeout: still running after 15s");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

/// A `scip-go` stand-in on PATH that sleeps past any test budget.
fn fake_sleeper(root: &std::path::Path) -> std::path::PathBuf {
    let bin = root.join("fake-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("scip-go");
    std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

/// PATH = the fake bin dir plus whatever the test host needs.
fn fake_path(fake: &std::path::Path) -> String {
    let host = std::env::var("PATH").unwrap_or_default();
    format!("{}:{host}", fake.parent().unwrap().display())
}

/// Defect 5 (rust.REPORT kink 8): a failed rust-analyzer build carried the
/// panic's `note: Some details are omitted, run with RUST_BACKTRACE=1` line as
/// scip_skip.detail instead of the panic line before it.
#[test]
fn error_line_skips_panic_notes() {
    let stderr = "thread 'rust-analyzer' panicked at src/x.rs:12:5:\n\
                  explicit panic at the real call site\n\
                  note: Some details are omitted, run with `RUST_BACKTRACE=1` \
                  environment variable to display a backtrace\n";
    assert_eq!(
        sprefa_extract::scip_ensure::last_error_line(stderr),
        "explicit panic at the real call site"
    );
    // No note lines: the last line wins unchanged.
    assert_eq!(
        sprefa_extract::scip_ensure::last_error_line("first\nlast\n"),
        "last"
    );
    // Only note lines: fall back to the last one rather than empty.
    assert!(sprefa_extract::scip_ensure::last_error_line(
        "note: only\n"
    )
    .starts_with("note:"));
}
