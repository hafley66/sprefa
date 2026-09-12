//! Tracing writes to stderr only. Enabling it leaves stdout and the exit code
//! byte-identical; the five phase events are one JSON line each, in phase
//! order. `--trace` with `RUST_LOG` unset still prints waves and rounds.

use std::path::Path;
use std::process::Command;

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("v7/test/fixtures")
        .join(name)
        .display()
        .to_string()
}

fn run(json: bool, trace: bool) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dl8"));
    command
        .arg("compile")
        .arg(fixture("2_partial.dl7"))
        .env_remove("RUST_LOG")
        .env_remove("HAFLEY_LOG_FORMAT");
    if trace {
        command.arg("--trace");
    }
    if json {
        command
            .env("RUST_LOG", "dl8=info")
            .env("HAFLEY_LOG_FORMAT", "json");
    }
    command.output().unwrap()
}

#[test]
fn json_phase_events_leave_stdout_byte_identical() {
    let plain = run(false, false);
    let logged = run(true, false);
    assert_eq!(
        logged.stdout, plain.stdout,
        "stdout differs with tracing enabled"
    );
    assert_eq!(
        logged.status.code(),
        plain.status.code(),
        "exit code differs with tracing enabled"
    );
    let names: Vec<String> = String::from_utf8(logged.stderr)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value["target"] == "dl8::phase")
        .map(|value| value["fields"]["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        ["read", "macrotime", "lower", "check", "comptime"].map(String::from)
    );
}

#[test]
fn trace_prints_waves_and_rounds_without_rust_log() {
    let traced = run(false, true);
    let stderr = String::from_utf8(traced.stderr).unwrap();
    assert!(stderr.contains("Round"), "no Round line:\n{stderr}");
    assert!(stderr.contains("Wave"), "no Wave line:\n{stderr}");
}
