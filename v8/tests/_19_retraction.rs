//! Key as replacement identity, through the real binary. A `(Key Name Options)`
//! column makes a later row with equal key cells replace the stored one, and a
//! stratum whose input lost a row is cleared and derived again.
//!
//! `dl8 compile` carries the key positions in `program.relations`; `dl8 eval`
//! and `dl8 run --serve timer` read them. The per-tick cases drive the same
//! compiled program through `Evaluate` in this process, one outside row per
//! tick, because `dl8 run` prints only the last tick's closure.
//!
//! Fail-first receipts, each measured: `register_keys` returning at entry fails
//! the four keyed cases and passes the two unkeyed ones; `input_lost` returning
//! `false` fails every case but the seed replacement, which derives nothing
//! before its seeds settle.

use dl8::_6_eval::evaluate::{Evaluate, Store};
use dl8::_6_eval::json::{program_from_json, program_names, term_to_json};
use dl8::_6_eval::{Program, Row, TermId, Trace, Universe};
use dl8::_7_effect::Slice;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const RUN_CAP: Duration = Duration::from_secs(10);

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/retraction")
        .join(name)
}

fn scratch(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("dl8-retraction-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// `dl8 compile` stdout, written to `<directory>/<stem>.json`.
fn compile(source: &str, directory: &Path) -> (PathBuf, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(fixture(source))
        .output()
        .unwrap();
    let compiled: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "compile no JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(compiled["diagnostics"], json!([]), "compile diagnostics");
    let program = directory.join(source.replace(".dl7", ".json"));
    std::fs::write(&program, &output.stdout).unwrap();
    (program, compiled["program"].clone())
}

/// Runs `dl8 <args>`, killed past `RUN_CAP`; stdout JSON and exit code.
fn dl8(args: &[&str]) -> (Value, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > RUN_CAP {
            let _ = child.kill();
            panic!("dl8 {args:?} exceeded {RUN_CAP:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut stdout = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    let value = serde_json::from_slice(&stdout)
        .unwrap_or_else(|e| panic!("dl8 {args:?} no JSON ({e}); stderr: {stderr}"));
    (value, status.code().unwrap_or(-1))
}

/// A `const(X)` cell as plain JSON: the string text, or the number.
fn cell(value: &Value) -> Value {
    match value["args"][0].get("s") {
        Some(text) => text.clone(),
        None => value["args"][0].clone(),
    }
}

/// The named relation's rows, sorted, from a closure printed as JSON.
fn rows(out: &Value, program: &Value, name: &str) -> Vec<Vec<Value>> {
    let relation = &program["names"][name];
    assert!(!relation.is_null(), "{name} missing from names");
    let mut found: Vec<Vec<Value>> = out["closure"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| &row["rel"] == relation)
        .map(|row| row["args"].as_array().unwrap().iter().map(cell).collect())
        .collect();
    found.sort_by_key(|row| row.iter().map(|v| v.to_string()).collect::<Vec<_>>());
    found
}

#[test]
pub fn a_later_keyed_seed_replaces_the_earlier_one() {
    let directory = scratch("seed");
    let (program, compiled) = compile("0_seed_replacement.dl7", &directory);
    assert_eq!(compiled["relations"][0]["keys"], json!([0]));
    let (out, code) = dl8(&["eval", program.to_str().unwrap()]);
    assert_eq!(code, 0, "diagnostics {}", out["diagnostics"]);
    let current = vec![
        vec![json!("cli"), json!("v2")],
        vec![json!("gh"), json!("v1")],
    ];
    assert_eq!(rows(&out, &compiled, "Watch"), current);
    assert_eq!(rows(&out, &compiled, "Tagged"), current);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
pub fn a_keyed_timer_fire_retracts_what_the_previous_fire_proved() {
    let directory = scratch("latch");
    let (program, compiled) = compile("1_latch.dl7", &directory);
    let (out, code) = dl8(&[
        "run",
        program.to_str().unwrap(),
        "--serve",
        "timer",
        "--max-ticks",
        "3",
    ]);
    assert_eq!(code, 0, "diagnostics {}", out["diagnostics"]);
    assert_eq!(out["ticks"], json!(3));
    assert_eq!(
        rows(&out, &compiled, "timer"),
        vec![vec![json!(1), json!(3)]]
    );
    assert_eq!(rows(&out, &compiled, "Fired"), vec![vec![json!(3)]]);
    assert_eq!(rows(&out, &compiled, "EvenFire"), Vec::<Vec<Value>>::new());
    assert_eq!(
        rows(&out, &compiled, "Latch"),
        vec![vec![json!(0), json!(2)]],
        "the keyed latch survives EvenFire emptying"
    );
    assert_eq!(rows(&out, &compiled, "Parity").len(), 4, "an unkeyed seed");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
pub fn an_aggregate_over_a_keyed_input_holds_one_row() {
    let directory = scratch("aggregate");
    let (program, compiled) = compile("2_aggregate.dl7", &directory);
    let (out, code) = dl8(&[
        "run",
        program.to_str().unwrap(),
        "--serve",
        "timer",
        "--max-ticks",
        "3",
    ]);
    assert_eq!(code, 0, "diagnostics {}", out["diagnostics"]);
    assert_eq!(rows(&out, &compiled, "FiredCount"), vec![vec![json!(1)]]);
    assert_eq!(rows(&out, &compiled, "TickSum"), vec![vec![json!(3)]]);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
pub fn an_unkeyed_timer_accumulates_and_its_count_holds_one_row() {
    let directory = scratch("unkeyed");
    let (program, compiled) = compile("3_unkeyed.dl7", &directory);
    assert_eq!(compiled["relations"], json!([]));
    let (out, code) = dl8(&[
        "run",
        program.to_str().unwrap(),
        "--serve",
        "timer",
        "--max-ticks",
        "3",
    ]);
    assert_eq!(code, 0, "diagnostics {}", out["diagnostics"]);
    assert_eq!(rows(&out, &compiled, "timer").len(), 3);
    assert_eq!(
        rows(&out, &compiled, "Fired"),
        vec![vec![json!(1)], vec![json!(2)], vec![json!(3)]]
    );
    assert_eq!(rows(&out, &compiled, "FiredCount"), vec![vec![json!(3)]]);
    let _ = std::fs::remove_dir_all(&directory);
}

/// One compiled program evaluated tick by tick against one store.
struct Ticks {
    u: Universe,
    program: Program,
    names: HashMap<String, TermId>,
    store: Store,
    retracts: Vec<Trace>,
}

impl Ticks {
    fn new(source: &str) -> Ticks {
        let directory = scratch(&format!("ticks-{source}"));
        let (_, compiled) = compile(source, &directory);
        let _ = std::fs::remove_dir_all(&directory);
        let mut u = Universe::new();
        let program = program_from_json(&mut u, &compiled).unwrap();
        let names = program_names(&mut u, &compiled).unwrap();
        let mut ticks = Ticks {
            u,
            program,
            names,
            store: Store::default(),
            retracts: Vec::new(),
        };
        ticks.evaluate();
        ticks
    }

    fn evaluate(&mut self) {
        self.store.mark_all();
        let retracts = &mut self.retracts;
        let closure = Evaluate::reduce(
            &mut self.store,
            (&mut self.u, &self.program),
            &mut |trace| {
                if matches!(trace, Trace::Retract { .. }) {
                    retracts.push(trace);
                }
            },
        );
        assert!(closure.diagnostics.is_empty(), "{:?}", closure.diagnostics);
        self.assert_one_row_per_key();
    }

    /// `timer(Period, Tick)` from outside, then one evaluation.
    fn fire(&mut self, period: i64, tick: i64) {
        let rel = self.names["timer"];
        let row = Row {
            rel,
            args: [period, tick]
                .iter()
                .map(|n| {
                    let n = self.u.int(*n);
                    self.u.compound("const", vec![n])
                })
                .collect(),
        };
        self.store.insert(row.rel, row.args.into_boxed_slice());
        self.evaluate();
    }

    fn rows(&self, name: &str) -> Vec<Vec<Value>> {
        let Some(table) = self.store.table(self.names[name]) else {
            return Vec::new();
        };
        let mut out: Vec<Vec<Value>> = table
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|c| cell(&term_to_json(&self.u, *c)))
                    .collect()
            })
            .collect();
        out.sort_by_key(|row| row.iter().map(|v| v.to_string()).collect::<Vec<_>>());
        out
    }

    fn assert_one_row_per_key(&self) {
        for relation in &self.program.relations {
            let Some(table) = self.store.table(relation.rel) else {
                continue;
            };
            let mut seen: HashSet<Vec<TermId>> = HashSet::new();
            for row in table.rows.iter() {
                let key: Vec<TermId> = relation.keys.iter().map(|&k| row[k]).collect();
                assert!(seen.insert(key), "two rows share a key: {row:?}");
            }
        }
    }
}

/// Rows per relation name at one tick.
type Snapshot = BTreeMap<&'static str, Vec<Vec<Value>>>;

/// `v6/tsv2/goldens/ghcacher_tick_golden/README.md` rows 3 to 5 in miniature:
/// replacement retracts the old fire's rows, the keyed latch survives, then
/// replaces, and a row under a fresh key rewrites nothing downstream.
#[test]
pub fn each_tick_holds_one_row_per_key_and_the_latch_follows_the_golden() {
    let mut ticks = Ticks::new("1_latch.dl7");
    let mut table: Vec<(i64, Snapshot)> = Vec::new();
    let mut capture = |tick: i64, ticks: &Ticks| {
        let names = ["timer", "Fired", "EvenFire", "Latch"];
        table.push((tick, names.iter().map(|n| (*n, ticks.rows(n))).collect()));
    };
    capture(0, &ticks);
    for tick in 1..=4 {
        ticks.fire(1, tick);
        capture(tick, &ticks);
    }
    let removed = ticks.store.removed;
    let written = ticks.store.written;
    ticks.fire(2, 9);
    capture(5, &ticks);

    let row = |cells: &[i64]| cells.iter().map(|n| json!(n)).collect::<Vec<Value>>();
    let expected: Vec<(i64, [Vec<Vec<Value>>; 4])> = vec![
        (0, [vec![], vec![], vec![], vec![]]),
        (1, [vec![row(&[1, 1])], vec![row(&[1])], vec![], vec![]]),
        (
            2,
            [
                vec![row(&[1, 2])],
                vec![row(&[2])],
                vec![row(&[2])],
                vec![row(&[0, 2])],
            ],
        ),
        (
            3,
            [
                vec![row(&[1, 3])],
                vec![row(&[3])],
                vec![],
                vec![row(&[0, 2])],
            ],
        ),
        (
            4,
            [
                vec![row(&[1, 4])],
                vec![row(&[4])],
                vec![row(&[4])],
                vec![row(&[0, 4])],
            ],
        ),
        (
            5,
            [
                vec![row(&[1, 4]), row(&[2, 9])],
                vec![row(&[4])],
                vec![row(&[4])],
                vec![row(&[0, 4])],
            ],
        ),
    ];
    for ((tick, got), (want_tick, want)) in table.iter().zip(expected.iter()) {
        assert_eq!(tick, want_tick);
        for (name, want_rows) in ["timer", "Fired", "EvenFire", "Latch"].iter().zip(want) {
            assert_eq!(&got[name], want_rows, "tick {tick} {name}");
        }
    }
    assert_eq!(
        ticks.store.removed, removed,
        "a fresh key removed a row downstream"
    );
    assert_eq!(
        ticks.store.written,
        written + 1,
        "only the timer row is new"
    );
}

#[test]
pub fn an_unkeyed_positive_read_is_never_cleared() {
    let mut ticks = Ticks::new("3_unkeyed.dl7");
    for tick in 1..=3 {
        ticks.fire(1, tick);
    }
    assert_eq!(ticks.rows("Fired").len(), 3);
    assert_eq!(ticks.rows("FiredCount"), vec![vec![json!(3)]]);
    let cleared: Vec<(usize, usize)> = ticks
        .retracts
        .iter()
        .filter_map(|trace| match trace {
            Trace::Retract {
                relations, rows, ..
            } => Some((*relations, *rows)),
            _ => None,
        })
        .collect();
    assert!(
        cleared.iter().all(|(relations, _)| *relations == 1),
        "a stratum other than FiredCount's was cleared: {cleared:?}"
    );
    let rows: usize = cleared.iter().map(|(_, rows)| rows).sum();
    assert_eq!(rows, 2, "FiredCount (1) and (2) are the only rows cleared");
    assert_eq!(ticks.store.removed, 2);
}
