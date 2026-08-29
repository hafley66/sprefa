//! Kink regression: `--scip-build` must honor `--scip-timeout` on the go arm
//! and surface the kill as one named `scip_skip` row, exit 0. Reported against
//! the go corpus crawl (plans/extract-crawl-2026-08-29/go.REPORT.md, kink row 5)
//! where the run was killed by the OUTER timeout at rc 124 with an empty
//! stream. The budget now reaches every build path (the CLI threads
//! `--scip-timeout` through `IndexBudget`), so this test pins the fixed
//! behavior rather than driving a fix.

use std::process::Command;

/// A go module whose indexer sleeps 30s: killed at the 2s budget, rc 0,
/// exactly one `scip_skip` row with reason `timed_out`. The planted script is
/// first on PATH for the child process only. Wall assert: under 5s.
#[test]
#[cfg(unix)]
fn scip_build_honors_scip_timeout_on_the_go_arm() {
    use std::time::Instant;

    let scratch = tempfile_dir();
    let bin_dir = scratch.join("bin");
    let root = scratch.join("mod");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("go.mod"),
        "module example.com/kink\n\ngo 1.21\n",
    )
    .unwrap();
    std::fs::write(root.join("kink.go"), "package kink\n\nfunc F() {}\n").unwrap();

    let planted = bin_dir.join("scip-go");
    std::fs::write(&planted, "#!/bin/sh\nsleep 30\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .env("PATH", format!("{}:/bin:/usr/bin", bin_dir.display()))
        .env_remove("SPREFA_SCIP_TIMEOUT_SECS")
        .args([
            "--family",
            "scip",
            "--scip-build",
            "--scip-timeout",
            "2",
            "--project-root",
            &root.to_string_lossy(),
            &root.to_string_lossy(),
        ])
        .output()
        .expect("extract binary runs");
    let wall = started.elapsed();

    assert!(output.status.success(), "a timeout skips, it does not fail");
    assert!(
        wall.as_secs() < 5,
        "the run must return near its 2s budget, not the planted 30s sleep; took {wall:?}"
    );
    let stream = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(
        stream,
        "{\"record\":\"scip_skip\",\"lang\":\"go\",\"bin\":\"scip-go\",\
         \"reason\":\"timed_out\",\"detail\":\"exceeded the 2s budget; process \
         group killed\"}\n",
        "exactly one named skip row must ride the stream"
    );
}

/// A unique scratch dir under the OS temp dir: pid + nanos, no tempfile dep
/// (same shape as the crate's own `fresh_temp_dir`).
fn tempfile_dir() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("scip-kinks-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
