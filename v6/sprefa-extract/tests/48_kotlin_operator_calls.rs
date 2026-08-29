//! TEST the kotlin call arm against the corpus battery finding: infix,
//! operator, and invoke call sites. FAIL-PRE-FIX: `kt_walk_call_sites` minted
//! sites from `call_expression` alone, so `1 plus2 2`, `Box(1) + Box(2)`, and
//! `Box(3)()` produced no `site` record. Spans are the operator token / infix
//! name, so `--resolve` joins them to the `operator fun` / `infix fun` def.

use std::process::Command;

const FIXTURE: &str = "tests/fixtures/kotlin/corpus_1_infix_operator.kt";
const DEFS: &str = "tests/fixtures/kotlin/corpus_2_ops_defs.kt";
const USE: &str = "tests/fixtures/kotlin/corpus_3_ops_use.kt";

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

/// (span start, span end, callee) per `site` record, in emission order.
fn sites(args: &[&str]) -> Vec<(u32, u32, String)> {
    run(args)
        .lines()
        .filter(|line| line.contains("\"record\":\"site\""))
        .map(|line| {
            let start = line
                .split("\"start\":")
                .nth(1)
                .and_then(|s| s.split(',').next())
                .and_then(|s| s.parse::<u32>().ok())
                .expect("site has span start");
            let end = line
                .split("\"end\":")
                .nth(1)
                .and_then(|s| s.split('}').next())
                .and_then(|s| s.parse::<u32>().ok())
                .expect("site has span end");
            let callee = line
                .split("\"callee\":\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .expect("site has callee")
                .to_string();
            (start, end, callee)
        })
        .collect()
}

#[test]
fn infix_operator_and_invoke_sites_are_minted() {
    assert_eq!(
        sites(&["--family", "call", FIXTURE]),
        [
            (216, 217, "plus".to_string()), // `this + other` in the infix fun body
            (289, 292, "Box".to_string()),  // Box(value + other.value)
            (299, 300, "plus".to_string()), // `value + other.value`
            (386, 391, "plus2".to_string()), // `1 plus2 2`
            (424, 425, "plus".to_string()), // `Box(1) + Box(2)`
            (417, 420, "Box".to_string()),  // Box(1)
            (426, 429, "Box".to_string()),  // Box(2)
            (460, 462, "invoke".to_string()), // Box(3)()
            (454, 457, "Box".to_string()),  // Box(3)
        ]
    );
}

#[test]
fn resolve_joins_operator_sites_to_operator_defs() {
    let out = run(&["--resolve", DEFS, USE]);
    let callees: Vec<&str> = out
        .lines()
        .filter(|line| line.contains("\"record\":\"resolved_edge\""))
        .filter_map(|line| {
            line.split("\"callee_name\":\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
        })
        .collect();
    for callee in ["plus2", "plus", "invoke"] {
        assert!(
            callees.contains(&callee),
            "resolved_edge to `{callee}` missing: {callees:?}"
        );
    }
}
