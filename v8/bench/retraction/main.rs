//! `cargo bench --bench retraction -- [rows ...]`: `watch.dl7` at each size, one
//! keyed `watch` replacement per tick, 20 measured ticks, three samples.

use dl8::_6_eval::evaluate::{Evaluate, Store};
use dl8::_6_eval::json::{program_from_json, program_names, program_to_json, relations_to_json};
use dl8::_6_eval::{Program, TermId, Universe};
use dl8::_7_effect::Slice;
use std::path::Path;
use std::time::{Duration, Instant};

const TICKS: usize = 20;
const SAMPLES: usize = 3;
const TAGS: i64 = 100;
const CHANNELS: i64 = 10;
const TICK_CAP: Duration = Duration::from_secs(10);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Variant {
    /// The program with its `relations` dropped: insert-only, so `watch` keeps
    /// every old tag and the closure is wrong. The as-built floor.
    InsertOnly,
    DeleteRederive,
}

struct Family {
    u: Universe,
    program: Program,
    watch: TermId,
    release: TermId,
    tracked: TermId,
    channel_repos: TermId,
}

fn family(variant: Variant) -> Family {
    let mut u = Universe::new();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("bench/retraction/watch.dl7");
    let compiled = dl8::compile(&mut u, &source, &mut |_| {}).expect("watch.dl7 compiles");
    assert!(compiled.diagnostics.is_empty(), "watch.dl7 diagnostics");
    let mut body = program_to_json(&mut u, compiled.runtime_program).expect("transport");
    body["relations"] = relations_to_json(&u, compiled.runtime_program, &compiled.compiler_rows);
    let mut program = program_from_json(&mut u, &body).expect("program");
    let names = program_names(&mut u, &body).expect("names");
    if variant == Variant::InsertOnly {
        program.relations.clear();
    }
    Family {
        u,
        program,
        watch: names["watch"],
        release: names["release"],
        tracked: names["tracked"],
        channel_repos: names["channel_repos"],
    }
}

fn cells(u: &mut Universe, values: &[i64]) -> Box<[TermId]> {
    values
        .iter()
        .map(|value| {
            let n = u.int(*value);
            u.compound("const", vec![n])
        })
        .collect()
}

fn evaluate(family: &mut Family, store: &mut Store) {
    store.mark_all();
    let closure = Evaluate::reduce(store, (&mut family.u, &family.program), &mut |_| {});
    assert!(closure.diagnostics.is_empty(), "{:?}", closure.diagnostics);
}

struct Tick {
    ms: f64,
    rows: usize,
}

fn sample(variant: Variant, rows: i64) -> Vec<Tick> {
    let mut family = family(variant);
    let mut store = Store::default();
    for tag in 0..TAGS {
        let row = cells(&mut family.u, &[tag, tag % CHANNELS]);
        store.insert(family.release, row);
    }
    for repo in 0..rows {
        let row = cells(&mut family.u, &[repo, repo % TAGS]);
        store.insert(family.watch, row);
    }
    evaluate(&mut family, &mut store);
    let mut ticks = Vec::with_capacity(TICKS);
    for tick in 0..TICKS as i64 {
        let repo = (tick * 7919) % rows;
        let row = cells(&mut family.u, &[repo, (repo + 1 + tick) % TAGS]);
        let before = store.written + store.removed;
        let started = Instant::now();
        store.insert(family.watch, row);
        evaluate(&mut family, &mut store);
        let elapsed = started.elapsed();
        assert!(
            elapsed < TICK_CAP,
            "{variant:?} rows={rows} tick {tick} took {elapsed:?}"
        );
        ticks.push(Tick {
            ms: elapsed.as_secs_f64() * 1000.0,
            rows: store.written + store.removed - before,
        });
    }
    if variant != Variant::InsertOnly {
        let tracked = store.table(family.tracked).map_or(0, |t| t.len());
        assert_eq!(tracked as i64, rows, "tracked holds one row per repo");
        let groups = store.table(family.channel_repos).map_or(0, |t| t.len());
        assert_eq!(groups as i64, CHANNELS, "one channel_repos row per channel");
    }
    ticks
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    values[values.len() / 2]
}

fn main() {
    let sizes: Vec<i64> = std::env::args()
        .skip(1)
        .filter_map(|arg| arg.parse().ok())
        .collect();
    let sizes = match sizes.is_empty() {
        true => vec![1_000, 10_000, 100_000],
        false => sizes,
    };
    println!("| variant | rows | ms/tick median per sample | ms/tick max | rows rewritten/tick |");
    println!("|---|---:|---|---:|---:|");
    for rows in sizes {
        for variant in [Variant::InsertOnly, Variant::DeleteRederive] {
            let samples: Vec<Vec<Tick>> = (0..SAMPLES).map(|_| sample(variant, rows)).collect();
            let medians: Vec<String> = samples
                .iter()
                .map(|ticks| {
                    let mut ms: Vec<f64> = ticks.iter().map(|t| t.ms).collect();
                    format!("{:.3}", median(&mut ms))
                })
                .collect();
            let max = samples
                .iter()
                .flatten()
                .map(|t| t.ms)
                .fold(0.0_f64, f64::max);
            let rewritten: usize = samples.iter().flatten().map(|t| t.rows).sum();
            let ticks = samples.iter().map(Vec::len).sum::<usize>();
            println!(
                "| {variant:?} | {rows} | {} | {max:.3} | {:.1} |",
                medians.join(", "),
                rewritten as f64 / ticks as f64
            );
        }
    }
}
