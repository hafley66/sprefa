//! `effect` rows answered inside the compiler fixpoint, through the real
//! binary. Each `fixtures/comptime_effect/*.dl7` runs `dl8 compile --trace`
//! from the crate root, so the document path in its seed is relative to it.
//!
//! The trace carries one `Round::Answer` line per answer step; its count is the
//! COUNT receipt. The last `Decision` line is the comptime fixpoint's stable
//! round (the first belongs to the macro program).

use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const LIMIT: Duration = Duration::from_secs(10);

struct Compiled {
    out: Value,
    trace: String,
    code: i32,
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn compile(stem: &str) -> Compiled {
    let source = format!("fixtures/comptime_effect/{stem}.dl7");
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(root())
        .args(["compile", "--trace", &source])
        .output()
        .expect("spawn dl8");
    let took = started.elapsed();
    assert!(took < LIMIT, "{stem}: compile took {took:?}");
    let out = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{stem}: no JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    Compiled {
        out,
        trace: String::from_utf8_lossy(&output.stderr).to_string(),
        code: output.status.code().unwrap_or(-1),
    }
}

fn kernel(name: &str) -> Value {
    json!({"f": "ref", "args": [{"f": "kernel", "args": [{"a": name}]}]})
}

/// Compiler rows of the relation `rel` names.
fn rows(compiled: &Compiled, rel: &Value) -> usize {
    compiled.out["compiler_rows"]
        .as_array()
        .expect("compiler_rows")
        .iter()
        .filter(|row| &row["args"][0] == rel)
        .count()
}

fn named(compiled: &Compiled, name: &str) -> usize {
    let rel = compiled.out["program"]["names"][name].clone();
    assert!(!rel.is_null(), "no name {name}");
    rows(compiled, &rel)
}

fn answers(compiled: &Compiled) -> usize {
    compiled.trace.matches("Answer {").count()
}

fn stable_round(compiled: &Compiled) -> Option<i64> {
    let line = compiled
        .trace
        .lines()
        .filter(|line| line.contains("outcome: Stable"))
        .last()?;
    let rest = line.split("round: ").nth(1)?;
    rest.split(',').next()?.trim().parse().ok()
}

fn assert_clean(stem: &str, compiled: &Compiled) {
    assert_eq!(compiled.code, 0, "{stem}: {}", compiled.out["diagnostics"]);
    assert_eq!(compiled.out["diagnostics"], json!([]), "{stem}");
}

#[test]
pub fn a_once_executor_answers_its_effect_row_at_comptime() {
    let compiled = compile("0_fs_json");
    assert_clean("0_fs_json", &compiled);
    assert_eq!(named(&compiled, "Member"), 2, "title and done");
    assert_eq!(named(&compiled, "fs.json"), 1);
    assert_eq!(rows(&compiled, &kernel("effect")), 1);
    assert_eq!(answers(&compiled), 1);
    assert_eq!(stable_round(&compiled), Some(3));
}

#[test]
pub fn a_relation_with_no_executor_is_never_asked() {
    let compiled = compile("1_no_executor");
    assert_clean("1_no_executor", &compiled);
    assert_eq!(named(&compiled, "Member"), 0);
    assert_eq!(rows(&compiled, &kernel("effect")), 0);
    assert_eq!(answers(&compiled), 0);
    assert_eq!(stable_round(&compiled), Some(1));
}

/// Comptime leaves `timer` unserved; `dl8 eval --serve timer` on the same
/// program still writes its effect row.
#[test]
pub fn a_continuing_executor_waits_for_the_runtime() {
    let compiled = compile("2_continuing");
    assert_clean("2_continuing", &compiled);
    assert_eq!(rows(&compiled, &kernel("effect")), 0);
    assert_eq!(answers(&compiled), 0);

    let path =
        std::env::temp_dir().join(format!("dl8-comptime-effect-{}.json", std::process::id()));
    std::fs::write(&path, serde_json::to_vec(&compiled.out).unwrap()).unwrap();
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .args(["eval", &path.to_string_lossy(), "--serve", "timer"])
        .output()
        .expect("spawn dl8");
    let _ = std::fs::remove_file(&path);
    assert!(started.elapsed() < LIMIT);
    let out: Value = serde_json::from_slice(&output.stdout).expect("eval JSON");
    let effects = out["closure"]
        .as_array()
        .expect("closure")
        .iter()
        .filter(|row| row["rel"] == kernel("effect"))
        .count();
    assert_eq!(effects, 1, "{}", out["closure"]);
}

#[test]
pub fn answered_rows_feed_a_rule_that_mints_an_edge() {
    let compiled = compile("3_two_rounds");
    assert_clean("3_two_rounds", &compiled);
    assert_eq!(named(&compiled, "Heading"), 1);
    assert_eq!(answers(&compiled), 1);
    assert_eq!(stable_round(&compiled), Some(3));
}
