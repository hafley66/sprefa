//! `Dd` against the naive oracle: after every step the rows the deltas add up to equal the
//! oracle's evaluation of every input so far.

mod common {
    pub mod oracle;
}

use common::oracle::Oracle;
use std::collections::BTreeSet;
use std::time::Instant;
use v9::_0_id::Dict;
use v9::_2_ir::Builtin::{Add, Lt, Neq};
use v9::_2_ir::Col::{Int, Text};
use v9::_2_ir::*;
use v9::_3_plan::{check, Plan, Row, Value};
use v9::_5_ends::{Bounds, Change, Diag, Engine};
use v9::_6_back::_1_dd::Dd;

type Step = Vec<(&'static str, Vec<Term>, isize)>;

struct Case {
    name: &'static str,
    rels: Vec<Rel>,
    rules: Vec<Rule>,
    steps: Vec<Step>,
    /// Step indexes that must return `FixpointLimit`.
    limited: &'static [usize],
    max_rounds: u32,
}

fn ins(
    rel: &'static str,
    args: impl IntoIterator<Item = Term>,
) -> (&'static str, Vec<Term>, isize) {
    (rel, args.into_iter().collect(), 1)
}

fn del(
    rel: &'static str,
    args: impl IntoIterator<Item = Term>,
) -> (&'static str, Vec<Term>, isize) {
    (rel, args.into_iter().collect(), -1)
}

fn edges(pairs: &[(&str, &str)], diff: isize) -> Step {
    pairs
        .iter()
        .map(|(from, to)| ("edge", vec![text(from), text(to)], diff))
        .collect()
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

fn graph_rels() -> Vec<Rel> {
    vec![
        rel("edge", [("from", Text), ("to", Text)]),
        rel("blocked", [("node", Text)]),
        derived("reach", [("from", Text), ("to", Text)]),
        derived("open", [("from", Text), ("to", Text)]),
    ]
}

fn case(name: &'static str, rels: Vec<Rel>, rules: Vec<Rule>, steps: Vec<Step>) -> Case {
    Case {
        name,
        rels,
        rules,
        steps,
        limited: &[],
        max_rounds: 1000,
    }
}

fn cases() -> Vec<Case> {
    let mut negation = reach_rules();
    negation.push(rule(
        atom("open", [v("a"), v("c")]),
        [pos("reach", [v("a"), v("c")]), neg("blocked", [v("c")])],
    ));
    let mut two_strata_rels = graph_rels();
    two_strata_rels.push(rel("node", [("name", Text)]));
    two_strata_rels.push(derived("apart", [("from", Text), ("to", Text)]));
    two_strata_rels.push(derived("lonely", [("name", Text)]));
    let mut two_strata = reach_rules();
    two_strata.push(rule(
        atom("apart", [v("a"), v("b")]),
        [
            pos("node", [v("a")]),
            pos("node", [v("b")]),
            neg("reach", [v("a"), v("b")]),
            call(Neq, [v("a"), v("b")]),
        ],
    ));
    two_strata.push(rule(
        atom("lonely", [v("a")]),
        [pos("node", [v("a")]), neg("apart", [v("a"), text("hub")])],
    ));
    vec![
        case(
            "join",
            vec![
                rel("edge", [("from", Text), ("to", Text)]),
                rel("label", [("node", Text), ("tag", Text)]),
                derived("tagged", [("from", Text), ("to", Text), ("tag", Text)]),
            ],
            vec![rule(
                atom("tagged", [v("a"), v("b"), v("t")]),
                [
                    pos("edge", [v("a"), v("b")]),
                    pos("label", [v("b"), v("t")]),
                ],
            )],
            vec![
                vec![
                    ins("edge", [text("a"), text("b")]),
                    ins("edge", [text("a"), text("c")]),
                    ins("label", [text("b"), text("red")]),
                ],
                vec![
                    ins("label", [text("c"), text("blue")]),
                    ins("label", [text("b"), text("green")]),
                ],
                vec![
                    del("label", [text("b"), text("red")]),
                    del("edge", [text("a"), text("c")]),
                ],
            ],
        ),
        case(
            "constants",
            vec![
                rel("edge", [("from", Text), ("to", Text)]),
                derived("from_a", [("to", Text)]),
                derived("self_loop", [("node", Text)]),
                derived("seven", [("n", Int)]),
            ],
            vec![
                rule(atom("from_a", [v("b")]), [pos("edge", [text("a"), v("b")])]),
                rule(atom("self_loop", [v("x")]), [pos("edge", [v("x"), v("x")])]),
                rule(atom("seven", [int(7)]), []),
            ],
            vec![
                edges(&[("a", "b"), ("b", "b"), ("c", "a")], 1),
                edges(&[("a", "a"), ("a", "c")], 1),
                edges(&[("a", "a"), ("b", "b")], -1),
            ],
        ),
        case(
            "recursion chain",
            graph_rels(),
            reach_rules(),
            vec![
                edges(&[("1", "2"), ("2", "3"), ("3", "4")], 1),
                edges(&[("4", "5"), ("0", "1")], 1),
                edges(&[("5", "6")], 1),
            ],
        ),
        case(
            "cycle",
            graph_rels(),
            reach_rules(),
            vec![
                edges(&[("a", "b"), ("b", "c"), ("c", "a")], 1),
                edges(&[("c", "d")], 1),
                edges(&[("b", "c")], -1),
                edges(&[("b", "c")], 1),
            ],
        ),
        case(
            "retract mid chain",
            graph_rels(),
            reach_rules(),
            vec![
                edges(&[("1", "2"), ("2", "3"), ("3", "4"), ("4", "5")], 1),
                edges(&[("3", "4")], -1),
                edges(&[("2", "4")], 1),
                edges(&[("2", "3"), ("2", "4")], -1),
            ],
        ),
        case(
            "negation flips on retraction",
            graph_rels(),
            negation,
            vec![
                edges(&[("a", "b"), ("b", "c")], 1),
                vec![ins("blocked", [text("c")])],
                vec![del("blocked", [text("c")]), ins("blocked", [text("b")])],
                vec![del("blocked", [text("b")])],
            ],
        ),
        case(
            "add lt neq",
            vec![
                rel("edge", [("from", Text), ("to", Text)]),
                rel("count", [("node", Text), ("n", Int)]),
                derived("next", [("node", Text), ("n", Int)]),
                derived("step", [("node", Text)]),
                derived("differ", [("from", Text), ("to", Text)]),
            ],
            vec![
                rule(
                    atom("next", [v("node"), v("m")]),
                    [
                        pos("count", [v("node"), v("n")]),
                        call(Add, [v("n"), int(1), v("m")]),
                        call(Lt, [v("m"), int(10)]),
                    ],
                ),
                rule(
                    atom("step", [v("node")]),
                    [
                        pos("count", [v("node"), v("n")]),
                        pos("count", [v("node"), v("m")]),
                        call(Add, [v("n"), int(1), v("m")]),
                    ],
                ),
                rule(
                    atom("differ", [v("a"), v("b")]),
                    [pos("edge", [v("a"), v("b")]), call(Neq, [v("a"), v("b")])],
                ),
            ],
            vec![
                vec![
                    ins("count", [text("a"), int(3)]),
                    ins("count", [text("b"), int(9)]),
                    ins("count", [text("c"), int(-5)]),
                    ins("edge", [text("a"), text("a")]),
                    ins("edge", [text("a"), text("b")]),
                ],
                vec![
                    ins("count", [text("a"), int(4)]),
                    ins("count", [text("d"), int(-1)]),
                ],
                vec![
                    del("count", [text("a"), int(3)]),
                    del("edge", [text("a"), text("b")]),
                ],
            ],
        ),
        case(
            "two strata",
            two_strata_rels,
            two_strata,
            vec![
                vec![
                    ins("node", [text("hub")]),
                    ins("node", [text("x")]),
                    ins("node", [text("y")]),
                    ins("edge", [text("x"), text("hub")]),
                ],
                vec![ins("edge", [text("y"), text("x")])],
                vec![
                    del("edge", [text("x"), text("hub")]),
                    ins("edge", [text("y"), text("hub")]),
                ],
            ],
        ),
        case(
            "answer semijoin request",
            vec![
                rel("want", [("path", Text)]),
                request("ryi", "ask", [("path", Text)]),
                answer("ryi", "told", [("path", Text), ("callee", Text)]),
                derived("callee", [("name", Text)]),
            ],
            vec![
                rule(atom("ask", [v("p")]), [pos("want", [v("p")])]),
                rule(atom("callee", [v("c")]), [pos("told", [v("p"), v("c")])]),
            ],
            vec![
                vec![ins("told", [text("a"), text("f")])],
                vec![
                    ins("want", [text("a")]),
                    ins("told", [text("a"), text("g")]),
                ],
                vec![del("want", [text("a")])],
                vec![
                    ins("want", [text("a")]),
                    del("told", [text("a"), text("f")]),
                ],
            ],
        ),
        case(
            "answer feeds its request",
            vec![
                rel("seed", [("path", Text)]),
                request("ryi", "ask", [("path", Text)]),
                answer("ryi", "told", [("path", Text), ("callee", Text)]),
            ],
            vec![
                rule(atom("ask", [v("p")]), [pos("seed", [v("p")])]),
                rule(atom("ask", [v("c")]), [pos("told", [v("p"), v("c")])]),
            ],
            vec![
                vec![
                    ins("told", [text("a"), text("b")]),
                    ins("told", [text("b"), text("c")]),
                    ins("told", [text("x"), text("y")]),
                ],
                vec![ins("seed", [text("a")])],
                vec![del("told", [text("a"), text("b")])],
                vec![
                    ins("seed", [text("b")]),
                    ins("told", [text("c"), text("a")]),
                ],
            ],
        ),
        Case {
            name: "fixpoint limit rolls back",
            rels: vec![
                rel("seed", [("n", Int)]),
                rel("cap", [("n", Int)]),
                rel("open", [("flag", Int)]),
                derived("num", [("n", Int)]),
            ],
            rules: vec![
                rule(atom("num", [v("n")]), [pos("seed", [v("n")])]),
                rule(
                    atom("num", [v("m")]),
                    [
                        pos("num", [v("n")]),
                        pos("cap", [v("c")]),
                        call(Add, [v("n"), int(1), v("m")]),
                        call(Lt, [v("m"), v("c")]),
                    ],
                ),
                rule(
                    atom("num", [v("m")]),
                    [
                        pos("num", [v("n")]),
                        pos("open", [int(1)]),
                        call(Add, [v("n"), int(1), v("m")]),
                    ],
                ),
            ],
            steps: vec![
                vec![ins("seed", [int(0)]), ins("cap", [int(3)])],
                vec![ins("open", [int(1)]), ins("seed", [int(100)])],
                vec![del("cap", [int(3)]), ins("cap", [int(5)])],
                vec![ins("open", [int(1)])],
                vec![ins("seed", [int(2)]), del("seed", [int(0)])],
            ],
            limited: &[1, 3],
            max_rounds: 64,
        },
        case(
            "base row counted, set in delta",
            vec![
                rel("edge", [("from", Text), ("to", Text)]),
                derived("reach", [("from", Text), ("to", Text)]),
            ],
            reach_rules(),
            vec![
                edges(&[("a", "b")], 1),
                edges(&[("a", "b"), ("b", "c")], 1),
                edges(&[("a", "b")], -1),
                edges(&[("a", "b")], -1),
            ],
        ),
    ]
}

/// Runs every step, checking the summed deltas against the oracle after each one.
fn run(
    name: &str,
    plan: &Plan,
    steps: &[Vec<Change>],
    limited: &[usize],
    max_rounds: u32,
) -> Vec<BTreeSet<Row>> {
    let mut engine = Dd::new(plan, Bounds { max_rounds }).expect("engine");
    let mut oracle = Oracle::default();
    let mut state = vec![BTreeSet::new(); plan.rels.len()];
    let mut event = 0;
    for (index, step) in steps.iter().enumerate() {
        match engine.step(step.clone()) {
            Err(Diag::FixpointLimit { .. }) if limited.contains(&index) => {}
            Err(diag) => panic!("step {index}: {diag:?}"),
            Ok(delta) => {
                assert!(!limited.contains(&index), "step {index} passed its limit");
                event += 1;
                assert_eq!(delta.event, event);
                assert!(
                    delta.changes.windows(2).all(|pair| pair[0] < pair[1]),
                    "sorted"
                );
                for change in delta.changes {
                    let rows = &mut state[change.rel as usize];
                    match change.diff {
                        1 => assert!(
                            rows.insert(change.row),
                            "{name} step {index}: +1 on a present row"
                        ),
                        -1 => assert!(
                            rows.remove(&change.row),
                            "{name} step {index}: -1 on an absent row"
                        ),
                        other => panic!("step {index}: diff {other}"),
                    }
                }
                oracle.apply(step);
            }
        }
        let expected = oracle.eval(plan);
        for (id, rows) in state.iter().enumerate() {
            assert_eq!(
                rows, &expected[id],
                "{name} step {index}: {}",
                plan.rels[id].name
            );
        }
    }
    state
}

fn decode(plan: &Plan, dict: &Dict, state: &[BTreeSet<Row>]) -> Vec<String> {
    state
        .iter()
        .enumerate()
        .map(|(id, rows)| {
            let rel = &plan.rels[id];
            let mut rows: Vec<String> = rows
                .iter()
                .map(|row| {
                    let cells: Vec<String> = row
                        .iter()
                        .zip(&rel.cols)
                        .map(|(value, col)| match col {
                            Text => String::from_utf8_lossy(&dict.bytes[value]).into_owned(),
                            Int => (*value as i64).to_string(),
                        })
                        .collect();
                    format!("({})", cells.join(" "))
                })
                .collect();
            rows.sort();
            format!("{}: {}", rel.name, rows.join(" "))
        })
        .collect()
}

fn lower_steps(plan: &Plan, dict: &mut Dict, steps: &[Step]) -> Vec<Vec<Change>> {
    steps
        .iter()
        .map(|step| {
            step.iter()
                .map(|(rel, args, diff)| Change {
                    rel: plan
                        .rels
                        .iter()
                        .position(|candidate| candidate.name == *rel)
                        .expect("rel") as u32,
                    row: args
                        .iter()
                        .map(|arg| match arg {
                            Term::Text(value) => dict.intern(value.as_bytes()),
                            Term::Int(value) => *value as Value,
                            Term::Var(name) => panic!("var {name} in a step"),
                        })
                        .collect(),
                    diff: *diff,
                })
                .collect()
        })
        .collect()
}

#[test]
fn every_case_equals_the_oracle_after_every_step() {
    let mut out = Vec::new();
    for case in cases() {
        let mut dict = Dict::default();
        let plan = check(
            Program {
                rels: case.rels,
                rules: case.rules,
            },
            &mut dict,
        )
        .unwrap_or_else(|diags| panic!("{}: {diags:?}", case.name));
        let steps = lower_steps(&plan, &mut dict, &case.steps);
        let state = run(case.name, &plan, &steps, case.limited, case.max_rounds);
        out.push((case.name, decode(&plan, &dict, &state)));
    }
    insta::assert_debug_snapshot!(out);
}

/// xorshift64: the same edges every run.
fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[test]
fn random_graph_equals_the_oracle_under_inserts_and_retracts() {
    const NODES: u64 = 4000;
    let mut dict = Dict::default();
    let mut rules = reach_rules();
    rules.push(rule(
        atom("open", [v("a"), v("c")]),
        [pos("reach", [v("a"), v("c")]), neg("blocked", [v("c")])],
    ));
    let plan = check(
        Program {
            rels: graph_rels(),
            rules,
        },
        &mut dict,
    )
    .expect("checks");
    let mut seed = 0x9e37_79b9_7f4a_7c15;
    let mut node = |seed: &mut u64| dict.intern(format!("n{}", next(seed) % NODES).as_bytes());
    let mut edge = |seed: &mut u64, diff| Change {
        rel: 0,
        row: vec![node(seed), node(seed)],
        diff,
    };
    let mut live: Vec<Change> = (0..2500).map(|_| edge(&mut seed, 1)).collect();
    let mut steps = vec![live.clone()];
    let more: Vec<Change> = (0..500).map(|_| edge(&mut seed, 1)).collect();
    live.extend(more.clone());
    steps.push(more);
    let mut blocked: Vec<Change> = live[..40]
        .iter()
        .map(|change| Change {
            rel: 1,
            row: vec![change.row[1]],
            diff: 1,
        })
        .collect();
    blocked.sort();
    blocked.dedup();
    steps.push(blocked);
    for round in 0..3 {
        let cut = (next(&mut seed) % 2000) as usize;
        let mut step: Vec<Change> = live
            .drain(cut..cut + 300 - round * 50)
            .map(|change| Change { diff: -1, ..change })
            .collect();
        let added: Vec<Change> = (0..200).map(|_| edge(&mut seed, 1)).collect();
        live.extend(added.clone());
        step.extend(added);
        steps.push(step);
    }
    let started = Instant::now();
    let state = run("random", &plan, &steps, &[], 10_000);
    let elapsed = started.elapsed();
    let sizes: Vec<usize> = state.iter().map(BTreeSet::len).collect();
    println!("rows per rel {sizes:?} in {elapsed:?}");
    assert!(sizes[2] > 3000, "reach is non-trivial: {sizes:?}");
    assert!(elapsed.as_secs() < 10, "{elapsed:?}");
}

/// A service chain (`next_ask(n)` answered by `next_told(n, n + 1)`, which asks `n + 1`) runs one
/// link per event for many events under a small `max_rounds`: no single event runs away, so no
/// step is limited. `open(1)` makes the same stratum a runaway counter; opened late in the chain
/// it still stops with `FixpointLimit`, rolls back, and the chain goes on after it.
#[test]
fn max_rounds_bounds_one_event_and_a_service_chain_runs_past_it() {
    const LINKS: u64 = 40;
    const LATE: u64 = 30;
    let mut dict = Dict::default();
    let plan = check(
        Program {
            rels: vec![
                rel("start", [("n", Int)]),
                request("succ", "next_ask", [("n", Int)]),
                answer("succ", "next_told", [("n", Int), ("m", Int)]),
                rel("open", [("x", Int)]),
            ],
            rules: vec![
                rule(atom("next_ask", [v("n")]), [pos("start", [v("n")])]),
                rule(
                    atom("next_ask", [v("m")]),
                    [pos("next_told", [v("n"), v("m")])],
                ),
                rule(
                    atom("next_ask", [v("m")]),
                    [
                        pos("next_ask", [v("n")]),
                        pos("open", [int(1)]),
                        call(Add, [v("n"), int(1), v("m")]),
                    ],
                ),
            ],
        },
        &mut dict,
    )
    .expect("checks");
    let id = |name: &str| {
        plan.rels
            .iter()
            .position(|rel| rel.name == name)
            .expect("rel") as u32
    };
    let change = |name: &str, row: Vec<u64>, diff: isize| Change {
        rel: id(name),
        row,
        diff,
    };
    let mut engine = Dd::new(&plan, Bounds { max_rounds: 8 }).expect("engine");
    let mut log = Vec::new();
    let mut input = vec![change("start", vec![0], 1)];
    for link in 0..LINKS {
        if link == LATE {
            let runaway = engine.step(vec![change("open", vec![1], 1)]);
            log.push(format!("link {link} runaway: {runaway:?}"));
        }
        let delta = engine
            .step(input)
            .unwrap_or_else(|diag| panic!("link {link}: {diag:?}"));
        let asked: Vec<u64> = delta
            .changes
            .iter()
            .filter(|change| change.rel == id("next_ask") && change.diff > 0)
            .map(|change| change.row[0])
            .collect();
        assert_eq!(asked, [link], "link {link}: {:?}", delta.changes);
        input = asked
            .iter()
            .map(|n| change("next_told", vec![*n, n + 1], 1))
            .collect();
    }
    let delta = engine.step(input).expect("last answer");
    log.push(format!("last: event {} {:?}", delta.event, delta.changes));
    insta::assert_debug_snapshot!(log);
}
