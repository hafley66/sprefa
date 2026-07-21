//! The DBSP (Feldera) side of the head-to-head: the SAME reachability, in the
//! third Z-set/IVM engine. Feldera's `dbsp` crate is a resident incremental
//! engine like differential-dataflow, with the Z-set algebra as first-class
//! `recursive` circuits. Step 1 builds the reachable set from roots {0,1};
//! step 2 retracts root 0 and dbsp emits the delta incrementally.
//! stdout = sorted surviving node ids (byte-identical to sqlite/dd).
//! stderr = SETUP + MEASURED retract with the incremental delta size.
//! `cargo run --release --example dbsp_reach -- <layers> <width>`

use std::collections::HashMap;
use std::time::Instant;

use dbsp::typed_batch::IndexedZSetReader;
use dbsp::{Circuit, OrdZSet, Runtime, Stream, operator::Generator, utils::Tup2};

use sprefa_store::{benchgraph, memcap};

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

fn peak_rss_mb() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        let bytes = if cfg!(target_os = "linux") {
            (ru.ru_maxrss as f64) * 1024.0
        } else {
            ru.ru_maxrss as f64
        };
        bytes / (1024.0 * 1024.0)
    }
}

fn main() {
    let cap_mb: u64 = std::env::var("DL_MEMCAP_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(4096);
    if cap_mb != 0 {
        memcap::cap_address_space_mb(cap_mb);
    }

    let args: Vec<String> = std::env::args().collect();
    let layers: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(8).clamp(1, 20);
    let width: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20_000).clamp(1, 500_000);

    // Multi-relation graph, encoded to dense node keys so dbsp sees
    // byte-identical inputs/outputs to the tagged SQLite side. Roots encode to 0/1.
    let g = benchgraph::gen_multi(layers, width);
    let edge_list: Vec<(i64, i64)> = g
        .edges
        .iter()
        .map(|(pt, pid, ct, cid)| (benchgraph::encode(*pt, *pid), benchgraph::encode(*ct, *cid)))
        .collect();
    let n = g.rows.len();
    let n_edges = edge_list.len();

    // Per-step input deltas. Step 1: all edges + roots {0,1}. Step 2: no edge
    // change, retract root 0.
    let edge_tuples: Vec<Tup2<Tup2<u64, u64>, i64>> = edge_list
        .iter()
        .map(|(f, t)| Tup2(Tup2(*f as u64, *t as u64), 1))
        .collect();
    let mut edge_steps = vec![
        OrdZSet::from_keys((), edge_tuples),
        OrdZSet::from_keys((), Vec::new()),
    ]
    .into_iter();
    let mut root_steps = vec![
        OrdZSet::from_keys((), vec![Tup2(0u64, 1i64), Tup2(1u64, 1i64)]),
        OrdZSet::from_keys((), vec![Tup2(0u64, -1i64)]),
    ]
    .into_iter();

    let (mut circuit, output) = Runtime::init_circuit(1, move |root_circuit| {
        let edges = root_circuit.add_source(Generator::new(move || edge_steps.next().unwrap()));
        let roots = root_circuit.add_source(Generator::new(move || root_steps.next().unwrap()));

        let reachable = root_circuit.recursive(
            |child, reach: Stream<_, OrdZSet<u64>>| {
                let edges = edges.delta0(child);
                let roots = roots.delta0(child);
                let extended = reach
                    .map_index(|n| (*n, ()))
                    .join(
                        &edges.map_index(|Tup2(f, t)| (*f, *t)),
                        |_from, &(), &to| to,
                    )
                    .plus(&roots);
                Ok(extended.distinct())
            },
        )?;

        Ok(reachable.output())
    })
    .expect("build dbsp circuit");

    let mut alive: HashMap<u64, i64> = HashMap::new();

    // ---- SETUP: step 1 ------------------------------------------------------
    let t = Instant::now();
    circuit.transaction().unwrap();
    let setup = t.elapsed();
    let mut build_recs = 0u64;
    for (node, (), weight) in output.consolidate().iter() {
        build_recs += 1;
        *alive.entry(node).or_insert(0) += weight;
    }

    // ---- MEASURED: step 2 (retract root 0) ----------------------------------
    let t = Instant::now();
    circuit.transaction().unwrap();
    let retract = t.elapsed();
    let mut retract_recs = 0u64;
    for (node, (), weight) in output.consolidate().iter() {
        retract_recs += 1;
        *alive.entry(node).or_insert(0) += weight;
    }

    let mut ids: Vec<u64> = alive.iter().filter(|(_, w)| **w > 0).map(|(d, _)| *d).collect();
    ids.sort_unstable();
    let survivors = ids.len();
    let killed = n - survivors;

    let mut buf = String::new();
    for id in &ids {
        buf.push_str(&id.to_string());
        buf.push('\n');
    }
    print!("{buf}");

    let rss = peak_rss_mb();
    eprintln!(
        "[dbsp]   SETUP    nodes={n} edges={n_edges} | {:?} | {} records (build the circuit state once)",
        setup, build_recs
    );
    eprintln!(
        "[dbsp]   RETRACT  killed={killed} survivors={survivors} | {:?} | {} delta records \
         | {:.4} records/killed-row | peak_rss {:.1} MB",
        retract,
        retract_recs,
        retract_recs as f64 / killed.max(1) as f64,
        rss
    );
    eprintln!(
        "CSV,dbsp,{n},{n_edges},{killed},{:.3},{:.3},{},{:.1}",
        setup.as_secs_f64() * 1e3,
        retract.as_secs_f64() * 1e3,
        retract_recs,
        rss
    );
}
