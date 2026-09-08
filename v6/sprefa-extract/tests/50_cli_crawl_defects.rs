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

fn run_failed_rust_indexer(
    name: &str,
    script_body: &str,
    rust_log: Option<&str>,
    log_format: &str,
) -> std::process::Output {
    let root = temp_root(name);
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='failed-indexer'\nversion='0.0.0'\nedition='2021'\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn probe() {}\n").unwrap();
    let bin_dir = root.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let indexer = bin_dir.join("rust-analyzer");
    std::fs::write(&indexer, format!("#!/bin/sh\n{script_body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&indexer, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut command = Command::new(env!("CARGO_BIN_EXE_extract"));
    command
        .env("PATH", fake_path(&indexer))
        .env("HAFLEY_LOG_FORMAT", log_format)
        .env("DL_TRAIL", "0")
        .args([
            "slow",
            "--scip-cache",
            root.join("cache").to_str().unwrap(),
            root.to_str().unwrap(),
        ]);
    if let Some(filter) = rust_log {
        command.env("RUST_LOG", filter);
    } else {
        command.env_remove("RUST_LOG");
    }
    command.output().expect("extract binary runs")
}

/// A failed indexer can put the root cause at the start of stderr and finish
/// with thousands of stack frames. The skip row retains both bounded ends plus
/// the command/status, and the default warning telemetry reports the failure
/// without contaminating stdout's JSONL protocol.
#[test]
fn failed_indexer_retains_bounded_root_cause_tail_status_and_telemetry() {
    let root = temp_root("scip-failed-evidence");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='failed-evidence'\nversion='0.0.0'\nedition='2021'\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn probe() {}\n").unwrap();
    let bin_dir = root.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let indexer = bin_dir.join("rust-analyzer");
    std::fs::write(
        &indexer,
        "#!/bin/sh\n\
         printf 'root cause: cargo metadata π failed\\n' >&2\n\
         i=0\n\
         while [ \"$i\" -lt 12000 ]; do printf 'é' >&2; i=$((i + 1)); done\n\
         printf '\\n7: worker_pool\\n8: __pthread_joiner_wake\\n' >&2\n\
         exit 17\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&indexer, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let cache = root.join("cache");
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .env("PATH", fake_path(&indexer))
        .env_remove("RUST_LOG")
        .env("HAFLEY_LOG_FORMAT", "json")
        .env("DL_TRAIL", "0")
        .args([
            "slow",
            "--scip-cache",
            cache.to_str().unwrap(),
            root.to_str().unwrap(),
        ])
        .output()
        .expect("extract binary runs");

    assert_eq!(output.status.code(), Some(0));
    let rows = String::from_utf8(output.stdout).expect("fact stream is UTF-8");
    let row: serde_json::Value = serde_json::from_str(rows.trim()).expect("one skip row");
    assert_eq!(row["record"], "scip_skip");
    assert_eq!(row["reason"], "failed");
    let detail = row["detail"].as_str().expect("skip detail");
    assert!(detail.contains("command [\"rust-analyzer\", \"scip\", \".\", \"--output\""));
    assert!(detail.contains("exited with code 17"));
    assert!(detail.contains("root cause: cargo metadata π failed"));
    assert!(detail.contains("stderr bytes omitted"));
    assert!(detail.ends_with("8: __pthread_joiner_wake"));
    assert!(!detail.contains('\u{fffd}'), "UTF-8 window boundary was split: {detail}");
    assert!(detail.len() < 17_000, "stderr evidence was not bounded: {}", detail.len());

    let events = String::from_utf8(output.stderr).expect("telemetry is UTF-8");
    let failed = events
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON event"))
        .find(|event| event["fields"]["message"] == "indexer process failed")
        .expect("default warning telemetry");
    assert_eq!(failed["fields"]["process.command"], "rust-analyzer");
    assert_eq!(failed["fields"]["process.status"], "exited with code 17");
    assert!(failed["fields"]["process.pid"].as_u64().is_some());
    assert!(failed["fields"]["duration_ms"].as_u64().is_some());
}

#[test]
fn failed_indexer_with_empty_stderr_keeps_stdout_jsonl_and_human_telemetry_separate() {
    let output = run_failed_rust_indexer("scip-empty-stderr", "exit 19", None, "human");
    assert_eq!(output.status.code(), Some(0));
    let row: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout remains one JSONL value");
    assert_eq!(row["record"], "scip_skip");
    assert!(row["detail"]
        .as_str()
        .unwrap()
        .contains("exited with code 19; stderr was empty"));
    let telemetry = String::from_utf8(output.stderr).expect("human telemetry is UTF-8");
    assert!(telemetry.contains("indexer process failed"));
    assert!(serde_json::from_str::<serde_json::Value>(telemetry.trim()).is_err());
}

#[test]
fn rust_log_off_suppresses_failure_telemetry_without_suppressing_skip_detail() {
    let output = run_failed_rust_indexer(
        "scip-rust-log-off",
        "printf 'small root cause\\n' >&2\nexit 21",
        Some("off"),
        "json",
    );
    assert_eq!(output.status.code(), Some(0));
    let row: serde_json::Value = serde_json::from_slice(&output.stdout).expect("skip JSONL");
    assert!(row["detail"].as_str().unwrap().contains("small root cause"));
    assert!(output.stderr.is_empty(), "RUST_LOG=off must suppress stderr");
}

#[cfg(unix)]
#[test]
fn signal_terminated_indexer_reports_signal_and_small_stderr() {
    let output = run_failed_rust_indexer(
        "scip-signal",
        "printf 'signal root cause\\n' >&2\nkill -TERM $$",
        None,
        "json",
    );
    assert_eq!(output.status.code(), Some(0));
    let row: serde_json::Value = serde_json::from_slice(&output.stdout).expect("skip JSONL");
    let detail = row["detail"].as_str().unwrap();
    assert!(detail.contains("terminated by signal 15"), "{detail}");
    assert!(detail.contains("signal root cause"), "{detail}");
    let events = String::from_utf8(output.stderr).expect("JSON telemetry is UTF-8");
    let failed = events
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON event"))
        .find(|event| event["fields"]["message"] == "indexer process failed")
        .expect("failure event");
    assert_eq!(failed["fields"]["process.status"], "terminated by signal 15");
}

#[test]
fn legacy_last_error_line_still_skips_a_trailing_panic_note() {
    assert_eq!(
        sprefa_extract::scip_ensure::last_error_line(
            "panic root\n8: __pthread_joiner_wake\nnote: run with RUST_BACKTRACE=1\n"
        ),
        "8: __pthread_joiner_wake"
    );
}
