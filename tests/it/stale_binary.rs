//! e2e: the stale-binary rail (failure-modes class 13 promotion,
//! docs/failure-modes.md:300-327): `dl daemon status` (one of the three
//! named surfaces alongside daemon start and `--check`) warns once when a
//! checkout that builds `dl` itself carries a `target/{debug,release}/dl`
//! newer than the binary actually running.
//!
//! Since the it-suite's own `CARGO_BIN_EXE_dl` is a real binary with a fixed
//! build-time mtime, the fixture fabricates the OTHER side of the compare: a
//! throwaway `Cargo.toml` naming this crate's own published package
//! (`sprefa-dl`, `env!("CARGO_PKG_NAME")` at compile time) plus a
//! `target/debug/dl` stand-in file whose mtime is stamped either after or
//! well before the running exe's — `std::fs::File::set_modified` (stable,
//! no `filetime` crate needed for one test file's `touch`).
//!
//! HERMETICITY: `SPREFA_CONFIG` points at a nonexistent path; `XDG_STATE_HOME`
//! is a per-test sandbox so `dl daemon status`'s (unreachable, never spawned)
//! singleton probe never touches a developer's real state dir.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime};

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_stale_binary_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("target/debug")).unwrap();
    dir
}

/// A `Cargo.toml` naming this crate's own package, plus a `target/debug/dl`
/// stand-in stamped `newer` (true) or well before (false) the real running
/// exe's mtime.
fn write_fixture(dir: &PathBuf, newer: bool) {
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"sprefa-dl\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let candidate = dir.join("target/debug/dl");
    fs::write(&candidate, b"not a real binary; only its mtime is read").unwrap();
    let running_mtime = fs::metadata(DL).unwrap().modified().unwrap();
    let stamp = if newer {
        running_mtime + Duration::from_secs(3600)
    } else {
        running_mtime.checked_sub(Duration::from_secs(3600)).unwrap_or(SystemTime::UNIX_EPOCH)
    };
    fs::File::open(&candidate).unwrap().set_modified(stamp).unwrap();
}

fn run_status(dir: &PathBuf, extra_envs: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(DL);
    cmd.args(["daemon", "status"])
        .current_dir(dir)
        .env("XDG_STATE_HOME", dir.join("xdg"))
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml");
    for (key, value) in extra_envs {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("run dl daemon status");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Fail-pre-fix shape: before this rail, `dl daemon status` never inspected
/// `target/` at all, so this line could never appear regardless of build
/// freshness — the assertion is a positive existence check, not a smell.
#[test]
fn daemon_status_warns_when_repo_build_is_newer() {
    let dir = sandbox("status_newer");
    write_fixture(&dir, true);
    let stderr = run_status(&dir, &[]);
    assert!(
        stderr.lines().any(|l| l.contains("installed dl is older than this repo's build")),
        "expected a stale-binary warning, got:\n{stderr}"
    );
    assert!(
        stderr.contains("cargo install --path . to deploy"),
        "missing the remediation hint:\n{stderr}"
    );
}

#[test]
fn daemon_status_silent_when_repo_build_is_older() {
    let dir = sandbox("status_older");
    write_fixture(&dir, false);
    let stderr = run_status(&dir, &[]);
    assert!(
        !stderr.lines().any(|l| l.contains("installed dl is older than this repo's build")),
        "an OLDER repo build must not warn, got:\n{stderr}"
    );
}

#[test]
fn dl_no_stale_warn_suppresses() {
    let dir = sandbox("suppress");
    write_fixture(&dir, true);
    let stderr = run_status(&dir, &[("DL_NO_STALE_WARN", "1")]);
    assert!(
        !stderr.lines().any(|l| l.contains("installed dl is older than this repo's build")),
        "DL_NO_STALE_WARN=1 must suppress the warning, got:\n{stderr}"
    );
}
