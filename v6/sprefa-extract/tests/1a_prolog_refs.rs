//! Prolog term-occurrence references: the `reference` record over a fixture with
//! KNOWN per-position counts. Assertions are exact.
//!
//! The fixture `1_refs.pl` is laid out so and relplan/5 appears exactly:
//!   2x head_arg  (foo/1 and bar/1 clause heads)
//!   2x term_arg  (process/1 argument data, and a list element)
//!   1x closure   (call/1's argument is a metacall closure slot: SWI
//!                 meta_predicate call(0) means the argument executes as a
//!                 goal, so its compound moved off term_arg; see 1b)
//!   0x goal      (never executed as a body conjunct)
//! and the nested compound case `wrap(outer(inner(w)))` emits BOTH outer/1 and
//! inner/1 references (term_arg), proving every-functor-at-every-depth.
//! Directive bodies are walked (module/2, dynamic/1, initialization/1 as goal;
//! their compound arguments as term_arg), and operator goals (is/2, =/2, !/0)
//! are DELIBERATELY emitted beyond compounds-only; both are pinned below.

use std::process::{Command, Output};

const FIXTURE: &str = "tests/fixtures/prolog/1_refs.pl";

fn raw(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs")
}

fn run(args: &[&str]) -> String {
    let output = raw(args);
    assert!(
        output.status.success(),
        "{args:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn refs_for(functor: &str) -> Vec<(String, String)> {
    let stdout = run(&["--family", "call", FIXTURE]);
    stdout
        .lines()
        .filter_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            if value.get("record")?.as_str()? != "reference" {
                return None;
            }
            if value.get("functor")?.as_str()? != functor {
                return None;
            }
            Some((
                value.get("position")?.as_str()?.to_string(),
                value.get("span")?.to_string(),
            ))
        })
        .collect()
}

fn count(rows: &[(String, String)], position: &str) -> usize {
    rows.iter().filter(|(p, _)| p == position).count()
}

#[test]
fn reference_counts_per_position_are_exact() {
    let relplan = refs_for("relplan/5");
    assert_eq!(count(&relplan, "head_arg"), 2);
    assert_eq!(count(&relplan, "term_arg"), 2);
    assert_eq!(count(&relplan, "closure"), 1);
    assert_eq!(count(&relplan, "goal"), 0);
}

#[test]
fn nested_compound_emits_both_inner_and_outer() {
    let outer = refs_for("outer/1");
    let inner = refs_for("inner/1");
    assert_eq!(outer.len(), 1);
    assert_eq!(inner.len(), 1);
    assert_eq!(outer[0].0, "term_arg");
    assert_eq!(inner[0].0, "term_arg");
    assert_ne!(outer[0].1, inner[0].1, "distinct compound spans");
}

#[test]
fn zero_arity_goals_emit_reference_records() {
    let side = refs_for("side_effect/0");
    assert_eq!(
        count(&side, "goal"),
        1,
        "a bare name used as a goal is a goal"
    );
    let callee = refs_for("call/1");
    assert_eq!(count(&callee, "goal"), 1, "call/1 is executed as a goal");
    assert_eq!(count(&callee, "term_arg"), 0);
    assert_eq!(refs_for("process/1").len(), 1);
    assert_eq!(refs_for("list/1").len(), 1);
    assert_eq!(refs_for("wrap/1").len(), 1);
}

#[test]
fn directive_bodies_are_walked_for_references() {
    let module = refs_for("module/2");
    assert_eq!(count(&module, "goal"), 1, "module/2 directive is executed");
    let dynamic = refs_for("dynamic/1");
    assert_eq!(count(&dynamic, "goal"), 1);
    let init = refs_for("initialization/1");
    assert_eq!(count(&init, "goal"), 1);
    let seed = refs_for("seed/1");
    assert_eq!(
        count(&seed, "term_arg"),
        1,
        "compound arg inside a directive"
    );
    let config = refs_for("config/1");
    assert_eq!(
        count(&config, "term_arg"),
        1,
        "nested compound in a directive"
    );
}

#[test]
fn operator_goals_are_deliberately_emitted() {
    let is_op = refs_for("is/2");
    assert_eq!(count(&is_op, "goal"), 1, "is/2 executes as a goal");
    let unify = refs_for("=/2");
    assert_eq!(count(&unify, "goal"), 1, "=/2 executes as a goal");
    let cut = refs_for("!/0");
    assert_eq!(count(&cut, "goal"), 1, "cut executes as a goal");
    assert_eq!(refs_for("+/2").len(), 0, "arithmetic data is not a goal");
}

#[test]
fn runs_are_byte_identical() {
    let first = run(&["--family", "call", FIXTURE]);
    let second = run(&["--family", "call", FIXTURE]);
    assert_eq!(first, second, "two runs must be byte-identical");
}
