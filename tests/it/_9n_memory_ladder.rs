//! `dl8 compile` peak memory over one TSI stream does not grow with the run's scope list.
//! The owner term carries the scope and every row names the owner, so a compile that builds
//! its output as a value tree grows with scope × rows (grapht: killed at 8 GB).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SCOPES: [usize; 3] = [1, 512, 4096];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The fixture stream with its run scope widened to `scope` digests.
fn stream(scope: usize, directory: &Path) -> PathBuf {
    let text = std::fs::read_to_string(root().join("tests/fixtures/tsi/1_semantic_user.jsonl")).unwrap();
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let mut record: serde_json::Value = serde_json::from_str(line).unwrap();
            if record["record"] == "run" {
                let mut digests = vec!["blake3:user".to_string()];
                digests.extend((1..scope).map(|index| format!("blake3:{index:064x}")));
                record["scope"] = serde_json::json!(digests);
            }
            record.to_string()
        })
        .collect();
    let path = directory.join(format!("scope{scope}.jsonl"));
    std::fs::write(&path, lines.join("\n")).unwrap();
    path
}

/// The child's peak resident size from `wait4` (bytes on macOS, KiB on Linux) and its
/// stdout length.
fn compile_peak(tsi: &Path, out: &Path) -> (i64, u64) {
    let child = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(root())
        .args(["compile", "fixtures/extract/main.dl7", "--project", "fixtures/extract", "--tsi"])
        .arg(tsi)
        .stdout(Stdio::from(std::fs::File::create(out).unwrap()))
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut status = 0;
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let waited = unsafe { libc::wait4(child.id() as libc::pid_t, &mut status, 0, &mut usage) };
    assert_eq!(waited, child.id() as libc::pid_t);
    assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0, "dl8 compile failed");
    (usage.ru_maxrss, std::fs::metadata(out).unwrap().len())
}

#[test]
fn compile_peak_is_flat_in_scope_length() {
    let directory = std::env::temp_dir().join(format!("dl8-memory-ladder-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let ladder: Vec<(usize, i64, u64)> = SCOPES
        .iter()
        .map(|&scope| {
            let tsi = stream(scope, &directory);
            let (peak, bytes) = compile_peak(&tsi, &directory.join(format!("scope{scope}.json")));
            (scope, peak, bytes)
        })
        .collect();
    std::fs::remove_dir_all(&directory).unwrap();
    let (_, smallest, _) = ladder[0];
    let (_, largest, _) = ladder[ladder.len() - 1];
    assert!(
        largest * 2 < smallest * 3,
        "peak grows with scope (scope, peak, output bytes): {ladder:?}"
    );
}
