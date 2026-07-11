//! The perf-woes dogfood rail (`.dl/perf-woes.dl`) must FIRE onsite on a
//! genuinely slow rule and a genuinely over-budget tick, and stay QUIET on a
//! healthy program. This is the regression test for the class of defect that
//! hid a 15-second-per-tick floor for weeks: nothing surfaced per-rule cost
//! anywhere a human or an agent would look. Here we plant a deliberately
//! expensive rule (an unfiltered triple self-join) under a temp root and run
//! the SHIPPED rail against it, the same idiom as
//! tests/it/magic_rel_audit.rs.
//!
//! Budgets are facts that UNION rather than override (verified empirically:
//! two `rel budget(max: int).` rows for the same fact rel both persist), so
//! the repro test adds a second, stricter budget row alongside the shipped
//! default rather than editing the shipped file — both rows gate
//! independently and the stricter one is what actually trips.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// The repo's actual rail file — the artifact under test, not a copy.
fn rail() -> String {
    format!("{}/.dl/perf-woes.dl", env!("CARGO_MANIFEST_DIR"))
}

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_perfwoes_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run the shipped rail plus a companion program over a sandbox root.
/// Returns (exit code, stdout, stderr).
fn run(dir: &Path, companion_path: &Path) -> (i32, String, String) {
    let out = Command::new(DL)
        .args([&rail(), companion_path.to_str().unwrap(), "--no-daemon", "--check"])
        .current_dir(dir)
        .args(["--db", dir.join("db").to_str().unwrap()])
        .output()
        .expect("run dl over the sandbox");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Build an unfiltered triple self-join over `n` seeded rows (n^3 output
/// rows), with a tiny per-rule budget (1ms) and a tiny tick budget (1ms)
/// unioned alongside the shipped defaults — deterministic and comfortably
/// nonzero regardless of machine speed, but small enough to stay well under
/// 2s wall (n=60 -> 216,000 rows, ~100ms+ rebuild on a dev laptop). Returns
/// (program text, 1-based line number of the `triple(...) <-` rule head).
fn expensive_program(n: usize) -> (String, usize) {
    let mut lines: Vec<String> = Vec::new();
    lines.push("rel seed_node(x: text).".to_string());
    for i in 0..n {
        lines.push(format!("seed_node(\"n{i}\")."));
    }
    lines.push("rel triple(a: text, b: text, c: text).".to_string());
    let triple_line = lines.len() + 1; // 1-based line of the next pushed line
    lines.push(
        "triple(a, b, c) <- seed_node(a), seed_node(b), seed_node(c).".to_string(),
    );
    // Tiny budgets, unioned alongside the shipped defaults (1000/10000/500000)
    // — this row is the one that actually trips.
    lines.push("rel perf_budget_ms(max: int).".to_string());
    lines.push("perf_budget_ms(1).".to_string());
    lines.push("rel tick_budget_ms(max: int).".to_string());
    lines.push("tick_budget_ms(1).".to_string());
    (lines.join("\n") + "\n", triple_line)
}

/// A cheap, healthy program: a handful of facts and one trivial derived rel.
/// Well under every shipped default budget (1000ms/10000ms/500000 rows).
const CHEAP_PROGRAM: &str = r#"
rel small_seed(x: text).
small_seed("a").
small_seed("b").
small_seed("c").

rel small_derived(x: text).
small_derived(x) <- small_seed(x).
"#;

/// REPRO: an expensive rule must trip both the onsite per-rule WARN (naming
/// the rel and pointing at its rule-head file:line) and the whole-tick
/// ERROR, and `--check` must exit 2.
#[test]
fn repro_slow_rule_and_tick_overbudget_fire_onsite() {
    let dir = sandbox("repro");
    let (program, triple_line) = expensive_program(60);
    let expensive_path = dir.join("expensive.dl");
    fs::write(&expensive_path, &program).unwrap();

    // First run: populate telemetry (stmt_ms/rel_count lag one tick).
    let (_code1, _out1, err1) = run(&dir, &expensive_path);
    assert!(!err1.contains("panic"), "first run didn't panic: {err1}");

    // Second run: telemetry has landed, the rail can see it. Diagnostics
    // print to stderr (--check's finding stream), not stdout.
    // tick-over-budget was deliberately downgraded to WARNING (33f549b: as an
    // error it exit-2'd the PostToolUse hook and blocked every write while the
    // perf debt it flags was being paid down). Warnings do not exit 2 — flip
    // this back to `assert_eq!(code2, 2, ...)` when the rail re-promotes.
    let (code2, out2, err2) = run(&dir, &expensive_path);
    assert_eq!(code2, 0, "an over-budget tick warns (not exit 2) while the rail is downgraded: out={out2} err={err2}");
    let findings = format!("{out2}{err2}");

    assert!(findings.contains("slow-rule-onsite"),
        "onsite per-rule WARN fires:\n{findings}");
    assert!(findings.contains("triple"),
        "the finding names the offending rel `triple`:\n{findings}");
    let expected_loc = format!("expensive.dl:{triple_line}:");
    assert!(findings.contains(&expected_loc),
        "the finding is positioned at the rule's own head line ({expected_loc}):\n{findings}");
    assert!(!findings.contains("slow-rule]"),
        "no un-located fallback finding when a rule head IS found:\n{findings}");

    assert!(findings.contains("tick-over-budget"),
        "the whole-tick hard tripwire fires:\n{findings}");
    assert!(findings.contains("warning[tick-over-budget]"),
        "tick-over-budget is WARNING severity while downgraded (see 33f549b):\n{findings}");
}

/// QUIET: a cheap program with the shipped (generous) default budgets must
/// produce zero perf-woes findings and exit 0.
#[test]
fn quiet_on_a_healthy_program() {
    let dir = sandbox("quiet");
    let cheap_path = dir.join("cheap.dl");
    fs::write(&cheap_path, CHEAP_PROGRAM).unwrap();

    let (code1, _out1, err1) = run(&dir, &cheap_path);
    assert_eq!(code1, 0, "first run clean: {err1}");
    let (code2, out2, err2) = run(&dir, &cheap_path);
    assert_eq!(code2, 0, "second run clean (exit 0, no ERROR finding): {err2}");
    let findings = format!("{out2}{err2}");

    for code in ["slow-rule", "slow-rule-onsite", "tick-over-budget", "perf-woes-rel-blowup"] {
        assert!(!findings.contains(code),
            "no `{code}` finding on a healthy program:\n{findings}");
    }
}

/// RAIL-HONESTY: the rail program itself parses clean, and its own
/// housekeeping rules (dl_rule_head / slow_rule_located / perf_tick_total_ms)
/// stay under its own shipped budgets on a small fixture — the audit
/// machinery must not become its own worst offender.
#[test]
fn rail_parses_clean_and_stays_under_its_own_budgets() {
    let parse_out = Command::new(DL)
        .args([&rail(), "--parse-only"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("parse-only the shipped rail");
    assert_eq!(parse_out.status.code().unwrap_or(-1), 0,
        "the shipped rail parses clean: {}", String::from_utf8_lossy(&parse_out.stderr));

    let dir = sandbox("honesty");
    let cheap_path = dir.join("cheap.dl");
    fs::write(&cheap_path, CHEAP_PROGRAM).unwrap();
    let (_code1, _out1, _err1) = run(&dir, &cheap_path);
    let (code2, out2, err2) = run(&dir, &cheap_path);
    assert_eq!(code2, 0, "the rail's own rules stay under its own budgets: {err2}");
    let findings = format!("{out2}{err2}");
    for own_rel in ["dl_rule_head", "slow_rule_located", "perf_tick_total_ms"] {
        assert!(!findings.contains(own_rel),
            "the rail's own housekeeping rel `{own_rel}` never trips its own budget:\n{findings}");
    }
}
