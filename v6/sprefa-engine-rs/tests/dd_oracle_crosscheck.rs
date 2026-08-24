//! The real differential-dataflow ecosystem runs a panel of conformance
//! programs and its per-tick output delta stream is diffed against the oracle's.
//! Correctness only; nothing here times anything.
//!
//! @comment-ok: TEST header carrying the sabotage receipt and the seam contract.
//!
//! ORACLE SIDE. `v6/prolog/conformance/dd_panel_export.pl` writes
//! `conformance/dd_panel.json`: per program, the post-seed row set, the signed
//! arrival schedule, the signed per-tick delta stream, and the finals. The file
//! is committed; regenerate with `swipl -q -l dd_panel_export.pl -g go -t halt`
//! and `-g check` fails on drift.
//!
//! DD SIDE. One hand-written circuit per program name, dispatched by
//! `build_circuit`. No generic datalog interpreter: each circuit spells its
//! fixture's rules in dd operators, so a lowering bug in this repo cannot hide
//! behind a shared evaluator that shares the bug.
//!
//! THE TIME MAP. dd time 0 carries the fixture's Initial seed, dd time i+1
//! carries schedule tick i. dd's time-0 batch is graded against `seed_state`
//! (every row weight +1) and time i+1 against `deltas[i]`. Sequence and
//! ordering columns are ours, not dd's, so each tick is compared as a MULTISET
//! of (row, weight), never as an ordered list.
//!
//! SABOTAGE RECEIPT, measured 2026-08-23. `Sabotage::NegateMirror` makes
//! `retraction_only_tick_retracts_level_view`'s `mirror` head emit
//! `.negate()`d, which flips the sign on both of its ticks and nothing else.
//! The comparator's verbatim output:
//!   tick 1: dd stream != oracle stream
//!     dd only    : -mirror(alpha) x2, -mirror(beta) x2
//!     oracle only: +mirror(alpha) x2, +mirror(beta) x2
//! x2, not x1: the reported figure is the one-sided weight difference, dd's -1
//! against the oracle's +1. `sabotage_flipped_sign_is_caught` pins that the diff
//! fires; without it a comparator reading row identity alone would call this
//! panel green.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use differential_dataflow::input::Input;
use differential_dataflow::operators::Iterate;
use differential_dataflow::VecCollection;
use serde::Deserialize;
use timely::worker::Worker;

// ═══ the wire types ═════════════════════════════════════════════════════════

/// A float column compares by its exact bit pattern, never by `PartialOrd`.
/// Two panel programs turn on that distinction: `float_exact_join_has_no_epsilon`
/// joins 0.3 against 0.30000000000000004, and the oracle keeps -0.0 apart from
/// 0.0. `ordered_float::OrderedFloat` orders through `partial_cmp`, which folds
/// the two zeroes together, so it is the wrong total order for this panel.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(transparent)]
struct Real(f64);

impl Real {
    fn bits(self) -> u64 {
        self.0.to_bits()
    }
}

impl PartialEq for Real {
    fn eq(&self, other: &Self) -> bool {
        self.bits() == other.bits()
    }
}
impl Eq for Real {}
impl PartialOrd for Real {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Real {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.bits().cmp(&other.bits())
    }
}
impl std::hash::Hash for Real {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bits().hash(state);
    }
}

impl serde::Serialize for Real {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, serde::Serialize)]
#[serde(tag = "t", content = "v")]
enum Value {
    #[serde(rename = "int")]
    Int(i64),
    #[serde(rename = "real")]
    Real(Real),
    #[serde(rename = "text")]
    Text(String),
}

impl Value {
    fn text(word: &str) -> Self {
        Value::Text(word.to_string())
    }
    fn real(number: f64) -> Self {
        Value::Real(Real(number))
    }
    fn as_int(&self) -> i64 {
        match self {
            Value::Int(number) => *number,
            other => panic!("expected an int column, got {other:?}"),
        }
    }
    fn as_real(&self) -> f64 {
        match self {
            Value::Real(number) => number.0,
            other => panic!("expected a real column, got {other:?}"),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(number) => write!(formatter, "{number}"),
            Value::Real(number) => write!(formatter, "{}", number.0),
            Value::Text(word) => write!(formatter, "{word}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, serde::Serialize)]
struct Row {
    rel: String,
    values: Vec<Value>,
}

impl Row {
    fn new(rel: &str, values: Vec<Value>) -> Self {
        Row {
            rel: rel.to_string(),
            values,
        }
    }
}

impl std::fmt::Display for Row {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let columns: Vec<String> = self.values.iter().map(|value| value.to_string()).collect();
        write!(formatter, "{}({})", self.rel, columns.join(","))
    }
}

#[derive(Clone, Debug, Deserialize)]
struct SignedRow {
    sign: i64,
    rel: String,
    values: Vec<Value>,
}

impl SignedRow {
    fn split(&self) -> (Row, isize) {
        (
            Row {
                rel: self.rel.clone(),
                values: self.values.clone(),
            },
            self.sign as isize,
        )
    }
}

#[derive(Debug, Deserialize)]
struct Program {
    name: String,
    note: String,
    seed_arrivals: Vec<Row>,
    seed_state: Vec<Row>,
    schedule: Vec<Vec<SignedRow>>,
    deltas: Vec<Vec<SignedRow>>,
    #[serde(rename = "final")]
    finals: Vec<Row>,
}

#[derive(Debug, Deserialize)]
struct Panel {
    programs: Vec<Program>,
}

fn load_panel() -> Panel {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../prolog/conformance/dd_panel.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

// ═══ the tick stream comparison ═════════════════════════════════════════════

/// A tick is a multiset of (row, weight). Consolidating into a map keyed by the
/// row is what makes the comparison order-free: dd may split one logical time
/// across several batches, and the oracle's list order is msort's, not a tick
/// order either side is entitled to read.
type Tick = BTreeMap<Row, isize>;

fn tick_from(updates: impl IntoIterator<Item = (Row, isize)>) -> Tick {
    let mut tick: Tick = BTreeMap::new();
    for (row, weight) in updates {
        *tick.entry(row).or_insert(0) += weight;
    }
    tick.retain(|_, weight| *weight != 0);
    tick
}

fn render(tick: &Tick) -> String {
    if tick.is_empty() {
        return "(empty)".to_string();
    }
    tick.iter()
        .map(|(row, weight)| {
            format!(
                "{}{row} x{}",
                if *weight < 0 { "-" } else { "+" },
                weight.abs()
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `Ok(())` when the two streams agree at every tick; otherwise the first
/// disagreeing tick with both one-sided differences spelled out.
fn compare_streams(dd: &[Tick], oracle: &[Tick]) -> Result<(), String> {
    if dd.len() != oracle.len() {
        return Err(format!(
            "tick count: dd {} vs oracle {}",
            dd.len(),
            oracle.len()
        ));
    }
    for (index, (dd_tick, oracle_tick)) in dd.iter().zip(oracle.iter()).enumerate() {
        if dd_tick == oracle_tick {
            continue;
        }
        let mut dd_only: Tick = BTreeMap::new();
        let mut oracle_only: Tick = BTreeMap::new();
        for (row, weight) in dd_tick {
            let difference = weight - oracle_tick.get(row).copied().unwrap_or(0);
            if difference != 0 {
                dd_only.insert(row.clone(), difference);
            }
        }
        for (row, weight) in oracle_tick {
            let difference = weight - dd_tick.get(row).copied().unwrap_or(0);
            if difference != 0 {
                oracle_only.insert(row.clone(), difference);
            }
        }
        return Err(format!(
            "tick {index}: dd stream != oracle stream\n  dd only    : {}\n  oracle only: {}",
            render(&dd_only),
            render(&oracle_only)
        ));
    }
    Ok(())
}

/// The oracle stream as dd sees it: time 0 is the seeded state at weight +1,
/// time i+1 is the fixture's delta tick i.
fn oracle_stream(program: &Program) -> Vec<Tick> {
    let mut stream = vec![tick_from(
        program.seed_state.iter().map(|row| (row.clone(), 1isize)),
    )];
    for delta in &program.deltas {
        stream.push(tick_from(delta.iter().map(|signed| signed.split())));
    }
    stream
}

/// Weights summed across the whole stream must land on the fixture's finals,
/// every row at exactly +1. A stream that agreed tick by tick but drifted here
/// would mean the comparison above was reading the wrong thing.
fn accumulate(stream: &[Tick]) -> Tick {
    let mut total: Tick = BTreeMap::new();
    for tick in stream {
        for (row, weight) in tick {
            *total.entry(row.clone()).or_insert(0) += weight;
        }
    }
    total.retain(|_, weight| *weight != 0);
    total
}

// ═══ the dd side ════════════════════════════════════════════════════════════

type Arrivals<'scope> = VecCollection<'scope, u64, Row, isize>;
type Derived<'scope> = VecCollection<'scope, u64, Row, isize>;

/// Only `retraction_only_tick_retracts_level_view` reads this, and only the
/// sabotage test passes anything but `None`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sabotage {
    None,
    NegateMirror,
}

/// Every circuit reads its base relations out of one arrival collection, so the
/// harness feeds a fixture's schedule verbatim and the circuit does the
/// selecting, exactly as the rule bodies do.
fn base<'scope>(arrivals: &Arrivals<'scope>, rel: &'static str, arity: usize) -> Derived<'scope> {
    arrivals
        .clone()
        .filter(move |row| row.rel == rel && row.values.len() == arity)
        .distinct()
}

fn column(row: &Row, index: usize) -> Value {
    row.values[index].clone()
}

/// Arrival rels appear in the oracle's delta stream too, so every circuit's
/// output carries them beside the heads it derives.
fn passthrough<'scope>(arrivals: &Arrivals<'scope>) -> Derived<'scope> {
    arrivals.clone().distinct()
}

fn build_circuit<'scope>(
    name: &str,
    arrivals: &Arrivals<'scope>,
    sabotage: Sabotage,
) -> Derived<'scope> {
    match name {
        "float_avg_is_grouped" => circuit_float_avg_is_grouped(arrivals),
        "float_exact_join_has_no_epsilon" => circuit_float_exact_join(arrivals),
        "retraction_only_tick_retracts_level_view" => circuit_mirror(arrivals, sabotage),
        "callgraph_derivation_over_extraction" => circuit_callgraph_calls(arrivals),
        "callgraph_unused_inverts_with_the_call_set" => circuit_callgraph_unused(arrivals),
        "ordered_program_level_fold_reaches_three_links" => circuit_leg_total(arrivals),
        "mutual_recursion_matches_oracle" => circuit_mutual_recursion(arrivals),
        "recount_retraction_reaches_two_heads_same_tick" => circuit_recount(arrivals),
        // One fixture program, two schedules; the panel grades both streams
        // through this one body.
        "coalesce_defaults_the_absent_row" | "coalesce_default_returns_when_source_retracts" => {
            circuit_coalesce(arrivals)
        }
        other => panic!("no hand circuit for panel program {other}"),
    }
}

/// mean(Group, avg(Value)) <- score(Group, Value)
fn circuit_float_avg_is_grouped<'scope>(arrivals: &Arrivals<'scope>) -> Derived<'scope> {
    let score = base(arrivals, "score", 2);
    let mean = score
        .map(|row| (column(&row, 0), column(&row, 1)))
        .reduce(
            |_group, input: &[(&Value, isize)], output: &mut Vec<(Value, isize)>| {
                let count: i64 = input.iter().map(|(_, weight)| *weight as i64).sum();
                if count <= 0 {
                    return;
                }
                let total: f64 = input
                    .iter()
                    .map(|(value, weight)| value.as_real() * (*weight as f64))
                    .sum();
                output.push((Value::real(total / count as f64), 1));
            },
        )
        .map(|(group, mean)| Row::new("mean", vec![group, mean]));
    passthrough(arrivals).concat(mean).consolidate()
}

/// matched(Name) <- left(Name, Value), right(Name, Value)
fn circuit_float_exact_join<'scope>(arrivals: &Arrivals<'scope>) -> Derived<'scope> {
    let left = base(arrivals, "left", 2);
    let right = base(arrivals, "right", 2);
    let matched = left
        .map(|row| ((column(&row, 0), column(&row, 1)), ()))
        .join_map(
            right.map(|row| ((column(&row, 0), column(&row, 1)), ())),
            |(name, _value), (), ()| Row::new("matched", vec![name.clone()]),
        )
        .distinct();
    passthrough(arrivals).concat(matched).consolidate()
}

/// mirror(Item) <- source_row(Item)
fn circuit_mirror<'scope>(arrivals: &Arrivals<'scope>, sabotage: Sabotage) -> Derived<'scope> {
    let mirror = base(arrivals, "source_row", 1)
        .map(|row| Row::new("mirror", vec![column(&row, 0)]))
        .distinct();
    let mirror = match sabotage {
        Sabotage::None => mirror,
        Sabotage::NegateMirror => mirror.negate(),
    };
    passthrough(arrivals).concat(mirror).consolidate()
}

/// def(Path, Name, Kind) <- node_fact(Path, node, Kind, Name)
fn callgraph_def<'scope>(arrivals: &Arrivals<'scope>) -> Derived<'scope> {
    base(arrivals, "node_fact", 4)
        .filter(|row| row.values[1] == Value::text("node"))
        .map(|row| {
            Row::new(
                "def",
                vec![column(&row, 0), column(&row, 3), column(&row, 2)],
            )
        })
        .distinct()
}

/// calls(Caller, Callee) <-
///   def(Path, Caller, _), call(Path, Callee), def(_, Callee, _), Caller \== Callee
fn circuit_callgraph_calls<'scope>(arrivals: &Arrivals<'scope>) -> Derived<'scope> {
    let def = callgraph_def(arrivals);
    let call = base(arrivals, "call", 2);
    let defined_names = def.clone().map(|row| column(&row, 1)).distinct();
    let calls = def
        .clone()
        .map(|row| (column(&row, 0), column(&row, 1)))
        .join_map(
            call.map(|row| (column(&row, 0), column(&row, 1))),
            |_path, caller, callee| (callee.clone(), caller.clone()),
        )
        .distinct()
        // the third body atom, def(_, Callee, _): the callee must be defined
        // somewhere, which is a semijoin against the name set, not a re-join
        // against def itself (that would multiply the row per defining path).
        .semijoin(defined_names)
        .filter(|(callee, caller)| caller != callee)
        .map(|(callee, caller)| Row::new("calls", vec![caller, callee]))
        .distinct();
    passthrough(arrivals)
        .concat(def)
        .concat(calls)
        .consolidate()
}

/// unused(Name) <- def(_, Name, _), not(call(_, Name))
fn circuit_callgraph_unused<'scope>(arrivals: &Arrivals<'scope>) -> Derived<'scope> {
    let def = callgraph_def(arrivals);
    let call = base(arrivals, "call", 2);
    let called = call.map(|row| column(&row, 1)).distinct();
    let unused = def
        .clone()
        .map(|row| (column(&row, 1), ()))
        .distinct()
        .antijoin(called)
        .map(|(name, ())| Row::new("unused", vec![name]))
        .distinct();
    passthrough(arrivals)
        .concat(def)
        .concat(unused)
        .consolidate()
}

/// leg_total(LegId, DispatchId, Kilos) <- dispatch_leg(LegId, DispatchId, 0, Kilos)
/// leg_total(LegId, DispatchId, KilosSoFar) <-
///   dispatch_leg(LegId, DispatchId, PreviousLeg, Kilos),
///   leg_total(PreviousLeg, DispatchId, KilosBefore),
///   KilosSoFar := KilosBefore + Kilos
fn circuit_leg_total<'scope>(arrivals: &Arrivals<'scope>) -> Derived<'scope> {
    let dispatch_leg = base(arrivals, "dispatch_leg", 4);
    let seed = dispatch_leg
        .clone()
        .filter(|row| row.values[2] == Value::Int(0))
        .map(|row| (column(&row, 0), column(&row, 1), column(&row, 3)));
    let step_source = dispatch_leg.clone();
    // `iterate` seeds the variable with `seed` and re-binds it to whatever the
    // closure returns, so the closure has to carry `reached` forward itself or
    // the base clause's rows fall out on the second round.
    let totals = seed.iterate(move |inner_scope, reached| {
        let legs = step_source.clone().enter(inner_scope);
        legs.map(|row| {
            (
                (column(&row, 2), column(&row, 1)),
                (column(&row, 0), column(&row, 3)),
            )
        })
        .join_map(
            reached
                .clone()
                .map(|(leg, dispatch, kilos)| ((leg, dispatch), kilos)),
            |(_previous, dispatch), (leg, kilos), before| {
                (
                    leg.clone(),
                    dispatch.clone(),
                    Value::Int(before.as_int() + kilos.as_int()),
                )
            },
        )
        .concat(reached)
        .distinct()
    });
    let leg_total =
        totals.map(|(leg, dispatch, kilos)| Row::new("leg_total", vec![leg, dispatch, kilos]));
    passthrough(arrivals).concat(leg_total).consolidate()
}

/// even(Value) <- seed(Value)
/// even(Value) <- odd(Value)
/// odd(Value)  <- even(Value)
fn circuit_mutual_recursion<'scope>(arrivals: &Arrivals<'scope>) -> Derived<'scope> {
    let seed = base(arrivals, "seed", 1);
    // The two heads share one iteration variable, tagged by head name; that is
    // what makes it a MUTUAL fixpoint rather than two independent ones.
    let both = seed
        .map(|row| (Value::text("even"), column(&row, 0)))
        .iterate(|_inner_scope, reached| {
            let to_odd = reached
                .clone()
                .filter(|(head, _)| *head == Value::text("even"))
                .map(|(_, value)| (Value::text("odd"), value));
            let to_even = reached
                .clone()
                .filter(|(head, _)| *head == Value::text("odd"))
                .map(|(_, value)| (Value::text("even"), value));
            reached.concat(to_odd).concat(to_even).distinct()
        });
    let heads = both.map(|(head, value)| {
        let rel = match &head {
            Value::Text(word) => word.clone(),
            other => panic!("head tag was not text: {other:?}"),
        };
        Row::new(&rel, vec![value])
    });
    passthrough(arrivals).concat(heads).consolidate()
}

/// b(Value) <- a(Value)
/// c(Value) <- b(Value)
fn circuit_recount<'scope>(arrivals: &Arrivals<'scope>) -> Derived<'scope> {
    let head_b = base(arrivals, "a", 1)
        .map(|row| Row::new("b", vec![column(&row, 0)]))
        .distinct();
    let head_c = head_b
        .clone()
        .map(|row| Row::new("c", vec![column(&row, 0)]))
        .distinct();
    passthrough(arrivals)
        .concat(head_b)
        .concat(head_c)
        .consolidate()
}

/// repo_latest(Name, Commit) <- repo(Name), coalesce(latest_commit(Name, Commit), 'absent')
///
/// The desugar 0_coalesce_expand.pl performs, spelled out: the present arm is
/// an ordinary join, the absent arm is the same body under not(...), and the
/// default is a literal in the head.
fn circuit_coalesce<'scope>(arrivals: &Arrivals<'scope>) -> Derived<'scope> {
    let repo = base(arrivals, "repo", 1);
    let latest_commit = base(arrivals, "latest_commit", 2);
    let present = repo
        .clone()
        .map(|row| (column(&row, 0), ()))
        .join_map(
            latest_commit
                .clone()
                .map(|row| (column(&row, 0), column(&row, 1))),
            |name, (), commit| Row::new("repo_latest", vec![name.clone(), commit.clone()]),
        )
        .distinct();
    let has_commit = latest_commit.map(|row| column(&row, 0)).distinct();
    let absent = repo
        .map(|row| (column(&row, 0), ()))
        .antijoin(has_commit)
        .map(|(name, ())| Row::new("repo_latest", vec![name, Value::text("absent")]))
        .distinct();
    passthrough(arrivals)
        .concat(present)
        .concat(absent)
        .consolidate()
}

// ═══ the run ════════════════════════════════════════════════════════════════

fn run_program(program: &Program, sabotage: Sabotage) -> Vec<Tick> {
    let captured: Arc<Mutex<BTreeMap<u64, Vec<(Row, isize)>>>> =
        Arc::new(Mutex::new(BTreeMap::new()));
    let sink = captured.clone();
    let name = program.name.clone();
    let seed = program.seed_arrivals.clone();
    let schedule = program.schedule.clone();
    let tick_count = schedule.len();

    timely::execute_directly(move |worker: &mut Worker| {
        let (mut input, probe) = worker.dataflow::<u64, _, _>(|scope| {
            let (handle, arrivals) = scope.new_collection::<Row, isize>();
            let output = build_circuit(&name, &arrivals, sabotage);
            let (probe, output) = output.probe();
            let sink = sink.clone();
            output.inspect_batch(move |time, updates| {
                let mut guard = sink.lock().unwrap();
                let bucket = guard.entry(*time).or_default();
                for (row, _time, weight) in updates {
                    bucket.push((row.clone(), *weight));
                }
            });
            (handle, probe)
        });

        for row in &seed {
            input.insert(row.clone());
        }
        for (index, tick) in schedule.iter().enumerate() {
            input.advance_to(index as u64 + 1);
            input.flush();
            while probe.less_than(input.time()) {
                worker.step();
            }
            for signed in tick {
                let (row, weight) = signed.split();
                input.update(row, weight);
            }
        }
        input.advance_to(tick_count as u64 + 1);
        input.flush();
        while probe.less_than(input.time()) {
            worker.step();
        }
    });

    let guard = captured.lock().unwrap();
    (0..=tick_count as u64)
        .map(|time| tick_from(guard.get(&time).cloned().unwrap_or_default()))
        .collect()
}

// ═══ the tests ══════════════════════════════════════════════════════════════

#[test]
fn panel_is_the_ten_programs_the_exporter_names() {
    let panel = load_panel();
    let names: Vec<&str> = panel
        .programs
        .iter()
        .map(|program| program.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "float_avg_is_grouped",
            "float_exact_join_has_no_epsilon",
            "retraction_only_tick_retracts_level_view",
            "callgraph_derivation_over_extraction",
            "callgraph_unused_inverts_with_the_call_set",
            "ordered_program_level_fold_reaches_three_links",
            "mutual_recursion_matches_oracle",
            "recount_retraction_reaches_two_heads_same_tick",
            "coalesce_defaults_the_absent_row",
            "coalesce_default_returns_when_source_retracts",
        ],
        "the panel drifted; regenerate dd_panel.json and add the hand circuit"
    );
}

#[test]
fn dd_stream_equals_oracle_stream_at_every_tick() {
    let panel = load_panel();
    let mut failures: Vec<String> = Vec::new();
    for program in &panel.programs {
        let dd = run_program(program, Sabotage::None);
        let oracle = oracle_stream(program);
        if let Err(report) = compare_streams(&dd, &oracle) {
            failures.push(format!("{} [{}]\n  {report}", program.name, program.note));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} panel programs diverged from the oracle:\n\n{}",
        failures.len(),
        panel.programs.len(),
        failures.join("\n\n")
    );
}

#[test]
fn dd_accumulated_stream_equals_oracle_finals() {
    let panel = load_panel();
    let mut failures: Vec<String> = Vec::new();
    for program in &panel.programs {
        let accumulated = accumulate(&run_program(program, Sabotage::None));
        let expected = tick_from(program.finals.iter().map(|row| (row.clone(), 1isize)));
        if accumulated != expected {
            failures.push(format!(
                "{}\n  dd     : {}\n  oracle : {}",
                program.name,
                render(&accumulated),
                render(&expected)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} panel programs accumulated to the wrong finals:\n\n{}",
        failures.len(),
        panel.programs.len(),
        failures.join("\n\n")
    );
}

#[test]
fn sabotage_flipped_sign_is_caught() {
    let panel = load_panel();
    let program = panel
        .programs
        .iter()
        .find(|program| program.name == "retraction_only_tick_retracts_level_view")
        .expect("the sabotage target is in the panel");

    let clean = run_program(program, Sabotage::None);
    let oracle = oracle_stream(program);
    compare_streams(&clean, &oracle).expect("the unsabotaged circuit agrees");

    let flipped = run_program(program, Sabotage::NegateMirror);
    let report =
        compare_streams(&flipped, &oracle).expect_err("a negated head must not read as agreement");
    assert!(
        report.contains("tick 1:") && report.contains("mirror(alpha)"),
        "the diff named neither the tick nor the row:\n{report}"
    );
    assert!(
        report.contains("dd only    : -mirror(alpha) x2, -mirror(beta) x2"),
        "the diff did not carry the flipped signs:\n{report}"
    );
}
