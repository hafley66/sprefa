//! R7 diag stage routing (plans/2026-07-17-diag-stage-routing.md): the
//! `diag_stage(code, stage)` sink + the `--stage` filter on `--check`, and the
//! hook's agent-turn path intersection. Errors route to every stage; an
//! unrouted warning only to `commit`; a colocated `diag_stage` row overrides
//! the default; the hook's agent-turn surface is gated to the files the latest
//! turn touched.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::Write;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stage_routing_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

/// Run `dl --check --no-daemon [--stage S]` in `dir`; return (exit, stdout, stderr).
/// Diags render to stderr in the `path:line: severity[code]: msg` style.
fn check(dir: &Path, prog: &str, stage: Option<&str>) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let mut cmd = Command::new(DL);
    cmd.arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--check", "--no-daemon"])
        .env("SPREFA_CONFIG", dir.join("no.toml"))
        .current_dir(dir);
    if let Some(s) = stage {
        cmd.args(["--stage", s]);
    }
    let out = cmd.output().expect("run dl --check");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// An error routes to EVERY stage; an unrouted warning to `commit` only.
#[test]
fn error_everywhere_warning_commit_only() {
    let d = sandbox("err_warn");
    fs::write(d.join("src/err.rs"), "// e\n").unwrap();
    fs::write(d.join("src/warn.rs"), "// w\n").unwrap();
    let prog = r#"
rel src_file(path: file).
src_file(path) <- scan("WORK", "src/**/*.rs", path, rev).
diag(path: path, line: 1, severity: "error", code: "boom", msg: "an error") <-
    src_file(path), path =~ /err\.rs/.
diag(path: path, line: 2, severity: "warning", code: "meh", msg: "a warning") <-
    src_file(path), path =~ /warn\.rs/.
"#;
    // commit (default): both surface; exit 2 (an error is present).
    let (code, _out, err) = check(&d, prog, None);
    assert_eq!(code, 2, "error present -> exit 2:\n{err}");
    assert!(err.contains("[boom]"), "commit surfaces the error:\n{err}");
    assert!(err.contains("[meh]"), "commit surfaces the warning:\n{err}");

    // live: only the error (the warning is commit-only); still exit 2.
    let (code, _out, err) = check(&d, prog, Some("live"));
    assert_eq!(code, 2, "error still present at live:\n{err}");
    assert!(err.contains("[boom]"), "live surfaces the error:\n{err}");
    assert!(!err.contains("[meh]"), "live must NOT surface the unrouted warning:\n{err}");

    // agent-turn: same — warning dropped, error kept.
    let (_code, _out, err) = check(&d, prog, Some("agent-turn"));
    assert!(err.contains("[boom]"), "agent-turn surfaces the error:\n{err}");
    assert!(!err.contains("[meh]"), "agent-turn drops the unrouted warning:\n{err}");
}

/// A colocated `diag_stage` row overrides the severity default: a warning routed
/// only to agent-turn is gone from `commit` and present at `agent-turn`.
#[test]
fn colocated_diag_stage_row_overrides_default() {
    let d = sandbox("override");
    fs::write(d.join("src/a.rs"), "// a\n").unwrap();
    let prog = r#"
rel src_file(path: file).
src_file(path) <- scan("WORK", "src/**/*.rs", path, rev).
diag(path: path, line: 3, severity: "warning", code: "turnwarn", msg: "turn only") <-
    src_file(path).
diag_stage("turnwarn", "agent-turn") <- src_file(_).
"#;
    // commit: the warning is routed away from commit -> not shown; exit 0.
    let (code, _out, err) = check(&d, prog, None);
    assert_eq!(code, 0, "no error -> exit 0:\n{err}");
    assert!(!err.contains("[turnwarn]"), "commit must not show an agent-turn-only warning:\n{err}");

    // agent-turn: now it surfaces.
    let (_code, _out, err) = check(&d, prog, Some("agent-turn"));
    assert!(err.contains("[turnwarn]"), "agent-turn surfaces its routed warning:\n{err}");
}

/// The `--stage` flag rejects an unknown name loudly (never silently surfaces
/// nothing).
#[test]
fn stage_flag_rejects_unknown_name() {
    let d = sandbox("badstage");
    fs::write(d.join("src/a.rs"), "// a\n").unwrap();
    let (code, _out, err) = check(&d, "rel r(x: int).\nr(1).\n", Some("bogus"));
    assert_ne!(code, 0, "unknown stage must fail:\n{err}");
    assert!(err.contains("unknown --stage"), "names the bad flag:\n{err}");
}

/// The hook's agent-turn surface is gated to the files the latest agent turn
/// touched: a warning routed to agent-turn on a touched file is injected; the
/// same-code warning on an untouched file is not.
#[test]
fn hook_agent_turn_intersects_touched_files() {
    let d = sandbox("hook_turn");
    let root = fs::canonicalize(&d).unwrap();
    fs::write(d.join("src/touched.rs"), "// t\n").unwrap();
    fs::write(d.join("src/other.rs"), "// o\n").unwrap();

    // Fake Claude Code transcript store: the latest turn edited src/touched.rs.
    let projects = d.join("cc_projects");
    let slug: String = root.to_string_lossy().chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let slug_dir = projects.join(&slug);
    fs::create_dir_all(&slug_dir).unwrap();
    let edited = root.join("src/touched.rs");
    let transcript = format!(
        r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"tool_use","name":"Edit","input":{{"file_path":"{}"}}}}]}}}}"#,
        edited.to_string_lossy());
    fs::write(slug_dir.join("sess1.jsonl"), format!("{transcript}\n")).unwrap();

    // Program: a warning on every src file, routed to agent-turn; a `touched`
    // rule references agent_touch so the (lazy) agent family is kept warm.
    let prog = r#"
rel src_file(path: file).
src_file(path) <- scan("WORK", "src/**/*.rs", path, rev).
diag(path: path, line: 1, severity: "warning", code: "storm-x", msg: "storm finding") <-
    src_file(path).
diag_stage("storm-x", "agent-turn") <- src_file(_).
rel touched(path: text).
touched(path) <- agent_touch(_, _, path).
"#;
    fs::write(d.join("p.dl"), prog).unwrap();

    let event = r#"{"hook_event_name":"PostToolUse","session_id":"sess1"}"#;
    let mut child = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap(), "--hook", "--no-daemon"])
        // Test-harness escape: blank db each run; the deadlined hook path
        // deliberately cold-skips those.
        .env("DL_HOOK_DEADLINE_MS", "0")
        .env("SPREFA_CONFIG", d.join("no.toml"))
        .env("SPREFA_CLAUDE_PROJECTS", &projects)
        .env("SPREFA_OPENCODE_DB", d.join("no-opencode.db"))
        .current_dir(&d)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn dl --hook");
    child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
    let out = child.wait_with_output().expect("wait dl --hook");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    // additionalContext lists the touched file's diag, not the untouched one.
    assert!(stdout.contains("src/touched.rs"),
        "agent-turn context must include the touched file:\nstdout={stdout}\nstderr={stderr}");
    assert!(!stdout.contains("src/other.rs"),
        "agent-turn context must exclude the untouched file:\nstdout={stdout}");
}
