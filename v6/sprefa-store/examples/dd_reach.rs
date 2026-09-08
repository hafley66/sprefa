//! Differential Dataflow arm for `bench/run.sh`.
//!
//! Uses the shared `benchgraph::gen(layers, width)` DAG, inserts roots 0 and 1,
//! reaches the initial fixed point, retracts root 0, and reaches the updated
//! fixed point. Complete ordered results are checked against an independent
//! BFS outside the timed phases.

use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use differential_dataflow::input::Input;
use differential_dataflow::operators::Iterate;
use timely::dataflow::operators::probe::Handle as ProbeHandle;

use sprefa_store::{benchgraph, memcap};

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

static ALL_RECORDS: AtomicU64 = AtomicU64::new(0);

fn peak_rss_mb() -> f64 {
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        let bytes = if cfg!(target_os = "linux") {
            usage.ru_maxrss as f64 * 1024.0
        } else {
            usage.ru_maxrss as f64
        };
        bytes / (1024.0 * 1024.0)
    }
}

fn oracle_survivors(parents: &[Vec<i64>]) -> Vec<i64> {
    let mut children = vec![Vec::new(); parents.len()];
    for (child, node_parents) in parents.iter().enumerate() {
        for &parent in node_parents {
            children[parent as usize].push(child as i64);
        }
    }
    let mut seen = vec![false; parents.len()];
    let mut queue = VecDeque::from([1i64]);
    seen[1] = true;
    while let Some(parent) = queue.pop_front() {
        for &child in &children[parent as usize] {
            if seen[child as usize] {
                continue;
            }
            seen[child as usize] = true;
            queue.push_back(child);
        }
    }
    seen.into_iter()
        .enumerate()
        .filter_map(|(node, alive)| alive.then_some(node as i64))
        .collect()
}

fn alive_ids(alive: &Mutex<HashMap<i64, isize>>) -> Vec<i64> {
    let map = alive.lock().unwrap();
    let mut ids: Vec<i64> = map
        .iter()
        .filter(|(_, weight)| **weight > 0)
        .map(|(node, _)| *node)
        .collect();
    ids.sort_unstable();
    ids
}

fn main() {
    let cap_mb = std::env::var("DL_MEMCAP_MB")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4096);
    if cap_mb != 0 {
        memcap::cap_address_space_mb(cap_mb);
    }

    let args: Vec<String> = std::env::args().collect();
    let layers = args
        .get(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(8usize)
        .clamp(1, 20);
    let width = args
        .get(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(20_000usize)
        .clamp(1, 500_000);

    let graph_started = Instant::now();
    let mut parents = benchgraph::gen(layers, width);
    let back_stride: usize = std::env::var("BENCH_BACK_STRIDE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if back_stride > 0 {
        for node in (2 + width)..parents.len() {
            if node % back_stride == 0 {
                parents[node - width].push(node as i64);
            }
        }
    }
    let edges = benchgraph::edges(&parents);
    let graph_setup = graph_started.elapsed();
    let mut sorted_edges = edges.clone();
    sorted_edges.sort_unstable();
    let mut input_hash = Sha256::new();
    for (parent, child) in sorted_edges {
        input_hash.update(format!("{parent},{child}\n"));
    }
    eprintln!("INPUT|differential-dataflow|{:x}", input_hash.finalize());
    let expected_before: Vec<i64> = (0..parents.len() as i64).collect();
    let expected_after = oracle_survivors(&parents);
    let expected_after_in = expected_after.clone();
    let node_count = parents.len();
    let edge_count = edges.len();

    let alive = Arc::new(Mutex::new(HashMap::<i64, isize>::new()));
    let alive_out = Arc::clone(&alive);

    let dataflow_started = Instant::now();
    let (setup, retract, build_records, retract_records) =
        timely::execute_directly(move |worker| {
            let accumulator = Arc::clone(&alive);
            let mut probe = ProbeHandle::new();
            let (mut edges_input, mut roots_input) = worker.dataflow(|scope| {
                let (edges_input, edges_collection) = scope.new_collection::<(i64, i64), isize>();
                let (roots_input, roots_collection) = scope.new_collection::<i64, isize>();
                let edges_for_loop = edges_collection.clone();
                let roots_for_loop = roots_collection.clone();
                let reach = roots_collection.iterate(move |inner_scope, inner| {
                    let loop_edges = edges_for_loop.enter(inner_scope);
                    let loop_roots = roots_for_loop.enter(inner_scope);
                    loop_edges
                        .semijoin(inner)
                        .map(|(_parent, child)| child)
                        .concat(loop_roots)
                        .distinct()
                });
                reach
                    .consolidate()
                    .inspect(move |(node, _time, difference)| {
                        ALL_RECORDS.fetch_add(1, Ordering::Relaxed);
                        *accumulator.lock().unwrap().entry(*node).or_insert(0) += *difference;
                    })
                    .probe_with(&mut probe);
                (edges_input, roots_input)
            });

            for &(parent, child) in &edges {
                edges_input.insert((parent, child));
            }
            roots_input.insert(0);
            roots_input.insert(1);
            edges_input.advance_to(1);
            roots_input.advance_to(1);
            edges_input.flush();
            roots_input.flush();
            worker.step_while(|| probe.less_than(edges_input.time()));
            let initial_count = alive
                .lock()
                .unwrap()
                .values()
                .filter(|weight| **weight > 0)
                .count();
            let setup = graph_setup + dataflow_started.elapsed();
            assert_eq!(
                initial_count,
                expected_before.len(),
                "DD initial count mismatch"
            );
            assert_eq!(
                alive_ids(&alive),
                expected_before,
                "DD initial set mismatch"
            );
            let build_records = ALL_RECORDS.load(Ordering::Relaxed);

            let retract_started = Instant::now();
            roots_input.remove(0);
            edges_input.advance_to(2);
            roots_input.advance_to(2);
            edges_input.flush();
            roots_input.flush();
            worker.step_while(|| probe.less_than(roots_input.time()));
            let survivor_count = alive
                .lock()
                .unwrap()
                .values()
                .filter(|weight| **weight > 0)
                .count();
            let retract = retract_started.elapsed();
            assert_eq!(
                survivor_count,
                expected_after_in.len(),
                "DD survivor count mismatch"
            );
            let retract_records = ALL_RECORDS.load(Ordering::Relaxed) - build_records;
            (setup, retract, build_records, retract_records)
        });

    let survivors = alive_ids(&alive_out);
    assert_eq!(survivors, expected_after, "DD survivor set mismatch");
    let killed = node_count - survivors.len();
    let rss = peak_rss_mb();
    eprintln!(
        "STATUS|differential-dataflow|ok|differential-dataflow 0.25 with timely 0.31; shared layered graph; setup and retract end after fixed-point materialization and count; exact ordered sets match BFS oracle|process peak RSS from getrusage including untimed oracle allocations|DL_MEMCAP_MB={cap_mb} caps live Rust allocations through CappedAlloc (0 disables); RLIMIT_AS and RLIMIT_DATA are also requested best-effort; total process RSS is not capped"
    );
    eprintln!(
        "[dd] SETUP nodes={node_count} edges={edge_count} records={build_records} ms={:.3}",
        setup.as_secs_f64() * 1e3
    );
    eprintln!(
        "[dd] RETRACT killed={killed} survivors={} records={retract_records} ms={:.3}",
        survivors.len(),
        retract.as_secs_f64() * 1e3
    );
    eprintln!(
        "CSV,differential-dataflow,{node_count},{edge_count},{killed},{:.3},{:.3},{retract_records},{rss:.1}",
        setup.as_secs_f64() * 1e3,
        retract.as_secs_f64() * 1e3
    );
}

#[cfg(test)]
mod tests {
    use super::oracle_survivors;
    use sprefa_store::benchgraph;

    #[test]
    fn oracle_uses_the_shared_contiguous_node_graph() {
        let parents = benchgraph::gen(3, 4);
        assert_eq!(
            oracle_survivors(&parents),
            vec![1, 2, 5, 6, 8, 9, 10, 11, 12, 13]
        );
    }
}
