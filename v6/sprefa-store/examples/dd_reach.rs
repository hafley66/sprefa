//! The differential-dataflow side of the head-to-head: the SAME reachability,
//! maintained by the resident incremental engine. Feed edges + roots, take the
//! initial reachable set, then RETRACT root 0 and let DD produce the delta.
//! stdout = sorted surviving node ids (byte-identical to sqlite_reach).
//! stderr = SETUP (one-time) and the MEASURED retract, with the incremental
//! delta size (dd's unit of work) alongside wall time.
//! `cargo run --release --example dd_reach -- <layers> <width>`
//!
//! Pinned to differential-dataflow 0.25 / timely 0.31. The 0.12 pair has a
//! non-terminating `iterate` on aarch64-apple-darwin (its own doc example never
//! converges, verified in isolation) — the resident-engine runaway this whole
//! exercise is about. 0.25 converges.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use differential_dataflow::input::Input;
use differential_dataflow::operators::Iterate;
use timely::dataflow::operators::probe::Handle as ProbeHandle;

use sprefa_store::{benchgraph, memcap};

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

// dd has no SQL statements; its unit of incremental work is the number of output
// diff records it emits. ALL_RECORDS counts every inspect firing; we snapshot it
// around the retract step so the delta = records emitted during the retract,
// robust to whatever internal timestamp the consolidate reports.
static ALL_RECORDS: AtomicU64 = AtomicU64::new(0);

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

    // Multi-relation graph, encoded to dense node keys so dd sees byte-identical
    // inputs/outputs to the tagged SQLite side. Roots encode to 0 and 1.
    let g = benchgraph::gen_multi(layers, width);
    let edge_list: Vec<(i64, i64)> = g
        .edges
        .iter()
        .map(|(pt, pid, ct, cid)| (benchgraph::encode(*pt, *pid), benchgraph::encode(*ct, *cid)))
        .collect();
    let n = g.rows.len();
    let n_edges = edge_list.len();

    // net accumulated diff per node across all times; >0 means alive at the end.
    let alive: Arc<Mutex<HashMap<i64, isize>>> = Arc::new(Mutex::new(HashMap::new()));
    let alive_out = alive.clone();

    let (setup, retract, build_recs, retract_recs) = timely::execute_directly(move |worker| {
        let acc = alive.clone();
        let mut probe = ProbeHandle::new();

        let (mut edges_in, mut roots_in) = worker.dataflow(|scope| {
            let (edges_in, edges) = scope.new_collection::<(i64, i64), isize>();
            let (roots_in, roots) = scope.new_collection::<i64, isize>();

            let edges_c = edges.clone();
            let roots_c = roots.clone();
            let reach = roots.iterate(move |scope, inner| {
                let edges = edges_c.enter(scope);
                let roots = roots_c.enter(scope);
                edges
                    .semijoin(inner)
                    .map(|(_parent, child)| child)
                    .concat(roots)
                    .distinct()
            });

            reach
                .consolidate()
                .inspect(move |(node, _time, diff)| {
                    ALL_RECORDS.fetch_add(1, Ordering::Relaxed);
                    *acc.lock().unwrap().entry(*node).or_insert(0) += *diff;
                })
                .probe_with(&mut probe);

            (edges_in, roots_in)
        });

        // ---- SETUP: feed edges + both roots, advance to time 1 --------------
        let t = Instant::now();
        for (parent, child) in &edge_list {
            edges_in.insert((*parent, *child));
        }
        roots_in.insert(0);
        roots_in.insert(1);
        edges_in.advance_to(1);
        roots_in.advance_to(1);
        edges_in.flush();
        roots_in.flush();
        worker.step_while(|| probe.less_than(edges_in.time()));
        let setup = t.elapsed();
        let build_recs = ALL_RECORDS.load(Ordering::Relaxed);

        // ---- MEASURED: retract root 0 at time 2 -----------------------------
        let t = Instant::now();
        roots_in.remove(0);
        edges_in.advance_to(2);
        roots_in.advance_to(2);
        edges_in.flush();
        roots_in.flush();
        worker.step_while(|| probe.less_than(roots_in.time()));
        let retract = t.elapsed();
        let retract_recs = ALL_RECORDS.load(Ordering::Relaxed) - build_recs;

        (setup, retract, build_recs, retract_recs)
    });

    let map = alive_out.lock().unwrap();
    let mut ids: Vec<i64> = map.iter().filter(|(_, w)| **w > 0).map(|(d, _)| *d).collect();
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
        "[dd]     SETUP    nodes={n} edges={n_edges} | {:?} | {} diff records (build the arrangement once)",
        setup, build_recs
    );
    eprintln!(
        "[dd]     RETRACT  killed={killed} survivors={survivors} | {:?} | {} delta records \
         | {:.4} records/killed-row | peak_rss {:.1} MB",
        retract,
        retract_recs,
        retract_recs as f64 / killed.max(1) as f64,
        rss
    );
    // Machine-parseable: engine,nodes,edges,killed,setup_ms,retract_ms,ops,rss_mb
    eprintln!(
        "CSV,dd,{n},{n_edges},{killed},{:.3},{:.3},{},{:.1}",
        setup.as_secs_f64() * 1e3,
        retract.as_secs_f64() * 1e3,
        retract_recs,
        rss
    );
}
