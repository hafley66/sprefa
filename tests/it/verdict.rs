//! e2e: the observability "verdict line" layer (src/verdict.rs). A cold
//! one-shot `--no-daemon` run emits an `[extract] ... REBUILD` verdict on
//! stderr at Default level; a warm re-run against the same `--db` emits the
//! `[extract] ... skipped (digest match)` verdict once `DL_LOG=debug` opts
//! in. Separately, a foreground singleton daemon start emits the `[dl]
//! mode=daemon ...` run-header line — the daemon-mode call site this arc
//! wired (see `src/daemon.rs::run_daemon`); one-shot/check/hook run-header
//! wiring lives in `src/cli/mod.rs`, out of scope for this arc (see the
//! session report).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("verdict_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

const PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
? type_entity(repo, sym, name, kind, parent, file, line).
"#;

fn run(dir: &Path, extra_envs: &[(&str, &str)]) -> String {
    fs::write(dir.join("p.dl"), PROG).unwrap();
    let mut cmd = Command::new(DL);
    cmd.arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml");
    for (k, v) in extra_envs { cmd.env(k, v); }
    let out = cmd.output().expect("run dl");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn cold_run_emits_extract_rebuild_and_warm_rerun_emits_skip_under_debug() {
    let dir = sandbox("cold_warm");
    fs::write(dir.join("src/a.rs"), "pub struct Alpha { pub n: u32 }\n").unwrap();

    // Cold one-shot: no prior `extract:*` digest, so every family rebuilds —
    // Default level (no DL_LOG set) already shows the verdict line.
    let cold_stderr = run(&dir, &[]);
    assert!(
        cold_stderr.lines().any(|l| l.contains("[extract]") && l.contains("REBUILD")),
        "cold run should print an [extract] ... REBUILD verdict line, got:\n{cold_stderr}"
    );
    assert!(
        cold_stderr.lines().any(|l| l.contains("first-run")),
        "cold run's first tick has no prior digest, so the reason must be first-run:\n{cold_stderr}"
    );

    // Warm re-run against the SAME --db: the extract:* digest matches, so
    // every family skips. The skip verdict is Debug-only.
    let warm_stderr_default = run(&dir, &[]);
    assert!(
        !warm_stderr_default.lines().any(|l| l.contains("[extract]") && l.contains("skipped")),
        "a skip verdict must NOT print at Default level:\n{warm_stderr_default}"
    );
    let warm_stderr_debug = run(&dir, &[("DL_LOG", "debug")]);
    assert!(
        warm_stderr_debug.lines().any(|l| l.contains("[extract]") && l.contains("skipped (digest match)")),
        "warm run under DL_LOG=debug should print an [extract] ... skipped (digest match) verdict, got:\n{warm_stderr_debug}"
    );
    assert!(
        !warm_stderr_debug.lines().any(|l| l.contains("[extract]") && l.contains("REBUILD")),
        "a warm no-change run must not also claim a rebuild:\n{warm_stderr_debug}"
    );
}

/// `DL_LOG=quiet` suppresses the verdict lines entirely (both the cold
/// REBUILD verdict at Default level and — moot here — any Debug-only skip
/// verdict), while the tick's own `[tick]`/existing stderr output is
/// untouched (this module never gates anything but its own lines).
#[test]
fn dl_log_quiet_suppresses_verdict_lines() {
    let dir = sandbox("quiet");
    fs::write(dir.join("src/a.rs"), "pub struct Alpha { pub n: u32 }\n").unwrap();
    let stderr = run(&dir, &[("DL_LOG", "quiet")]);
    assert!(
        !stderr.lines().any(|l| l.contains("[extract]")),
        "DL_LOG=quiet must suppress every [extract] verdict line, got:\n{stderr}"
    );
}

/// The daemon-mode run-header verdict: `dl daemon start --foreground` prints
/// `[dl] mode=daemon ...` naming the root/db/version/build before serving.
#[test]
fn daemon_foreground_start_emits_run_header() {
    let base = std::env::temp_dir().join(format!("verdict_daemon_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let home = base.join("xdg");
    let root = base.join("repo");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(root.join(".dl")).unwrap();

    let mut cmd = Command::new(DL);
    cmd.args(["daemon", "start", "--foreground"])
        .current_dir(&root)
        .env("DL_DAEMON_ROOT", &root)
        .env("XDG_STATE_HOME", &home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = DaemonGuard(cmd.spawn().expect("spawn foreground daemon"));
    let mut stderr = child.0.stderr.take().expect("captured stderr");

    use std::io::Read;
    // Give the daemon time to finish its cold-tick startup sequence and
    // print the run-header line before it settles into the poll loop.
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let _ = child.0.kill();
    let mut buf = String::new();
    let _ = stderr.read_to_string(&mut buf);

    assert!(
        buf.lines().any(|l| l.starts_with("[dl] mode=daemon")),
        "daemon start should print a [dl] mode=daemon run-header verdict, got:\n{buf}"
    );
}
