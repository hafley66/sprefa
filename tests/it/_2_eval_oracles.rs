use dl8::_6_eval::ir_eval::{evaluate_ir, Backend, IrSession};
use dl8::_6_eval::json::closure_to_json;
use dl8::_6_eval::json::program_from_json;
use dl8::_6_eval::Universe;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::Config;
use proptest::test_runner::RngAlgorithm;
use proptest::test_runner::TestRng;
use proptest::test_runner::TestRunner;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

// From tests/_0_eval_oracle.rs
// Oracle parity through the real binary. Every `oracle/eval/*.json` holds a
// program and the closure v7 `evaluate/4` produced for it, frozen by
// `oracle/eval/dump_eval.pl`. `dl8 eval` must print the same closure and the
// same diagnostics.

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/eval");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    out.sort();
    out
}

#[test]
fn every_oracle_fixture_matches_v7() {
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no oracle fixtures found");
    let mut failures = Vec::new();
    for path in &fixtures {
        let text = std::fs::read_to_string(path).unwrap();
        let fixture: serde_json::Value = serde_json::from_str(&text).unwrap();
        let program_path = std::env::temp_dir().join(format!(
            "dl8-oracle-{}-{}.json",
            std::process::id(),
            path.file_stem().unwrap().to_string_lossy()
        ));
        std::fs::write(
            &program_path,
            serde_json::to_string(&fixture["program"]).unwrap(),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("eval")
            .arg(&program_path)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&program_path);
        let got: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "{}: no JSON on stdout ({e}); stderr: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
        };
        let want = &fixture["expected"];
        if got["closure"] != want["closure"] {
            failures.push(format!(
                "{}: closure differs\n  want: {}\n  got:  {}",
                path.display(),
                want["closure"],
                got["closure"]
            ));
        }
        if got["diagnostics"] != want["diagnostics"] {
            failures.push(format!(
                "{}: diagnostics differ\n  want: {}\n  got:  {}",
                path.display(),
                want["diagnostics"],
                got["diagnostics"]
            ));
        }
        let want_code = if want["diagnostics"].as_array().is_none_or(|d| d.is_empty()) {
            0
        } else {
            1
        };
        if output.status.code() != Some(want_code) {
            failures.push(format!(
                "{}: exit {:?}, want {want_code}",
                path.display(),
                output.status.code()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn ir_engines_match_v7_oracles() {
    let mut failures = Vec::new();
    let known_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("plans/v8/receipts/2026-09-26-dd-eval-differential/known-gaps.json");
    let known: Value = serde_json::from_slice(&std::fs::read(known_path).unwrap()).unwrap();
    let gaps = known["oracle_gaps"].as_array().unwrap();
    let mut reproduced = std::collections::HashSet::new();
    for path in fixtures() {
        let fixture: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let program_path = std::env::temp_dir().join(format!(
            "dl8-ir-oracle-{}-{}.json",
            std::process::id(),
            path.file_stem().unwrap().to_string_lossy()
        ));
        std::fs::write(
            &program_path,
            serde_json::to_vec(&fixture["program"]).unwrap(),
        )
        .unwrap();
        for engine in ["ir:dd"] {
            let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
                .arg("eval")
                .arg(&program_path)
                .env("DL8_ENGINE", engine)
                .output()
                .unwrap();
            let got: Value = match serde_json::from_slice(&output.stdout) {
                Ok(value) => value,
                Err(error) => {
                    failures.push(format!(
                        "{} {engine}: {error}: {}",
                        path.display(),
                        String::from_utf8_lossy(&output.stderr)
                    ));
                    continue;
                }
            };
            let fixture_name = path.file_name().unwrap().to_string_lossy();
            if let Some(gap) = gaps
                .iter()
                .find(|gap| gap["fixture"] == fixture_name.as_ref() && gap["engine"] == engine)
            {
                assert_eq!(
                    gap["program"], fixture["program"],
                    "stale oracle gap program: {fixture_name}"
                );
                let error = gap["error"].as_str().unwrap();
                let install_error = format!("Install rel None: Unsupported(\"{error}\")");
                assert!(
                    got["diagnostics"]
                        .as_array()
                        .is_some_and(|diagnostics| diagnostics.iter().any(|diagnostic| {
                            diagnostic["phase"] == "ir_eval"
                                && diagnostic["payload"]["f"] == "ir_engine_error"
                                && diagnostic["payload"]["args"][1]["s"]
                                    .as_str()
                                    .is_some_and(|detail| detail.contains(&install_error))
                        })),
                    "oracle gap stopped reproducing: {fixture_name} {engine}: {error}"
                );
                reproduced.insert((fixture_name.to_string(), engine.to_string()));
                continue;
            }
            if got["closure"] != fixture["expected"]["closure"]
                || got["diagnostics"] != fixture["expected"]["diagnostics"]
            {
                failures.push(format!(
                    "{} {engine}: expected {} rows, got {} rows; diagnostics {}",
                    path.display(),
                    fixture["expected"]["closure"]
                        .as_array()
                        .map_or(0, Vec::len),
                    got["closure"].as_array().map_or(0, Vec::len),
                    got["diagnostics"]
                ));
            }
        }
        let _ = std::fs::remove_file(program_path);
    }
    assert_eq!(
        reproduced.len(),
        gaps.len(),
        "listed oracle gap stopped reproducing"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}


// From tests/_29_eval_random.rs
// Seeded structural programs compared in one process across the two engines.
// `DL8_RANDOM_CATEGORY` selects one construct or aggregate kind for replay.

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

pub(crate) fn rel(name: &str) -> Value {
    let inner = match name {
        "tuple" | "intern" => json!({"f":"kernel","args":[{"a":name}]}),
        "int.add" => json!({"f":"kernel","args":[{"a":"int"},{"a":"add"}]}),
        _ => json!({"a":name}),
    };
    json!({"f":"ref","args":[inner]})
}

pub(crate) fn constant(value: i8) -> Value {
    json!({"f":"const","args":[value]})
}

pub(crate) fn variable(index: usize) -> Value {
    json!({"v":{"a":format!("v{index}")}})
}

pub(crate) fn call(name: &str, args: Vec<Value>) -> Value {
    json!({"rel":rel(name),"args":args})
}

pub(crate) fn goal(name: &str, args: Vec<Value>, negative: bool) -> Value {
    json!({"rel":rel(name),"args":args,"polarity":if negative {"negative"} else {"positive"}})
}

pub(crate) fn rule(name: &str, head: Vec<Value>, body: Vec<Value>) -> Value {
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
                vec![goal("tuple", vec![constant(1), variable(1)], false)],
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


/// One closure per seed set: a fresh install each step, or one session per
/// engine that receives each step's seed change as retractions and inserts.
fn closures_for_seed_sequence(
    program: &Value,
    sequence: &[Vec<usize>],
    backend: Backend,
    cached: bool,
) -> Vec<Value> {
    let mut universe = Universe::new();
    let mut session = IrSession::new(backend);
    sequence
        .iter()
        .map(|seed_indices| {
            let mut program = program_from_json(&mut universe, program).unwrap();
            let all_seeds = program.seeds.clone();
            program.seeds = seed_indices
                .iter()
                .map(|&index| all_seeds[index].clone())
                .collect();
            let closure = if cached {
                session.evaluate(&mut universe, &program, None, &mut |_| {})
            } else {
                evaluate_ir(&mut universe, &program, backend, &mut |_| {})
            };
            canonical(closure_to_json(
                &universe,
                &closure.rows,
                &closure.diagnostics,
            ))
        })
        .collect()
}

fn seed_sequence(seed: u64, seed_count: usize) -> Vec<Vec<usize>> {
    let step_count = 3 + (seed as usize % 4);
    let mut present = vec![true; seed_count];
    let mut sequence = Vec::with_capacity(step_count);
    for step in 0..step_count {
        if step > 0 {
            let index = (seed as usize).wrapping_add((step - 1) / 2) % seed_count;
            present[index] = step % 2 == 0;
        }
        sequence.push(
            present
                .iter()
                .enumerate()
                .filter_map(|(index, &included)| included.then_some(index))
                .collect(),
        );
    }
    sequence
}

fn runner(seed: u64) -> TestRunner {
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
    TestRunner::new_with_rng(config, TestRng::from_seed(RngAlgorithm::ChaCha, &bytes))
}

#[test]
fn random_seed_retractions_match_fresh_installs() {
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
    let mut seed_sets = 0usize;
    for seed in seeds {
        let generated = strategy();
        let spec = generated.new_tree(&mut runner(seed)).unwrap().current();
        let program = program_json(seed, &spec);
        let seed_count = program["seeds"].as_array().unwrap().len();
        let sequence = seed_sequence(seed, seed_count);
        seed_sets += sequence.len();

        let fresh = closures_for_seed_sequence(&program, &sequence, Backend::Dd, false);
        let cached = closures_for_seed_sequence(&program, &sequence, Backend::Dd, true);
        for (step, indices) in sequence.iter().enumerate() {
            assert_eq!(
                cached[step], fresh[step],
                "seed {seed} step {step} indices={indices:?}: the cached session differs from a fresh install"
            );
        }
    }
    println!(
        "random incremental seed_sets={seed_sets} wall_ms={}",
        started.elapsed().as_millis()
    );
}
