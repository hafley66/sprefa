//! `DL_TRACE_CHROME` (docs/tracing-chrome.md): a one-shot `dl <prog> --check`
//! with the env var set produces a chrome-trace JSON file that (a) parses as
//! strict JSON once the process has exited cleanly and (b) carries at least
//! one `tick` span. Never starts a daemon (`DL_NO_DAEMON=1`, no
//! `daemon start`/`daemon serve`) per this arc's rules.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_trace_chrome_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const EDGE_PROGRAM: &str =
    "rel edge(a: text, b: text).\nedge(\"a\", \"b\").\nedge(\"b\", \"c\").\n? edge(a, b).\n";

/// A clean one-shot `--check` run with `DL_TRACE_CHROME` set produces a file
/// that `serde_json` parses as a JSON array containing at least one object
/// whose `name` field is `"tick"`.
#[test]
fn one_shot_check_emits_a_parseable_trace_with_a_tick_span() {
    let dir = sandbox("check");
    fs::write(dir.join("p.dl"), EDGE_PROGRAM).unwrap();
    let trace_path = dir.join("trace.json");
    let db_path = dir.join("db.sqlite");

    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", db_path.to_str().unwrap()])
        .arg("--check")
        .current_dir(&dir)
        .env("DL_TRACE_CHROME", &trace_path)
        .env("DL_NO_DAEMON", "1")
        .env("DL_NO_BUDGET", "1")
        .output()
        .expect("run dl --check");
    assert!(
        out.status.success(),
        "dl --check should exit 0 on a clean program:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(trace_path.exists(), "DL_TRACE_CHROME path should exist after a clean exit");
    let text = fs::read_to_string(&trace_path).expect("read trace file");
    let events: serde_json::Value =
        serde_json::from_str(&text).expect("trace file should be strict JSON after a clean exit (finish_chrome_trace ran)");
    let events = events.as_array().expect("chrome trace is a JSON array of events");
    assert!(!events.is_empty(), "trace should carry at least one event");

    let tick_spans: Vec<&serde_json::Value> = events
        .iter()
        .filter(|event| event.get("name").and_then(|n| n.as_str()) == Some("tick"))
        .collect();
    assert!(
        !tick_spans.is_empty(),
        "trace should carry at least one `tick` span; got names: {:?}",
        events.iter().filter_map(|e| e.get("name").and_then(|n| n.as_str())).collect::<Vec<_>>()
    );
}

/// `DL_TRACE_CHROME` unset: no trace file materializes, and the run is
/// otherwise unaffected (layer never constructed — see `trace::chrome_layer`).
#[test]
fn unset_env_var_writes_no_trace_file() {
    let dir = sandbox("unset");
    fs::write(dir.join("p.dl"), EDGE_PROGRAM).unwrap();
    let trace_path = dir.join("trace.json");
    let db_path = dir.join("db.sqlite");

    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", db_path.to_str().unwrap()])
        .arg("--check")
        .current_dir(&dir)
        .env_remove("DL_TRACE_CHROME")
        .env("DL_NO_DAEMON", "1")
        .env("DL_NO_BUDGET", "1")
        .output()
        .expect("run dl --check");
    assert!(out.status.success(), "dl --check should exit 0 on a clean program");
    assert!(!trace_path.exists(), "no trace file should appear when DL_TRACE_CHROME is unset");
}
