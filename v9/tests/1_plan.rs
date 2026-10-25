//! `check` over written programs: passing programs show their strata, each failing program shows
//! its diags.

use v9::_0_id::Dict;
use v9::_2_ir::Builtin::{Add, Lt, Neq};
use v9::_2_ir::Col::{Int, Text};
use v9::_2_ir::*;
use v9::_3_plan::{check, Plan};

/// The declarations every case shares.
fn rels() -> Vec<Rel> {
    vec![
        rel("edge", [("from", Text), ("to", Text)]),
        rel("blocked", [("node", Text)]),
        rel("count", [("node", Text), ("n", Int)]),
        derived("reach", [("from", Text), ("to", Text)]),
        derived("open", [("from", Text), ("to", Text)]),
        derived("next", [("node", Text), ("n", Int)]),
        request("ryi", "ask", [("path", Text)]),
        answer("ryi", "told", [("path", Text), ("callee", Text)]),
    ]
}

fn program(rules: Vec<Rule>) -> Program {
    Program {
        rels: rels(),
        rules,
    }
}

fn reach_rules() -> Vec<Rule> {
    vec![
        rule(
            atom("reach", [v("a"), v("c")]),
            [pos("edge", [v("a"), v("c")])],
        ),
        rule(
            atom("reach", [v("a"), v("c")]),
            [
                pos("reach", [v("a"), v("b")]),
                pos("edge", [v("b"), v("c")]),
            ],
        ),
    ]
}

/// Each stratum as rel names, recursive flag, rule indexes.
fn strata(plan: &Plan) -> Vec<(Vec<String>, bool, Vec<usize>)> {
    plan.strata
        .iter()
        .map(|stratum| {
            let names = stratum
                .rels
                .iter()
                .map(|id| plan.rels[*id as usize].name.clone())
                .collect();
            (names, stratum.recursive, stratum.rules.clone())
        })
        .collect()
}

#[test]
fn passing_programs_stratify() {
    let mut negation = reach_rules();
    negation.push(rule(
        atom("open", [v("a"), v("c")]),
        [
            pos("reach", [v("a"), v("c")]),
            neg("blocked", [v("c")]),
            call(Neq, [v("a"), v("c")]),
        ],
    ));
    let add = vec![
        rule(
            atom("next", [v("node"), v("m")]),
            [
                pos("count", [v("node"), v("n")]),
                call(Add, [v("n"), int(1), v("m")]),
                call(Lt, [v("m"), int(10)]),
            ],
        ),
        rule(atom("ask", [v("node")]), [pos("next", [v("node"), int(3)])]),
        rule(
            atom("reach", [v("node"), v("callee")]),
            [pos("told", [v("node"), v("callee")])],
        ),
    ];
    let cases = [
        ("reach", reach_rules()),
        ("negation", negation),
        ("add", add),
    ];
    let mut dict = Dict::default();
    let out: Vec<_> = cases
        .into_iter()
        .map(|(name, rules)| (name, strata(&check(program(rules), &mut dict).expect(name))))
        .collect();
    insta::assert_debug_snapshot!(out);
}

#[test]
fn lowered_rules_bind_slots_in_written_order() {
    let rules = vec![rule(
        atom("next", [v("node"), v("m")]),
        [
            pos("count", [v("node"), v("n")]),
            neg("blocked", [v("node")]),
            call(Add, [v("n"), int(-1), v("m")]),
            call(Neq, [v("node"), text("root")]),
        ],
    )];
    let mut dict = Dict::default();
    let plan = check(program(rules), &mut dict).expect("checks");
    insta::assert_debug_snapshot!((&plan.rules, &plan.rels[7], plan.hash, dict.take_new()));
}

#[test]
fn every_check_names_its_failure() {
    let mut negation_cycle = reach_rules();
    negation_cycle.push(rule(
        atom("open", [v("a"), v("c")]),
        [
            pos("edge", [v("a"), v("c")]),
            neg("reach", [v("a"), v("c")]),
        ],
    ));
    negation_cycle.push(rule(
        atom("reach", [v("a"), v("c")]),
        [pos("open", [v("a"), v("c")])],
    ));

    let mut bad_rels = rels();
    bad_rels.push(rel("edge", [("x", Text), ("x", Text)]));
    bad_rels.push(answer("ryi", "told_too", [("path", Text)]));
    bad_rels.push(answer("gh", "issues", [("repo", Text)]));
    bad_rels.push(request("oai", "prompt", [("text", Text)]));
    bad_rels.push(answer("oai", "reply", [("prompt", Int), ("text", Text)]));

    let cases: Vec<(&str, Program)> = vec![
        (
            "unknown rel",
            program(vec![rule(
                atom("reach", [v("a"), v("b")]),
                [pos("edges", [v("a"), v("b")])],
            )]),
        ),
        (
            "rel arity",
            program(vec![rule(
                atom("reach", [v("a"), v("b")]),
                [pos("edge", [v("a"), v("b"), v("c")])],
            )]),
        ),
        (
            "builtin arity",
            program(vec![rule(
                atom("next", [v("a"), v("n")]),
                [pos("count", [v("a"), v("n")]), call(Add, [v("n"), int(1)])],
            )]),
        ),
        (
            "var type disagrees",
            program(vec![rule(
                atom("next", [v("a"), v("b")]),
                [pos("edge", [v("a"), v("b")])],
            )]),
        ),
        (
            "constant type",
            program(vec![rule(
                atom("reach", [v("a"), int(1)]),
                [pos("edge", [v("a"), text("b")]), call(Lt, [v("a"), int(3)])],
            )]),
        ),
        (
            "neq types",
            program(vec![rule(
                atom("next", [v("a"), v("n")]),
                [pos("count", [v("a"), v("n")]), call(Neq, [v("a"), v("n")])],
            )]),
        ),
        (
            "unbound head var",
            program(vec![rule(
                atom("reach", [v("a"), v("z")]),
                [pos("edge", [v("a"), v("b")])],
            )]),
        ),
        (
            "unbound neg var",
            program(vec![rule(
                atom("open", [v("a"), v("b")]),
                [neg("blocked", [v("a")]), pos("edge", [v("a"), v("b")])],
            )]),
        ),
        (
            "unbound call input",
            program(vec![rule(
                atom("next", [v("a"), v("m")]),
                [
                    call(Add, [v("n"), int(1), v("m")]),
                    pos("count", [v("a"), v("n")]),
                ],
            )]),
        ),
        (
            "rule into Base",
            program(vec![rule(
                atom("edge", [v("a"), v("b")]),
                [pos("reach", [v("a"), v("b")])],
            )]),
        ),
        (
            "rule into Answer",
            program(vec![rule(
                atom("told", [v("a"), v("b")]),
                [pos("edge", [v("a"), v("b")])],
            )]),
        ),
        ("negation in an SCC", program(negation_cycle)),
        (
            "rel declarations",
            Program {
                rels: bad_rels,
                rules: vec![],
            },
        ),
    ];
    let mut dict = Dict::default();
    let out: Vec<_> = cases
        .into_iter()
        .map(|(name, program)| (name, check(program, &mut dict).expect_err(name)))
        .collect();
    assert!(dict.take_new().is_empty(), "a failed check interns nothing");
    insta::assert_debug_snapshot!(out);
}
