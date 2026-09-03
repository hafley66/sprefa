//! `--resolve --family flow`: the CLI door onto the FlowF join.
//!
//! ROUTE. `parse_arms` is private to `src/bin/extract.rs`, so every assertion
//! here drives the built binary rather than calling the parser. The binary IS
//! the contract under test: the join and its unit coverage already landed
//! (`13_flow_join.rs`) with zero production callers.
//!
//! FAIL-PRE-FIX, captured on 4531b4297 before the arm existed:
//!   $ extract --resolve --family flow 0_caller.ts 1_callee.ts
//!   Error: "--family 'flow' is not a resolve arm; under --resolve only 'call'
//!   and 'type' are meaningful"
//!   rc=1
//!
//! The `call`-only default is pinned byte-for-byte against the same golden
//! `1_resolve_cli.rs` uses, so gaining the arm cannot move existing output.

use std::process::Command;

const CALLER: &str = "tests/fixtures/resolve/0_caller.ts";
const CALLEE: &str = "tests/fixtures/resolve/1_callee.ts";
const CALL_GOLDEN: &str = include_str!("fixtures/resolve/2_resolved_edges.jsonl");

fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

#[test]
fn flow_is_a_resolve_arm() {
    let output = run(&["--resolve", "--family", "flow", CALLER, CALLEE]);

    assert!(
        output.lines().count() >= 1,
        "flow arm emitted nothing: {output}"
    );
    for line in output.lines() {
        assert!(
            line.contains("\"record\":\"flow_edge\"") && line.contains("\"family\":\"flow\""),
            "flow arm alone emitted a non-flow row: {line}"
        );
    }
}

#[test]
fn call_and_flow_arms_emit_both_families() {
    let output = run(&["--resolve", "--family", "call,flow", CALLER, CALLEE]);

    let flow_rows = output
        .lines()
        .filter(|line| line.contains("\"record\":\"flow_edge\""))
        .count();
    let call_rows = output
        .lines()
        .filter(|line| line.contains("\"record\":\"resolved_edge\""))
        .count();
    assert!(flow_rows >= 1, "no flow_edge row: {output}");
    assert!(call_rows >= 1, "no resolved_edge row: {output}");
    assert!(
        output.contains("\"kind\":\"ret_to_call_res\""),
        "the callee's return never reached the call site: {output}"
    );
}

#[test]
fn unknown_arm_names_flow_in_its_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(["--resolve", "--family", "bogus", CALLER])
        .output()
        .expect("extract binary runs");

    assert!(!output.status.success(), "a bogus arm name has to stop");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("is not a resolve arm") && stderr.contains("flow"),
        "the error text has to name the arms, flow included: {stderr}"
    );
}

#[test]
fn resolve_without_family_is_byte_identical() {
    assert_eq!(run(&["--resolve", CALLER, CALLEE]), CALL_GOLDEN);
}

/// FAIL-PRE-FIX, same base sha: `bench` took no `cfg` flag, so
/// `--bench --family cfg` printed
/// `ts: extract 95.542µs serial 1.125µs (cst=10 type=0 call=0 df=0 facts=19)`
/// with the cfg pass never run and nothing in the summary naming it.
#[test]
fn bench_runs_the_cfg_pass_when_the_family_names_it() {
    // The per-file bench numbers are a `tracing` event, so the assert reads the
    // JSON door rather than a stderr line whose shape nothing pinned.
    let bench = |args: &[&str]| -> serde_json::Value {
        let output = Command::new(env!("CARGO_BIN_EXE_extract"))
            .args(args)
            // The bench event's target is the BIN crate, `extract`, never the
            // library's `sprefa_extract`.
            .env("RUST_LOG", "extract=info")
            .env("HAFLEY_LOG_FORMAT", "json")
            .env("DL_TRAIL", "0")
            .output()
            .expect("extract binary runs");
        assert!(output.status.success(), "{args:?} did not exit clean");
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        stderr
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|value| value["fields"]["message"] == "bench")
            .unwrap_or_else(|| panic!("no bench event for {args:?} in: {stderr}"))
    };

    let with_cfg = bench(&["--bench", "--family", "cfg", CALLER]);
    assert!(
        with_cfg["fields"]["cfg_us"].as_u64().unwrap() > 0,
        "the cfg pass was never timed: {with_cfg}"
    );
    assert!(
        with_cfg["fields"]["cfg"].as_u64().unwrap() > 0,
        "the cfg pass produced no nodes: {with_cfg}"
    );

    let without_cfg = bench(&["--bench", "--family", "cst", CALLER]);
    assert_eq!(
        without_cfg["fields"]["cfg_us"].as_u64().unwrap(),
        0,
        "a run that never named cfg timed it anyway: {without_cfg}"
    );
}
