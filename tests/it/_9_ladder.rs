//! Fixed-shape ladder programs through the public `dl8 eval` command.
//! Counters unavailable at this boundary are recorded in the commit body.

use super::_2_eval_oracles::{call, constant, goal, rule, variable};
use hafley_observe::{assert_growth_sized, Growth, SpanCounts};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);
const ENGINES: [&str; 5] = ["rust", "sqlite", "dd", "ir:dd", "ir:sqlite"];

fn run(rung: usize, size: usize, engine: &str, program: Value) -> (Vec<Value>, String) {
    let path = std::env::temp_dir().join(format!(
        "dl8-ladder-{}-{}-{}-{}.json",
        std::process::id(),
        rung,
        size,
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, serde_json::to_vec(&program).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("eval")
        .arg(&path)
        .arg("--trace")
        .args(if rung == 8 {
            &["--serve", "page_request"][..]
        } else {
            &[]
        })
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!("rung={rung} size={size} engine={engine} counter=json: {error}; {stderr}")
    });
    assert_eq!(
        output.status.code(),
        Some(0),
        "rung={rung} size={size} engine={engine} counter=exit: {value}; {stderr}"
    );
    assert_eq!(
        value["diagnostics"],
        json!([]),
        "rung={rung} size={size} engine={engine} counter=diagnostics"
    );
    (value["closure"].as_array().unwrap().clone(), stderr)
}

fn rows(closure: &[Value], name: &str) -> Vec<Vec<Value>> {
    let relation = super::_2_eval_oracles::rel(name);
    closure
        .iter()
        .filter(|row| row["rel"] == relation)
        .map(|row| row["args"].as_array().unwrap().clone())
        .collect()
}

fn check_count(rung: usize, size: usize, engine: &str, counter: &str, got: usize, want: usize) {
    assert_eq!(
        got, want,
        "rung={rung} size={size} engine={engine} counter={counter}"
    );
}

fn program(seeds: Vec<Value>, rules: Vec<Value>) -> Value {
    json!({"seeds":seeds,"rules":rules})
}

fn linear_rows(count: usize) -> SpanCounts {
    SpanCounts {
        entries: BTreeMap::from([("closure_rows".into(), count)]),
        ..SpanCounts::default()
    }
}

fn trace_count(trace: &str, marker: &str) -> usize {
    trace
        .lines()
        .filter_map(|line| line.split(marker).nth(1))
        .filter_map(|tail| tail.split(|c: char| !c.is_ascii_digit()).next())
        .filter_map(|digits| digits.parse::<usize>().ok())
        .sum()
}

#[test]
fn rung_0_seeds() {
    for engine in ENGINES {
        for size in 0..=3 {
            let seeds = (0..size)
                .map(|i| call("seed", vec![constant(i as i8)]))
                .collect();
            let (closure, _) = run(0, size, engine, program(seeds, vec![]));
            check_count(
                0,
                size,
                engine,
                "seed_rows",
                rows(&closure, "seed").len(),
                size,
            );
            if size > 0 {
                let repeated = vec![call("seed", vec![constant(0)]); size];
                let (closure, _) = run(0, size, engine, program(repeated, vec![]));
                check_count(
                    0,
                    size,
                    engine,
                    "duplicate_free_rows",
                    rows(&closure, "seed").len(),
                    1,
                );
            }
        }
        let small = run(
            0,
            8,
            engine,
            program(
                (0..8)
                    .map(|i| call("seed", vec![json!({"f":"const","args":[i]})]))
                    .collect(),
                vec![],
            ),
        )
        .0;
        let large = run(
            0,
            800,
            engine,
            program(
                (0..800)
                    .map(|i| call("seed", vec![json!({"f":"const","args":[i]})]))
                    .collect(),
                vec![],
            ),
        )
        .0;
        assert_growth_sized(
            &linear_rows(rows(&small, "seed").len()),
            &linear_rows(rows(&large, "seed").len()),
            "closure_rows",
            8,
            800,
            Growth::Linear,
        );
    }
}

#[test]
fn rung_1_filter_project() {
    for engine in ENGINES {
        for size in 0..=3 {
            let seeds = (0..size)
                .map(|i| call("source", vec![constant(i as i8), constant((i % 2) as i8)]))
                .collect();
            let rules = vec![rule(
                "selected",
                vec![variable(0)],
                vec![goal("source", vec![variable(0), constant(1)], false)],
            )];
            let (closure, _) = run(1, size, engine, program(seeds, rules));
            check_count(
                1,
                size,
                engine,
                "selected_rows",
                rows(&closure, "selected").len(),
                size / 2,
            );
        }
    }
}

#[test]
fn rung_2_join() {
    for engine in ENGINES {
        for size in 0..=3 {
            let mut seeds = Vec::new();
            for i in 0..size {
                seeds.push(call(
                    "left",
                    vec![constant(i as i8), constant((i % 2) as i8)],
                ));
                seeds.push(call(
                    "right",
                    vec![constant((i % 2) as i8), constant(i as i8)],
                ));
            }
            let rules = vec![rule(
                "joined",
                vec![variable(0), variable(2)],
                vec![
                    goal("left", vec![variable(0), variable(1)], false),
                    goal("right", vec![variable(1), variable(2)], false),
                ],
            )];
            let (closure, _) = run(2, size, engine, program(seeds, rules));
            let want = (0..size)
                .map(|i| (0..size).filter(|j| i % 2 == j % 2).count())
                .sum();
            check_count(
                2,
                size,
                engine,
                "joined_rows",
                rows(&closure, "joined").len(),
                want,
            );
        }
    }
}

#[test]
fn rung_3_negation() {
    for engine in ENGINES {
        for size in 0..=3 {
            let mut seeds = vec![call("left", vec![constant(0)])];
            seeds.extend((0..size).map(|_| call("blocked", vec![constant(0)])));
            let rules = vec![rule(
                "kept",
                vec![variable(0)],
                vec![
                    goal("left", vec![variable(0)], false),
                    goal("blocked", vec![variable(0)], true),
                ],
            )];
            let (closure, _) = run(3, size, engine, program(seeds, rules));
            check_count(
                3,
                size,
                engine,
                "kept_rows",
                rows(&closure, "kept").len(),
                usize::from(size == 0),
            );
        }
    }
}

#[test]
fn rung_4_aggregates() {
    for engine in ENGINES {
        for size in 0..=3 {
            for kind in ["count", "sum", "min", "max"] {
                let seeds = (0..size)
                    .map(|i| call("source", vec![constant(i as i8)]))
                    .collect();
                let rules = vec![rule(
                    "folded",
                    vec![json!({kind:variable(0)})],
                    vec![goal("source", vec![variable(0)], false)],
                )];
                let (closure, _) = run(4, size, engine, program(seeds, rules));
                let actual = rows(&closure, "folded");
                check_count(4, size, engine, kind, actual.len(), usize::from(size > 0));
                if size > 0 {
                    let want = match kind {
                        "count" => size,
                        "sum" => size * (size - 1) / 2,
                        "min" => 0,
                        "max" => size - 1,
                        _ => unreachable!(),
                    };
                    assert_eq!(
                        actual[0],
                        vec![json!({"f":"const","args":[want]})],
                        "rung=4 size={size} engine={engine} counter={kind}_value"
                    );
                }
            }
        }
    }
}

#[test]
fn rung_5_recursion() {
    for engine in ENGINES {
        for size in 0..=3 {
            let seeds = (0..size)
                .map(|i| call("edge", vec![constant(i as i8), constant((i + 1) as i8)]))
                .collect();
            let rules = vec![
                rule(
                    "path",
                    vec![variable(0), variable(1)],
                    vec![goal("edge", vec![variable(0), variable(1)], false)],
                ),
                rule(
                    "path",
                    vec![variable(0), variable(2)],
                    vec![
                        goal("path", vec![variable(0), variable(1)], false),
                        goal("edge", vec![variable(1), variable(2)], false),
                    ],
                ),
            ];
            let (closure, trace) = run(5, size, engine, program(seeds, rules));
            check_count(
                5,
                size,
                engine,
                "path_rows",
                rows(&closure, "path").len(),
                size * (size + 1) / 2,
            );
            if engine == "rust" && size > 0 {
                assert!(
                    trace.contains("Round"),
                    "rung=5 size={size} engine={engine} counter=round_trace"
                );
            }
            if size >= 2 {
                let cycle = (0..size)
                    .map(|i| {
                        call(
                            "edge",
                            vec![constant(i as i8), constant(((i + 1) % size) as i8)],
                        )
                    })
                    .collect();
                let cycle_rules = vec![
                    rule(
                        "path",
                        vec![variable(0), variable(1)],
                        vec![goal("edge", vec![variable(0), variable(1)], false)],
                    ),
                    rule(
                        "path",
                        vec![variable(0), variable(2)],
                        vec![
                            goal("path", vec![variable(0), variable(1)], false),
                            goal("edge", vec![variable(1), variable(2)], false),
                        ],
                    ),
                ];
                let (closure, _) = run(5, size, engine, program(cycle, cycle_rules));
                check_count(
                    5,
                    size,
                    engine,
                    "cycle_path_rows",
                    rows(&closure, "path").len(),
                    size * size,
                );
            }
        }
    }
}

#[test]
fn rung_6_kernels() {
    for engine in ENGINES {
        for size in 0..=3 {
            let seeds = (0..size)
                .map(|i| call("pair", vec![constant(i as i8), constant(1)]))
                .collect();
            let rules = vec![rule(
                "added",
                vec![variable(2)],
                vec![
                    goal("pair", vec![variable(0), variable(1)], false),
                    goal(
                        "int.add",
                        vec![variable(0), variable(1), variable(2)],
                        false,
                    ),
                ],
            )];
            let (closure, _) = run(6, size, engine, program(seeds, rules));
            check_count(
                6,
                size,
                engine,
                "added_rows",
                rows(&closure, "added").len(),
                size,
            );
            let seeds = (0..size)
                .map(|i| call("source", vec![constant(i as i8)]))
                .collect();
            let rules = vec![rule(
                "list",
                vec![variable(2)],
                vec![
                    goal("source", vec![variable(0)], false),
                    goal("nil", vec![variable(1)], false),
                    goal("cons", vec![variable(0), variable(1), variable(2)], false),
                ],
            )];
            let (closure, _) = run(6, size, engine, program(seeds, rules));
            check_count(
                6,
                size,
                engine,
                "cons_rows",
                rows(&closure, "list").len(),
                size,
            );
            let rules = (0..size)
                .map(|i| {
                    rule(
                        "constructed",
                        vec![variable(0)],
                        vec![goal(
                            "intern",
                            vec![
                                super::_2_eval_oracles::rel("ctor"),
                                json!({"f":"const","args":[[constant(i as i8)]]}),
                                variable(0),
                            ],
                            false,
                        )],
                    )
                })
                .collect();
            let (closure, _) = run(6, size, engine, program(vec![], rules));
            check_count(
                6,
                size,
                engine,
                "interned_distinct_terms",
                rows(&closure, "constructed").len(),
                size,
            );
        }
    }
}

#[test]
fn rung_7_demand() {
    for engine in ENGINES {
        for size in 0..=3 {
            let seeds = (0..size)
                .map(|i| call("source", vec![constant(i as i8)]))
                .collect();
            let bound = json!({"rel":{"f":"ref","args":[{"f":"kernel","args":[{"a":"int"},{"a":"lt"}]}]},"args":[variable(0),constant(50)],"polarity":"positive"});
            let rules = vec![
                rule("bound_check", vec![variable(0)], vec![bound]),
                rule(
                    "derived",
                    vec![variable(0)],
                    vec![
                        goal("source", vec![variable(0)], false),
                        goal("bound_check", vec![variable(0)], false),
                    ],
                ),
            ];
            let (closure, trace) = run(7, size, engine, program(seeds, rules));
            check_count(
                7,
                size,
                engine,
                "derived_rows",
                rows(&closure, "derived").len(),
                size,
            );
            if engine == "rust" && size > 0 {
                let calls = trace_count(&trace, "Demand { calls: ");
                assert!(
                    calls > 0 && calls <= 4 * size,
                    "rung=7 size={size} engine={engine} counter=demand_calls: {calls}"
                );
            }
        }
    }
}

#[test]
fn rung_8_effects() {
    for engine in ENGINES {
        for size in 0..=3 {
            let seeds = (0..size)
                .map(|i| call("source", vec![constant(i as i8)]))
                .collect();
            let rules = vec![rule(
                "result",
                vec![variable(0)],
                vec![
                    goal("source", vec![variable(0)], false),
                    goal("page_request", vec![variable(0)], false),
                ],
            )];
            let mut case = program(seeds, rules);
            case["names"] = json!({"page_request":super::_2_eval_oracles::rel("page_request")});
            let (closure, _) = run(8, size, engine, case);
            let effect = json!({"f":"ref","args":[{"f":"kernel","args":[{"a":"effect"}]}]});
            check_count(
                8,
                size,
                engine,
                "effect_rows",
                closure.iter().filter(|row| row["rel"] == effect).count(),
                size,
            );
        }
    }
}

#[test]
fn rung_9_comptime_rounds() {
    for engine in ENGINES {
        let empty = json!({"nodes":[],"edges":[],"relations":[],"rules":[],"seeds":[],"depends":[],"strata":[]});
        let case = json!({"entry":"evaluate_checked","input":{"checked":empty,"replay":[{"deferred":false,"facts_len":1,"generated_len":0,"checked":empty,"diagnostics":[]}]}});
        let path = std::env::temp_dir().join(format!(
            "dl8-ladder-comptime-{}-{}.json",
            std::process::id(),
            engine
        ));
        std::fs::write(&path, serde_json::to_vec(&case).unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("comptime")
            .arg(&path)
            .env("DL8_ENGINE", engine)
            .output()
            .unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "rung=9 size=0 engine={engine} counter=comptime_exit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let out: Value = serde_json::from_slice(&output.stdout).unwrap();
        let stable: Vec<_> = out["rounds"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["event"] == "decision" && row["outcome"] == "stable")
            .collect();
        check_count(
            9,
            0,
            engine,
            "stable_round",
            stable[0]["round"].as_u64().unwrap() as usize,
            1,
        );
        for size in 0..=3 {
            let seeds = vec![call("stage0", vec![constant(0)])];
            let rules = (0..size)
                .map(|i| {
                    rule(
                        &format!("stage{}", i + 1),
                        vec![variable(0)],
                        vec![goal(&format!("stage{i}"), vec![variable(0)], false)],
                    )
                })
                .collect();
            let (closure, _) = run(9, size, engine, program(seeds, rules));
            check_count(
                9,
                size,
                engine,
                "last_stage_rows",
                rows(&closure, &format!("stage{size}")).len(),
                1,
            );
        }
    }
}
