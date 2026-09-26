//! Seeded structural programs compared in one process across the three evaluators.

use dl8::_6_eval::json::{closure_to_json, program_from_json};
use dl8::_6_eval::{evaluate, evaluate_dd, evaluate_sqlite, Universe};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestCaseError, TestError, TestRng, TestRunner};
use serde_json::{json, Value};
use std::time::Instant;

type SeedSpec = (u8, Vec<i8>);
type GoalSpec = (u8, Vec<u8>);
type RuleSpec = (u8, Vec<u8>, Vec<GoalSpec>);
type Spec = (Vec<SeedSpec>, Vec<RuleSpec>);

fn strategy() -> impl Strategy<Value = Spec> {
    let args = proptest::collection::vec(0u8..5, 0..4);
    let goals = proptest::collection::vec((0u8..4, args.clone()), 0..3);
    (
        proptest::collection::vec((0u8..12, proptest::collection::vec(-2i8..=2, 0..4)), 0..5),
        proptest::collection::vec((0u8..4, args, goals), 0..4),
    )
}

fn rel(name: &str) -> Value {
    let inner = match name {
        "nil" | "cons" | "intern" => json!({"f":"kernel","args":[{"a":name}]}),
        "int.add" => json!({"f":"kernel","args":[{"a":"int"},{"a":"add"}]}),
        _ => json!({"a":name}),
    };
    json!({"f":"ref","args":[inner]})
}

fn constant(value: i8) -> Value {
    json!({"f":"const","args":[value]})
}

fn variable(index: usize) -> Value {
    json!({"v":{"a":format!("v{index}")}})
}

fn call(name: &str, args: Vec<Value>) -> Value {
    json!({"rel":rel(name),"args":args})
}

fn goal(name: &str, args: Vec<Value>, negative: bool) -> Value {
    json!({"rel":rel(name),"args":args,"polarity":if negative {"negative"} else {"positive"}})
}

fn rule(name: &str, head: Vec<Value>, body: Vec<Value>) -> Value {
    json!({"head":call(name,head),"body":body})
}

fn vars(width: usize) -> Vec<Value> {
    (0..width).map(variable).collect()
}

fn program_json(seed: u64, spec: &Spec) -> Value {
    let mut seeds = vec![call("s0", vec![])];
    let mut rules = Vec::new();
    match seed % 8 {
        0 => {
            seeds.push(call("s1", vec![constant(1)]));
            rules.push(rule("d1", vars(1), vec![goal("s1", vars(1), false)]));
        }
        1 => {
            seeds.push(call("s2", vec![constant(0), constant(1)]));
            seeds.push(call("s2", vec![constant(1), constant(2)]));
            rules.push(rule("d2", vars(2), vec![goal("s2", vars(2), false)]));
            rules.push(rule(
                "d2",
                vec![variable(0), variable(2)],
                vec![
                    goal("d2", vec![variable(0), variable(1)], false),
                    goal("s2", vec![variable(1), variable(2)], false),
                ],
            ));
        }
        2 => {
            seeds.push(call("s1", vec![constant(0)]));
            seeds.push(call("b1", vec![constant(1)]));
            rules.push(rule(
                "d1",
                vars(1),
                vec![goal("s1", vars(1), false), goal("b1", vars(1), true)],
            ));
        }
        3 => {
            seeds.push(call("s2", vec![constant(1), constant(1)]));
            rules.push(rule(
                "d1",
                vars(1),
                vec![goal("s2", vec![variable(0), variable(0)], false)],
            ));
        }
        4 => {
            seeds.push(call("s1", vec![constant(1)]));
            seeds.push(call("s1", vec![constant(2)]));
            let kind = ["count", "sum", "min", "max"][((seed / 8) % 4) as usize];
            rules.push(rule(
                "agg1",
                vec![json!({kind:variable(0)})],
                vec![goal("s1", vars(1), false)],
            ));
        }
        5 => {
            rules.push(rule(
                "d1",
                vec![variable(1)],
                vec![
                    goal("nil", vec![variable(0)], false),
                    goal("cons", vec![constant(1), variable(0), variable(1)], false),
                ],
            ));
        }
        6 => {
            seeds.push(call("s2", vec![constant(1), constant(2)]));
            rules.push(rule(
                "d1",
                vec![variable(2)],
                vec![
                    goal("s2", vec![variable(0), variable(1)], false),
                    goal(
                        "int.add",
                        vec![variable(0), variable(1), variable(2)],
                        false,
                    ),
                ],
            ));
        }
        _ => {
            let ctor = rel("ctor");
            let arguments = json!({"f":"const","args":[[constant(1)]]});
            rules.push(rule(
                "d1",
                vec![variable(0)],
                vec![goal("intern", vec![ctor, arguments, variable(0)], false)],
            ));
        }
    }
    for (id, cells) in &spec.0 {
        let width = (id % 4) as usize;
        let prefix = match id / 4 {
            0 => "s",
            1 => "b",
            _ => "d",
        };
        let args = (0..width)
            .map(|i| constant(*cells.get(i).unwrap_or(&0)))
            .collect();
        seeds.push(call(&format!("{prefix}{width}"), args));
    }
    for (width, head_spec, extra) in &spec.1 {
        let width = *width as usize;
        let name = format!("d{width}");
        let source = format!("s{width}");
        let mut body = vec![goal(&source, vars(width), false)];
        for (kind, arg_spec) in extra {
            let relation = match kind {
                0 => source.clone(),
                1 | 3 => name.clone(),
                _ => format!("b{width}"),
            };
            let args = (0..width)
                .map(|i| variable((*arg_spec.get(i).unwrap_or(&0) as usize) % width.max(1)))
                .collect();
            body.push(goal(&relation, args, *kind >= 2));
        }
        let head = (0..width)
            .map(|i| {
                let choice = *head_spec.get(i).unwrap_or(&(i as u8));
                if choice == 4 {
                    constant(0)
                } else {
                    variable((choice as usize) % width.max(1))
                }
            })
            .collect();
        rules.push(rule(&name, head, body));
    }
    json!({"rules":rules,"seeds":seeds})
}

fn canonical(mut closure: Value) -> Value {
    for field in ["closure", "diagnostics"] {
        let rows = closure[field].as_array_mut().unwrap();
        rows.sort_by_key(Value::to_string);
    }
    closure
}

fn closures(program: &Value) -> [Value; 3] {
    let run = |engine: usize| {
        let mut universe = Universe::new();
        let program = program_from_json(&mut universe, program).unwrap();
        let closure = match engine {
            0 => evaluate(&mut universe, &program, &mut |_| {}),
            1 => evaluate_sqlite(&mut universe, &program, &mut |_| {}),
            _ => evaluate_dd(&mut universe, &program, &mut |_| {}),
        };
        canonical(closure_to_json(
            &universe,
            &closure.rows,
            &closure.diagnostics,
        ))
    };
    [run(0), run(1), run(2)]
}

fn differs(program: &Value) -> bool {
    let [rust, sqlite, dd] = closures(program);
    rust != sqlite || rust != dd
}

fn minimize(mut program: Value) -> Value {
    for _ in 0..256 {
        let mut next = None;
        for section in ["rules", "seeds"] {
            for i in 0..program[section].as_array().unwrap().len() {
                let mut candidate = program.clone();
                candidate[section].as_array_mut().unwrap().remove(i);
                if differs(&candidate) {
                    next = Some(candidate);
                    break;
                }
            }
            if next.is_some() {
                break;
            }
        }
        if next.is_none() {
            for i in 0..program["rules"].as_array().unwrap().len() {
                let n = program["rules"][i]["body"].as_array().unwrap().len();
                for j in 0..n {
                    let mut candidate = program.clone();
                    candidate["rules"][i]["body"]
                        .as_array_mut()
                        .unwrap()
                        .remove(j);
                    if differs(&candidate) {
                        next = Some(candidate);
                        break;
                    }
                }
                if next.is_some() {
                    break;
                }
            }
        }
        if next.is_none() {
            for section in ["rules", "seeds"] {
                for i in 0..program[section].as_array().unwrap().len() {
                    let paths: Vec<Vec<&str>> = if section == "seeds" {
                        vec![vec!["args"]]
                    } else {
                        vec![vec!["head", "args"]]
                    };
                    if section == "rules" {
                        for j in 0..program["rules"][i]["body"].as_array().unwrap().len() {
                            let len = program["rules"][i]["body"][j]["args"]
                                .as_array()
                                .unwrap()
                                .len();
                            for k in 0..len {
                                let mut reduced = program.clone();
                                reduced["rules"][i]["body"][j]["args"]
                                    .as_array_mut()
                                    .unwrap()
                                    .remove(k);
                                if differs(&reduced) {
                                    next = Some(reduced);
                                    break;
                                }
                            }
                            if next.is_some() {
                                break;
                            }
                        }
                        if next.is_some() {
                            break;
                        }
                    }
                    for path in paths {
                        let args = if path.len() == 1 {
                            &program[section][i][path[0]]
                        } else {
                            &program[section][i][path[0]][path[1]]
                        };
                        for j in 0..args.as_array().unwrap().len() {
                            let mut candidate = program.clone();
                            let args = if path.len() == 1 {
                                &mut candidate[section][i][path[0]]
                            } else {
                                &mut candidate[section][i][path[0]][path[1]]
                            };
                            args.as_array_mut().unwrap().remove(j);
                            if differs(&candidate) {
                                next = Some(candidate);
                                break;
                            }
                        }
                        if next.is_some() {
                            break;
                        }
                    }
                    if next.is_some() {
                        break;
                    }
                }
                if next.is_some() {
                    break;
                }
            }
        }
        match next {
            Some(candidate) => program = candidate,
            None => break,
        }
    }
    program
}

#[test]
fn random_programs_match_all_evaluators() {
    assert!(
        std::env::var_os("DL8_ENGINE").is_none(),
        "Rust path requires DL8_ENGINE unset"
    );
    let replay = std::env::var("DL8_RANDOM_SEED")
        .ok()
        .map(|n| n.parse::<u64>().expect("DL8_RANDOM_SEED must be u64"));
    let count = std::env::var("DL8_RANDOM_SEEDS")
        .ok()
        .map(|n| n.parse::<u64>().expect("DL8_RANDOM_SEEDS must be u64"))
        .unwrap_or(200);
    let seeds: Box<dyn Iterator<Item = u64>> = match replay {
        Some(seed) => Box::new(std::iter::once(seed)),
        None => Box::new(0..count),
    };
    let started = Instant::now();
    let mut coverage = [0usize; 8];
    for seed in seeds {
        coverage[(seed % 8) as usize] += 1;
        let mut bytes = [0u8; 32];
        for (i, chunk) in bytes.as_chunks_mut::<8>().0.iter_mut().enumerate() {
            chunk.copy_from_slice(&seed.wrapping_add(i as u64).to_le_bytes());
        }
        let config = Config {
            cases: 1,
            max_shrink_iters: 256,
            failure_persistence: None,
            ..Config::default()
        };
        let mut runner =
            TestRunner::new_with_rng(config, TestRng::from_seed(RngAlgorithm::ChaCha, &bytes));
        let result = runner.run(&strategy(), |spec| {
            let program = program_json(seed, &spec);
            if differs(&program) {
                Err(TestCaseError::fail("closure mismatch"))
            } else {
                Ok(())
            }
        });
        match result {
            Err(TestError::Fail(_, spec)) => {
                let program = minimize(program_json(seed, &spec));
                let [rust, sqlite, dd] = closures(&program);
                let receipt =
                    json!({"seed":seed,"program":program,"rust":rust,"sqlite":sqlite,"dd":dd});
                let path = format!(
                    "plans/v8/receipts/2026-09-26-dd-eval-differential/counterexample-{seed}.json"
                );
                std::fs::create_dir_all("plans/v8/receipts/2026-09-26-dd-eval-differential")
                    .unwrap();
                std::fs::write(&path, serde_json::to_string_pretty(&receipt).unwrap()).unwrap();
                panic!(
                    "seed {seed} mismatch; receipt {path}\n{}",
                    serde_json::to_string_pretty(&receipt).unwrap()
                );
            }
            Err(TestError::Abort(reason)) => panic!("seed {seed}: {reason}"),
            Ok(()) => {}
        }
    }
    println!(
        "random seeds={} coverage={coverage:?} wall_ms={}",
        coverage.iter().sum::<usize>(),
        started.elapsed().as_millis()
    );
}
