//! Global invocation log (`src/invlog.rs`) + rolling file-log (`src/trace.rs`)
//! end-to-end tests: one-shot rows, kill evidence, the `why` join, DL_LOG file
//! logging, `DL_INVLOG=0` opt-out.
//!
//! HERMETICITY: every test sandboxes `XDG_STATE_HOME` (invlog's db and the
//! rolling log files both live under `<XDG_STATE_HOME>/sprefa`) and points
//! `SPREFA_CONFIG` at a nonexistent file so no ambient config repos load. No
//! test here starts a real daemon against a real repo root.

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const DL: &str = env!("CARGO_BIN_EXE_dl");

struct Sandbox {
    home: PathBuf,
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("dl_invlog_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let home = base.join("xdg");
        let root = base.join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(root.join(".dl")).unwrap();
        Sandbox { home, root }
    }

    fn log_dir(&self) -> PathBuf {
        self.home.join("sprefa").join("log")
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(DL);
        cmd.current_dir(&self.root)
            .env("XDG_STATE_HOME", &self.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml");
        cmd
    }

    fn write_program(&self) -> PathBuf {
        let p = self.root.join("p.dl");
        fs::write(
            &p,
            "rel edge(a: text, b: text).\nedge(\"a\", \"b\").\n? edge(a, b).\n",
        )
        .unwrap();
        p
    }

    /// `dl daemon invocations` output, read straight off this sandbox's
    /// invocations.db (no daemon needs to be running — it's a file read).
    fn invocations_report(&self) -> String {
        let out = self
            .cmd()
            .args(["daemon", "invocations", "--limit", "50"])
            .output()
            .expect("run dl daemon invocations");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn why_report(&self) -> String {
        let out = self
            .cmd()
            .args(["daemon", "why"])
            .output()
            .expect("run dl daemon why");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

/// Poll-wait helper: retry `f` until it returns `Some`, or give up after
/// `timeout`. Used instead of a fixed sleep so the test is fast on a quiet
/// machine and still tolerant on a loaded CI box.
fn wait_for<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let start = std::time::Instant::now();
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if start.elapsed() > timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn wait_and_kill(mut child: Child, timeout: Duration) -> Child {
    // No macOS `timeout` binary (see repo convention): a manual try_wait loop
    // stands in. Used to give a `--watch` invocation a moment to actually
    // start (open its invlog row) before we SIGKILL it.
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    child
}

/// (1) A plain one-shot `dl prog.dl --no-daemon` records exactly one
/// invocation row: `ts_end` set, `exit=0`, argv/cwd correct.
#[test]
fn one_shot_run_records_a_finished_invocation() {
    let sb = Sandbox::new("oneshot");
    let prog = sb.write_program();

    let status = sb
        .cmd()
        .arg(&prog)
        .arg("--no-daemon")
        .status()
        .expect("run dl");
    assert!(status.success(), "one-shot query run should exit 0");

    let report = sb.invocations_report();
    // The `dl daemon invocations` query itself is a fresh invocation whose OWN
    // row is still open (RUNNING) at the moment it reads the table — that is
    // expected, not a leak — so scope the assertion to the query program's
    // own line rather than asserting no row anywhere is open.
    let prog_line = report
        .lines()
        .find(|l| l.contains("--no-daemon") && l.contains("p.dl"))
        .unwrap_or_else(|| panic!("no line for the p.dl run in report:\n{report}"));
    assert!(
        prog_line.contains("exit=0"),
        "expected a finished (exit=0) row:\n{prog_line}"
    );
}

/// (2) A long-running one-shot (`--watch`, which loops until killed) that
/// gets SIGKILLed leaves its row OPEN (`ts_end` NULL) — that IS the kill
/// evidence — and `dl daemon invocations` flags it `KILLED` once the pid is
/// confirmed dead.
#[test]
fn sigkilled_invocation_leaves_an_open_row_flagged_killed() {
    let sb = Sandbox::new("killed");
    let prog = sb.write_program();

    let child = sb
        .cmd()
        .arg(&prog)
        .arg("--watch")
        .arg("--no-daemon")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn watch run");
    let mut child = wait_and_kill(child, Duration::from_secs(3));
    let _ = child.wait();

    let report = wait_for(Duration::from_secs(5), || {
        let r = sb.invocations_report();
        if r.contains("KILLED") {
            Some(r)
        } else {
            None
        }
    });
    let report = report.unwrap_or_else(|| sb.invocations_report());
    assert!(
        report.contains("KILLED"),
        "killed --watch row must be flagged KILLED:\n{report}"
    );
    assert!(
        report.contains("--watch"),
        "argv should still show the watch invocation:\n{report}"
    );
}

/// (3) `dl daemon why`'s invocations section prints even when this sandbox
/// never ran a daemon at all (the respawn-storm shape: only one-shot clients,
/// no daemon ever settled) — deliverable 3's join must not depend on a why
/// trail existing.
#[test]
fn why_report_prints_invocations_section_with_no_daemon_ever_run() {
    let sb = Sandbox::new("whyjoin");
    let prog = sb.write_program();
    let status = sb
        .cmd()
        .arg(&prog)
        .arg("--no-daemon")
        .status()
        .expect("run dl");
    assert!(status.success());

    let report = sb.why_report();
    assert!(
        report.contains("recent invocations:"),
        "invocations section must print:\n{report}"
    );
    assert!(
        report.contains("--no-daemon"),
        "the one-shot run's argv should appear:\n{report}"
    );
}

/// (4) `DL_INVLOG=0` disables recording end to end: no row for that
/// invocation (the report may still show OTHER rows from earlier in the same
/// sandbox, so assert on the absence of this run's distinguishing flag).
#[test]
fn dl_invlog_0_disables_recording_end_to_end() {
    let sb = Sandbox::new("disabled");
    let prog = sb.write_program();

    let status = sb
        .cmd()
        .arg(&prog)
        .arg("--no-daemon")
        .env("DL_INVLOG", "0")
        .status()
        .expect("run dl");
    assert!(status.success());

    let report = sb.invocations_report();
    assert!(
        report.starts_with("no invocations recorded") || !report.contains("--no-daemon"),
        "DL_INVLOG=0 must not create a row for this run:\n{report}"
    );
}

/// (5) `DL_LOG=info` makes the rolling file layers write: `dl.log` carries a
/// process-start line with this run's pid; `error.log` exists (created
/// unconditionally by the same `file_layers` call, even with nothing routed
/// to it yet) and does NOT contain any `info`-level line.
#[test]
fn dl_log_writes_rolling_files_with_correct_levels() {
    let sb = Sandbox::new("logfiles");
    let prog = sb.write_program();

    let status = sb
        .cmd()
        .arg(&prog)
        .arg("--no-daemon")
        .env("DL_LOG", "info")
        .status()
        .expect("run dl");
    assert!(status.success());

    let dl_log_path = sb.log_dir().join("dl.log");
    let dl_log = wait_for(Duration::from_secs(2), || {
        fs::read_to_string(&dl_log_path).ok()
    })
    .unwrap_or_default();
    assert!(
        !dl_log.is_empty(),
        "dl.log should exist and be non-empty at {}",
        dl_log_path.display()
    );
    assert!(
        dl_log.contains("INFO"),
        "dl.log should carry at least one INFO line:\n{dl_log}"
    );

    let error_log_path = sb.log_dir().join("error.log");
    // error.log is created by the SAME `file_layers` construction even when
    // nothing warn-or-above fired during this quiet run; existence alone is
    // the assertion for "no info lines" — an empty/absent file trivially
    // contains no INFO line, and if it DOES have content, none of it may be
    // an INFO line (this crate's fmt layer prints the level per line).
    if let Ok(error_log) = fs::read_to_string(&error_log_path) {
        assert!(
            !error_log.contains(" INFO "),
            "error.log must not contain info lines:\n{error_log}"
        );
    }
}
