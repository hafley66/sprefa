//! `dl --hook` self-timeout. An agent-harness hook has about one second to
//! answer; dl must let go of the call itself: on `DL_HOOK_DEADLINE_MS` expiry
//! the process emits the no-op reply (empty stdout, exit 0) instead of making
//! the harness wait, a blank/absent-db in-process run is skipped outright (a
//! cold build can never fit the deadline), and the hook path never autostarts
//! a daemon. `DL_HOOK_DEADLINE_MS=0` is the internal test-harness escape that
//! disables both the deadline and the cold-db skip (the other `--hook`
//! it-suites run under it).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const DL: &str = env!("CARGO_BIN_EXE_dl");

const EVENT: &str = r#"{"hook_event_name":"PostToolUse","session_id":"sess1"}"#;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hookdl_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::canonicalize(&dir).unwrap()
}

/// Hermetic env: sandboxed XDG state (daemon socket + invlog), no ambient
/// config, no real Claude/opencode transcript stores.
fn hermetic(cmd: &mut Command, dir: &Path) {
    cmd.env("SPREFA_CONFIG", dir.join("no-config.toml"))
        .env("XDG_STATE_HOME", dir.join("state"))
        .env("SPREFA_CLAUDE_PROJECTS", dir.join("cc-store"))
        .env("SPREFA_OPENCODE_DB", dir.join("no-opencode.db"));
}

/// Spawn `dl --hook` with `extra_args` + `envs`, feed EVENT on stdin, wait.
fn run_hook(dir: &Path, extra_args: &[&str], envs: &[(&str, &str)]) -> (Output, Duration) {
    let mut cmd = Command::new(DL);
    cmd.arg("--hook")
        .arg(dir.join("p.dl"))
        .args(extra_args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hermetic(&mut cmd, dir);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let started = Instant::now();
    let mut child = cmd.spawn().expect("spawn dl --hook");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(EVENT.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("wait dl --hook");
    (out, started.elapsed())
}

/// Work that exceeds the deadline (the DL_TEST_HANG_SECS injected slow point,
/// same seam the wall watchdog test uses) must yield the no-op: exit 0, empty
/// stdout, and the whole invocation done well under the hang duration —
/// bounded by ~2x the deadline.
#[test]
fn overrun_work_returns_noop_exit0_within_2x_deadline() {
    let dir = sandbox("overrun");
    fs::write(dir.join("p.dl"), "rel inject(text: text).\n").unwrap();
    let db = dir.join("db");
    let (out, elapsed) = run_hook(
        &dir,
        &["--db", db.to_str().unwrap(), "--no-daemon"],
        &[("DL_TEST_HANG_SECS", "10"), ("DL_HOOK_DEADLINE_MS", "1000")],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "deadline give-up must exit 0:\n{stderr}"
    );
    assert!(
        stdout.trim().is_empty(),
        "deadline give-up must be a no-op reply:\n{stdout}"
    );
    assert!(
        elapsed < Duration::from_millis(2000),
        "hook must let go at the deadline (~2x budget), took {elapsed:?}:\n{stderr}"
    );
    assert!(
        stderr.contains("deadline"),
        "give-up names itself on stderr:\n{stderr}"
    );
}

/// A blank/absent db means the in-process fallback would be a COLD engine; the
/// hook skips the work outright (no-op) instead of starting a build that can
/// never fit the deadline. `DL_HOOK_DEADLINE_MS=0` (the test-harness escape)
/// proves the guard was the only reason for silence: the same run then does
/// the cold work and injects.
#[test]
fn cold_blank_db_skips_work_and_zero_deadline_escape_runs_it() {
    let dir = sandbox("cold");
    let git = |args: &[&str]| {
        assert!(Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    };
    git(&["init", "-q"]);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/foo.rs"), "fn f() {}\n").unwrap();
    fs::write(
        dir.join("p.dl"),
        concat!(
            "rel src_file(path: file).\n",
            "src_file(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev).\n",
            "rel inject(text: text).\n",
            "inject(\"corpus seen\") <- src_file(_).\n",
        ),
    )
    .unwrap();

    // No --db: the in-process fallback would open an in-memory (blank) engine.
    let (out, _) = run_hook(&dir, &["--no-daemon"], &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "cold skip must exit 0:\n{stderr}"
    );
    assert!(
        stdout.trim().is_empty(),
        "cold skip must be a no-op reply:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("cold"),
        "cold skip names itself on stderr:\n{stderr}"
    );

    // Escape hatch: deadline 0 disables the deadline AND the cold skip.
    let (out, _) = run_hook(&dir, &["--no-daemon"], &[("DL_HOOK_DEADLINE_MS", "0")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("additionalContext") && stdout.contains("corpus seen"),
        "deadline 0 must run the cold work and inject:\n{stdout}"
    );
}

/// A daemon-eligible root (`.dl/` dir, daemon enabled) with NO daemon running:
/// the hook must not spawn one — it falls to the in-process arm (here: cold
/// skip) and leaves the sandboxed daemon home without a socket.
#[test]
fn hook_never_autostarts_a_daemon() {
    let dir = sandbox("noautostart");
    fs::create_dir_all(dir.join(".dl")).unwrap();
    fs::write(dir.join("p.dl"), "rel inject(text: text).\n").unwrap();
    // Positional program + no --db keeps the run daemon-eligible; DL_NO_DAEMON
    // deliberately NOT set.
    let (out, _) = run_hook(&dir, &[], &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "no-daemon hook run must exit 0:\n{stderr}"
    );
    assert!(stdout.trim().is_empty(), "no-op reply expected:\n{stdout}");
    assert!(
        stderr.contains("never starts one"),
        "attach-only notice expected:\n{stderr}"
    );
    // The sandboxed daemon home must hold no socket: nothing was spawned.
    let home = dir.join("state/sprefa");
    let socks: Vec<_> = fs::read_dir(&home)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "sock").unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        socks.is_empty(),
        "hook must never autostart a daemon; found {socks:?}\n{stderr}"
    );
}
