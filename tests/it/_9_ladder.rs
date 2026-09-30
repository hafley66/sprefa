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

fn run(
    rung: usize,
    size: usize,
    engine: &str,
    program: Value,
) -> (Vec<Value>, Option<Value>, String) {
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
    let counters = value.get("counters").cloned();
    if engine.starts_with("ir:") {
        assert!(
            counters.is_some(),
            "rung={rung} size={size} engine={engine} counter=object"
        );
    } else {
        assert!(
            counters.is_none(),
            "rung={rung} size={size} engine={engine} counter=absent"
        );
    }
    (
        value["closure"].as_array().unwrap().clone(),
        counters,
        stderr,
    )
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

fn counter_counts(counters: &Value, path: &[&str]) -> SpanCounts {
    let value = path
        .iter()
        .fold(counters, |value, key| &value[*key])
        .as_u64()
        .unwrap_or_else(|| panic!("counter path {} is absent", path.join(".")));
    SpanCounts {
        entries: BTreeMap::from([(path.join("."), value as usize)]),
        ..SpanCounts::default()
    }
}

fn assert_counter_growth(
    rung: usize,
    engine: &str,
    small: &Value,
    large: &Value,
    path: &[&str],
    small_n: usize,
    large_n: usize,
    growth: Growth,
) {
    let name = path.join(".");
    for (size, counters) in [(small_n, small), (large_n, large)] {
        assert!(
            path.iter()
                .fold(counters, |value, key| &value[*key])
                .is_u64(),
            "rung={rung} size={size} engine={engine} counter={name}"
        );
    }
    let result = std::panic::catch_unwind(|| {
        assert_growth_sized(
            &counter_counts(small, path),
            &counter_counts(large, path),
            &name,
            small_n,
            large_n,
            growth,
        );
    });
    assert!(
        result.is_ok(),
        "rung={rung} size={small_n}->{large_n} engine={engine} counter={name}"
    );
}

fn trace_count(trace: &str, marker: &str) -> usize {
    trace
        .lines()
        .filter_map(|line| line.split(marker).nth(1))
        .filter_map(|tail| tail.split(|c: char| !c.is_ascii_digit()).next())
        .filter_map(|digits| digits.parse::<usize>().ok())
        .sum()
}

fn fold_frontier(
    size: usize,
    engine: &str,
    step: Value,
    step_rule: Option<Value>,
    append: i64,
) -> Value {
    let input = super::_2_eval_oracles::rel("input");
    let total = super::_2_eval_oracles::rel("total");
    let mut rules = vec![rule(
        "total",
        vec![json!({"f":"fold","args":[step, constant(0), variable(0)]})],
        vec![goal("input", vec![variable(0)], false)],
    )];
    if let Some(step_rule) = step_rule {
        rules.push(step_rule);
    }
    let program = json!({
        "names": {"input": input, "total": total},
        "seeds": (1..=size).map(|n| call("input", vec![json!({"f":"const","args":[n]})])).collect::<Vec<_>>(),
        "rules": rules,
    });
    let frontiers = json!([{
        "changes": [{"rel":"input", "args":[json!({"f":"const","args":[append]})], "w":1}]
    }]);
    let id = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let program_path = dir.join(format!("dl8-fold-ladder-{}-{id}.json", std::process::id()));
    let frontiers_path = dir.join(format!(
        "dl8-fold-frontiers-{}-{id}.json",
        std::process::id()
    ));
    std::fs::write(&program_path, serde_json::to_vec(&program).unwrap()).unwrap();
    std::fs::write(&frontiers_path, serde_json::to_vec(&frontiers).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("run")
        .arg(&program_path)
        .arg("--frontiers")
        .arg(&frontiers_path)
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    std::fs::remove_file(program_path).unwrap();
    std::fs::remove_file(frontiers_path).unwrap();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "engine={engine} size={size} fold frontier json: {error}; {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(
        output.status.code(),
        Some(0),
        "engine={engine} size={size}: {value}"
    );
    assert_eq!(
        value["diagnostics"],
        json!([]),
        "engine={engine} size={size}: {value}"
    );
    value["frontiers"][0].clone()
}

#[test]
fn rung_7_declared_fold_growth() {
    let int_add = json!({"f":"ref","args":[{"f":"kernel","args":[{"a":"int"},{"a":"add"}]}]});
    let custom_step = super::_2_eval_oracles::rel("step");
    let step_rule = rule(
        "step",
        vec![variable(0), variable(1), variable(2)],
        vec![json!({
            "rel": int_add,
            "args": [variable(0), variable(1), variable(2)],
            "polarity": "positive",
        })],
    );
    for engine in ["ir:dd", "ir:sqlite"] {
        let small = fold_frontier(8, engine, int_add.clone(), None, 9);
        let large = fold_frontier(80, engine, int_add.clone(), None, 81);
        for (size, frontier) in [(8, &small), (80, &large)] {
            let closure = frontier["closure"]["closure"].as_array().unwrap();
            let total = (size * (size + 1) / 2 + size + 1) as i64;
            assert_eq!(
                rows(closure, "total"),
                vec![vec![json!({"f":"const","args":[total]})]],
                "engine={engine} size={size}"
            );
        }
        assert_counter_growth(
            7,
            engine,
            &small["counters"],
            &large["counters"],
            &["delta_rows", "window"],
            8,
            80,
            Growth::Constant,
        );

        let small = fold_frontier(8, engine, custom_step.clone(), Some(step_rule.clone()), 0);
        let large = fold_frontier(80, engine, custom_step.clone(), Some(step_rule.clone()), 0);
        for (size, frontier) in [(8, &small), (80, &large)] {
            let closure = frontier["closure"]["closure"].as_array().unwrap();
            let total = (size * (size + 1) / 2) as i64;
            assert_eq!(
                rows(closure, "total"),
                vec![vec![json!({"f":"const","args":[total]})]],
                "engine={engine} size={size}"
            );
        }
        assert_counter_growth(
            7,
            engine,
            &small["counters"],
            &large["counters"],
            &["rows_written"],
            8,
            80,
            Growth::Linear,
        );
    }
}

#[test]
fn rung_7_declared_fold_algebras() {
    let steps = [
        (
            json!({"f":"kernel","args":[{"a":"int"},{"a":"add"}]}),
            5,
            11,
        ),
        (json!({"f":"kernel","args":[{"a":"count_step"}]}), 0, 3),
        (json!({"f":"kernel","args":[{"a":"min_step"}]}), 0, 0),
        (json!({"f":"kernel","args":[{"a":"max_step"}]}), 0, 3),
    ];
    for engine in ["rust", "ir:dd", "ir:sqlite"] {
        for (step, seed, expected) in &steps {
            let seeds = (1..=3)
                .map(|value| call("input", vec![constant(value)]))
                .collect();
            let rules = vec![rule(
                "total",
                vec![
                    json!({"f":"fold","args":[{"f":"ref","args":[step]},constant(*seed),variable(0)]}),
                ],
                vec![goal("input", vec![variable(0)], false)],
            )];
            let (closure, _, _) = run(7, 3, engine, program(seeds, rules));
            assert_eq!(
                rows(&closure, "total"),
                vec![vec![constant(*expected)]],
                "engine={engine} step={step}"
            );
        }
    }
}

#[test]
fn rung_0_seeds() {
    for engine in ENGINES {
        for size in 0..=3 {
            let seeds = (0..size)
                .map(|i| call("seed", vec![constant(i as i8)]))
                .collect();
            let (closure, _, _) = run(0, size, engine, program(seeds, vec![]));
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
                let (closure, _, _) = run(0, size, engine, program(repeated, vec![]));
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
        let (small, small_counters, _) = run(
            0,
            8,
            engine,
            program(
                (0..8)
                    .map(|i| call("seed", vec![json!({"f":"const","args":[i]})]))
                    .collect(),
                vec![],
            ),
        );
        let (large, large_counters, _) = run(
            0,
            800,
            engine,
            program(
                (0..800)
                    .map(|i| call("seed", vec![json!({"f":"const","args":[i]})]))
                    .collect(),
                vec![],
            ),
        );
        assert_growth_sized(
            &linear_rows(rows(&small, "seed").len()),
            &linear_rows(rows(&large, "seed").len()),
            "closure_rows",
            8,
            800,
            Growth::Linear,
        );
        if engine.starts_with("ir:") {
            assert_counter_growth(
                0,
                engine,
                small_counters.as_ref().unwrap(),
                large_counters.as_ref().unwrap(),
                &["rows_written"],
                8,
                800,
                Growth::Linear,
            );
        }
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
            let (closure, _, _) = run(1, size, engine, program(seeds, rules));
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
            let (closure, _, _) = run(2, size, engine, program(seeds, rules));
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
    for engine in ["ir:dd", "ir:sqlite"] {
        let measure = |size| {
            let mut seeds: Vec<_> = (0..size)
                .map(|i| call("padding", vec![json!({"f":"const","args":[i]})]))
                .collect();
            seeds.push(call("left", vec![constant(0)]));
            seeds.push(call("right", vec![constant(0)]));
            let rules = vec![rule(
                "joined",
                vec![variable(0)],
                vec![
                    goal("left", vec![variable(0)], false),
                    goal("right", vec![variable(0)], false),
                ],
            )];
            run(2, size, engine, program(seeds, rules)).1.unwrap()
        };
        assert_counter_growth(
            2,
            engine,
            &measure(8),
            &measure(80),
            &["delta_rows", "join"],
            8,
            80,
            Growth::Constant,
        );
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
            let (closure, _, _) = run(3, size, engine, program(seeds, rules));
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
                let (closure, _, _) = run(4, size, engine, program(seeds, rules));
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
            let (closure, _, trace) = run(5, size, engine, program(seeds, rules));
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
                let (closure, _, _) = run(5, size, engine, program(cycle, cycle_rules));
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
    for engine in ["ir:dd", "ir:sqlite"] {
        let measure = |size| {
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
            run(5, size, engine, program(seeds, rules)).1.unwrap()
        };
        assert_counter_growth(
            5,
            engine,
            &measure(8),
            &measure(32),
            &["rounds"],
            8,
            32,
            Growth::Linear,
        );
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
            let (closure, _, _) = run(6, size, engine, program(seeds, rules));
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
            let (closure, _, _) = run(6, size, engine, program(seeds, rules));
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
            let (closure, _, _) = run(6, size, engine, program(vec![], rules));
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
    for engine in ["ir:dd", "ir:sqlite"] {
        let measure = |size| {
            let rules = (0..size)
                .map(|i| {
                    rule(
                        "constructed",
                        vec![variable(0)],
                        vec![goal(
                            "intern",
                            vec![
                                super::_2_eval_oracles::rel("ctor"),
                                json!({"f":"const","args":[[json!({"f":"const","args":[i]})]]}),
                                variable(0),
                            ],
                            false,
                        )],
                    )
                })
                .collect();
            run(6, size, engine, program(vec![], rules)).1.unwrap()
        };
        assert_counter_growth(
            6,
            engine,
            &measure(8),
            &measure(80),
            &["interned"],
            8,
            80,
            Growth::Linear,
        );
    }
}

#[test]
fn rung_6_tuple_feeds_intern() {
    let kernel = |name: &str, args: Vec<Value>| {
        json!({
            "rel":{"f":"ref","args":[{"f":"kernel","args":[{"a":name}]}]},
            "args":args,"polarity":"positive"
        })
    };
    let seed = call(
        "field",
        vec![
            json!({"f":"ref","args":[{"f":"node","args":[1]}]}),
            json!({"f":"const","args":[{"a":"name"}]}),
        ],
    );
    let rules = vec![rule(
        "made",
        vec![variable(3)],
        vec![
            goal("field", vec![variable(0), variable(1)], false),
            kernel("tuple", vec![variable(0), variable(1), variable(2)]),
            kernel(
                "intern",
                vec![json!({"f":"ref","args":[{"a":"pair"}]}), variable(2), variable(3)],
            ),
        ],
    )];
    let mut reference = None;
    for engine in ENGINES {
        let (closure, _, _) = run(6, 1, engine, program(vec![seed.clone()], rules.clone()));
        assert_eq!(rows(&closure, "made").len(), 1, "engine={engine}");
        match &reference {
            None => reference = Some(closure),
            Some(want) => assert_eq!(&closure, want, "engine={engine}"),
        }
    }
}

#[test]
fn rung_6_scalar_kernel_edges() {
    let bare = |name: &str, args: Vec<Value>| {
        json!({
            "rel":{"f":"ref","args":[{"f":"kernel","args":[{"a":name}]}]},
            "args":args,"polarity":"positive"
        })
    };
    let typed = |owner: &str, name: &str, args: Vec<Value>| {
        json!({
            "rel":{"f":"ref","args":[{"f":"kernel","args":[{"a":owner},{"a":name}]}]},
            "args":args,"polarity":"positive"
        })
    };
    let rules = vec![
        rule(
            "counted",
            vec![variable(0)],
            vec![bare(
                "count_step",
                vec![constant(2), constant(99), variable(0)],
            )],
        ),
        rule(
            "minimum",
            vec![variable(0)],
            vec![bare(
                "min_step",
                vec![constant(2), constant(1), variable(0)],
            )],
        ),
        rule(
            "maximum",
            vec![variable(0)],
            vec![bare(
                "max_step",
                vec![constant(2), constant(1), variable(0)],
            )],
        ),
        rule(
            "empty_str",
            vec![variable(0)],
            vec![typed("str", "nil", vec![variable(0)])],
        ),
        rule(
            "joined_str",
            vec![variable(0)],
            vec![typed(
                "str",
                "cons",
                vec![
                    json!({"f":"const","args":[{"s":"é"}]}),
                    json!({"f":"const","args":[{"s":"cho"}]}),
                    variable(0),
                ],
            )],
        ),
        rule(
            "split_str",
            vec![variable(0), variable(1)],
            vec![typed(
                "str",
                "cons",
                vec![
                    variable(0),
                    variable(1),
                    json!({"f":"const","args":[{"s":"écho"}]}),
                ],
            )],
        ),
        rule(
            "split_empty_str",
            vec![variable(0)],
            vec![typed(
                "str",
                "cons",
                vec![
                    variable(0),
                    variable(1),
                    json!({"f":"const","args":[{"s":""}]}),
                ],
            )],
        ),
        rule(
            "non_integer",
            vec![],
            vec![typed(
                "int",
                "lt",
                vec![json!({"f":"const","args":[{"a":"oops"}]}), constant(2)],
            )],
        ),
    ];
    for engine in ["rust", "dd", "ir:dd", "ir:sqlite"] {
        let (closure, _, _) = run(6, 4, engine, program(vec![], rules.clone()));
        assert_eq!(
            rows(&closure, "counted"),
            vec![vec![constant(3)]],
            "engine={engine}"
        );
        assert_eq!(
            rows(&closure, "minimum"),
            vec![vec![constant(1)]],
            "engine={engine}"
        );
        assert_eq!(
            rows(&closure, "maximum"),
            vec![vec![constant(2)]],
            "engine={engine}"
        );
        assert_eq!(
            rows(&closure, "empty_str"),
            vec![vec![json!({"f":"const","args":[{"s":""}]})]],
            "engine={engine}"
        );
        assert_eq!(
            rows(&closure, "joined_str"),
            vec![vec![json!({"f":"const","args":[{"s":"écho"}]})]],
            "engine={engine}"
        );
        assert_eq!(
            rows(&closure, "split_str"),
            vec![vec![
                json!({"f":"const","args":[{"s":"é"}]}),
                json!({"f":"const","args":[{"s":"cho"}]})
            ]],
            "engine={engine}"
        );
        assert!(
            rows(&closure, "split_empty_str").is_empty(),
            "engine={engine}"
        );
        assert!(rows(&closure, "non_integer").is_empty(), "engine={engine}");
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
            let (closure, _, trace) = run(7, size, engine, program(seeds, rules));
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
            let (closure, _, _) = run(8, size, engine, case);
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
            let (closure, _, _) = run(9, size, engine, program(seeds, rules));
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

#[test]
fn rung_9_ir_reuses_one_comptime_install() {
    let source = "fixtures/comptime_effect/0_fs_json.dl7";
    let compile = |engine: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .args(["compile", "--trace", source])
            .env("DL8_ENGINE", engine)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{engine}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        (
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    };
    let (fresh, _) = compile("rust");
    for engine in ["ir:dd", "ir:sqlite"] {
        let (retained, trace) = compile(engine);
        assert_eq!(retained["diagnostics"], json!([]), "{engine}");
        assert_eq!(
            retained["compiler_rows"], fresh["compiler_rows"],
            "{engine}: closure"
        );
        assert_eq!(
            trace
                .lines()
                .filter(|line| line.contains("comptime") && line.contains("Round(Evaluate"))
                .count(),
            2,
            "{engine}: rounds"
        );
        assert_eq!(
            trace
                .lines()
                .filter(|line| line.contains("comptime") && line.contains("ir install"))
                .count(),
            1,
            "{engine}: installs"
        );
    }
}

#[test]
fn rung_10_continuing_executors() {
    super::_11_watchers::check_fs_watch_edits_retract_requests_and_increment_only_dependents();
}

#[test]
fn rung_11_handwritten_oracle_constructs() {
    // These are the program inputs of the hand-written eval cases. The original
    // expected outputs stay in oracle/eval until the deletion commit. Each
    // program runs with zero, one, and all of its original input rows.
    let cases: [(&str, usize, &[(&str, usize)]); 12] = [
        ("0_negation_cons_intern", 6, &[("upper", 1)]),
        ("1_count", 7, &[("region_count", 2), ("big_region", 1)]),
        (
            "2_cons_lists",
            21,
            &[("suffix", 4), ("item", 3), ("pair", 9)],
        ),
        (
            "3_int_compare",
            23,
            &[
                ("less", 3),
                ("not_less", 6),
                ("same", 3),
                ("ge_two", 2),
                ("ne_two", 2),
                ("le_two", 2),
                ("text_never", 0),
            ],
        ),
        (
            "4_mixed_lower_recursion",
            17,
            &[("open", 3), ("reach", 3), ("unreached", 1)],
        ),
        (
            "5_edge_ref",
            10,
            &[("edge_of", 2), ("tagged", 2), ("literal_vars", 0)],
        ),
        ("6_nonground_negation", 5, &[("free", 1), ("lonely", 0)]),
        (
            "7_repeated_vars_and_arity",
            14,
            &[("loop", 1), ("triple", 4), ("fact", 1), ("pinned", 2)],
        ),
        ("8_deep_terms_order", 31, &[("seen", 15)]),
        (
            "9_demand_recursion",
            19,
            &[("chain", 7), ("reaches_end", 1), ("wrap_twice", 2)],
        ),
        (
            "10_aggregate_tables_only",
            9,
            &[
                ("nil_count", 1),
                ("cons_count", 0),
                ("item_count", 1),
                ("pair_count", 3),
            ],
        ),
        ("11_intern_row_reuse", 9, &[("made", 2), ("who_made", 2)]),
    ];
    for (stem, full_closure, expected_relations) in cases {
        let path = format!("{}/fixtures/ladder/{stem}.json", env!("CARGO_MANIFEST_DIR"));
        let source: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let expected_path = format!(
            "{}/fixtures/ladder/{stem}.expected.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let frozen: Value = serde_json::from_slice(&std::fs::read(expected_path).unwrap()).unwrap();
        assert_eq!(source, frozen["program"], "case={stem} program bytes");
        let seeds = source["seeds"].as_array().unwrap();
        let sizes = if stem == "6_nonground_negation" {
            [0, seeds.len(), seeds.len()]
        } else {
            [0, 1.min(seeds.len()), seeds.len()]
        };
        for size in sizes.into_iter().collect::<std::collections::BTreeSet<_>>() {
            let mut case = source.clone();
            case["seeds"] = Value::Array(seeds[..size].to_vec());
            let mut reference = None;
            for engine in ENGINES {
                let (closure, _, _) = run(11, size, engine, case.clone());
                if let Some(want) = &reference {
                    assert_eq!(&closure, want, "case={stem} size={size} engine={engine}");
                } else {
                    reference = Some(closure.clone());
                }
                if size == seeds.len() {
                    assert_eq!(
                        Value::Array(closure.clone()),
                        frozen["expected"]["closure"],
                        "case={stem} size={size} engine={engine} exact closure"
                    );
                    check_count(11, size, engine, stem, closure.len(), full_closure);
                    for (relation, want) in expected_relations {
                        check_count(
                            11,
                            size,
                            engine,
                            relation,
                            rows(&closure, relation).len(),
                            *want,
                        );
                    }
                }
            }
        }
        if stem == "6_nonground_negation" {
            let mut grown = source.clone();
            grown["seeds"].as_array_mut().unwrap().extend([
                call("source", vec![json!({"f":"const","args":[{"a":"three"}]})]),
                call("taken", vec![json!({"f":"const","args":[{"a":"three"}]})]),
            ]);
            let mut reference = None;
            for engine in ENGINES {
                let (closure, _, _) = run(11, 5, engine, grown.clone());
                if let Some(want) = &reference {
                    assert_eq!(&closure, want, "case={stem} size=5 engine={engine}");
                } else {
                    reference = Some(closure);
                }
            }
        }
    }
}

#[test]
fn rung_5_nonlinear_recursion() {
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
                        goal("path", vec![variable(1), variable(2)], false),
                    ],
                ),
            ];
            let (closure, _, _) = run(5, size, engine, program(seeds, rules));
            check_count(
                5,
                size,
                engine,
                "nonlinear_path_rows",
                rows(&closure, "path").len(),
                size * (size + 1) / 2,
            );
        }
    }
}
