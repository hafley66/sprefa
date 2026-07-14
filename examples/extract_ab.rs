//! A/B proof for Rust extraction: three production extractor calls versus one
//! parse feeding all three production projections.
//!
//! Usage: `cargo run --release --example extract_ab -- <baseline|bundle|verify> <files>`

use rayon::prelude::*;
use serde::Serialize;
use sprefa_v5::typegraph::{
    AnalysisBundle, AnalysisMask, CallFacts, DataflowFacts, RustTypes, TypeFacts, TypeLang,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Default)]
struct InFlight {
    jobs: AtomicUsize,
    bytes: AtomicUsize,
    max_jobs: AtomicUsize,
    max_bytes: AtomicUsize,
}

impl InFlight {
    fn enter(self: &Arc<Self>, bytes: usize) -> FlightGuard {
        let jobs = self.jobs.fetch_add(1, Ordering::SeqCst) + 1;
        let live_bytes = self.bytes.fetch_add(bytes, Ordering::SeqCst) + bytes;
        self.max_jobs.fetch_max(jobs, Ordering::SeqCst);
        self.max_bytes.fetch_max(live_bytes, Ordering::SeqCst);
        FlightGuard {
            state: Arc::clone(self),
            bytes,
        }
    }
}

struct FlightGuard {
    state: Arc<InFlight>,
    bytes: usize,
}

impl Drop for FlightGuard {
    fn drop(&mut self) {
        self.state.bytes.fetch_sub(self.bytes, Ordering::SeqCst);
        self.state.jobs.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Serialize)]
struct Counters<'a> {
    arm: &'a str,
    files: usize,
    source_bytes: usize,
    inventory_sweeps: usize,
    reads: usize,
    parse_calls: usize,
    wall_ms: f64,
    type_rows: usize,
    call_rows: usize,
    dataflow_rows: usize,
    checksum: String,
    max_jobs_inflight: usize,
    max_input_bytes_inflight: usize,
    two_largest_input_bytes: usize,
    outputs_equal: Option<bool>,
}

fn main() -> anyhow::Result<()> {
    let arm = std::env::args().nth(1).unwrap_or_else(|| "verify".into());
    let count: usize = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "128".into())
        .parse()?;
    if !matches!(arm.as_str(), "baseline" | "bundle" | "verify") {
        anyhow::bail!("arm must be baseline, bundle, or verify");
    }

    let fixture_root =
        std::env::temp_dir().join(format!("sprefa-extract-ab-{}-{count}", std::process::id()));
    if fixture_root.exists() {
        std::fs::remove_dir_all(&fixture_root)?;
    }
    std::fs::create_dir_all(&fixture_root)?;
    generate_fixtures(&fixture_root, count)?;

    // The sole inventory sweep. Paths are identities only; source is read by workers.
    let mut files: Vec<PathBuf> = std::fs::read_dir(&fixture_root)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    files.sort();
    let sizes: Vec<usize> = files
        .iter()
        .map(|p| p.metadata().map(|m| m.len() as usize))
        .collect::<Result<_, _>>()?;
    let source_bytes = sizes.iter().sum();
    let mut sorted_sizes = sizes.clone();
    sorted_sizes.sort_unstable_by(|a, b| b.cmp(a));
    let two_largest = sorted_sizes.iter().take(2).sum();

    let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build()?;
    let (bundles, elapsed, reads, parses, flight, equal) = match arm.as_str() {
        "baseline" => {
            let flight = Arc::new(InFlight::default());
            let start = Instant::now();
            let out = pool.install(|| baseline(&files, &flight));
            (out, start.elapsed(), count * 3, count * 3, flight, None)
        }
        "bundle" => {
            let flight = Arc::new(InFlight::default());
            let start = Instant::now();
            let out = pool.install(|| bundled(&files, &flight));
            (out, start.elapsed(), count, count, flight, None)
        }
        "verify" => {
            let baseline_flight = Arc::new(InFlight::default());
            let bundle_flight = Arc::new(InFlight::default());
            let start = Instant::now();
            let a = pool.install(|| baseline(&files, &baseline_flight));
            let b = pool.install(|| bundled(&files, &bundle_flight));
            let elapsed = start.elapsed();
            let equal = a == b;
            if !equal {
                anyhow::bail!("baseline and bundled production facts differ");
            }
            (b, elapsed, count * 4, count * 4, bundle_flight, Some(equal))
        }
        _ => unreachable!(),
    };

    let (type_rows, call_rows, dataflow_rows) = row_counts(&bundles);
    let counters = Counters {
        arm: &arm,
        files: count,
        source_bytes,
        inventory_sweeps: 1,
        reads,
        parse_calls: parses,
        wall_ms: elapsed.as_secs_f64() * 1000.0,
        type_rows,
        call_rows,
        dataflow_rows,
        checksum: checksum(&bundles),
        max_jobs_inflight: flight.max_jobs.load(Ordering::SeqCst),
        max_input_bytes_inflight: flight.max_bytes.load(Ordering::SeqCst),
        two_largest_input_bytes: two_largest,
        outputs_equal: equal,
    };
    println!("{}", serde_json::to_string(&counters)?);
    std::fs::remove_dir_all(&fixture_root)?;
    Ok(())
}

fn read_source(path: &Path, flight: &Arc<InFlight>) -> (String, FlightGuard) {
    let content =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let guard = flight.enter(content.len());
    (content, guard)
}

fn logical_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

fn baseline(files: &[PathBuf], flight: &Arc<InFlight>) -> Vec<AnalysisBundle> {
    let types: Vec<TypeFacts> = files
        .par_iter()
        .map(|path| {
            let (content, _guard) = read_source(path, flight);
            RustTypes.extract(&logical_name(path), &content)
        })
        .collect();
    let calls: Vec<CallFacts> = files
        .par_iter()
        .map(|path| {
            let (content, _guard) = read_source(path, flight);
            RustTypes.extract_calls(&logical_name(path), &content)
        })
        .collect();
    let dataflow: Vec<DataflowFacts> = files
        .par_iter()
        .map(|path| {
            let (content, _guard) = read_source(path, flight);
            RustTypes.extract_dataflow(&logical_name(path), &content)
        })
        .collect();
    types
        .into_iter()
        .zip(calls)
        .zip(dataflow)
        .map(|((types, calls), dataflow)| AnalysisBundle {
            types: Some(types),
            calls: Some(calls),
            dataflow: Some(dataflow),
        })
        .collect()
}

fn bundled(files: &[PathBuf], flight: &Arc<InFlight>) -> Vec<AnalysisBundle> {
    files
        .par_iter()
        .map(|path| {
            let (content, _guard) = read_source(path, flight);
            RustTypes.extract_bundle(&logical_name(path), &content, AnalysisMask::ALL)
        })
        .collect()
}

fn row_counts(bundles: &[AnalysisBundle]) -> (usize, usize, usize) {
    bundles.iter().fold((0, 0, 0), |mut n, b| {
        if let Some(f) = &b.types {
            n.0 += f.entities.len() + f.edges.len() + f.docs.len() + f.consts.len();
        }
        if let Some(f) = &b.calls {
            n.1 += f.defs.len() + f.sites.len();
        }
        if let Some(f) = &b.dataflow {
            n.2 += f.nodes.len()
                + f.edges.len()
                + f.loops.len()
                + f.allocators.len()
                + f.nests.len()
                + f.param_pos.len()
                + f.args.len()
                + f.fields.len()
                + f.lits.len();
        }
        n
    })
}

fn checksum(bundles: &[AnalysisBundle]) -> String {
    let mut h = blake3::Hasher::new();
    for b in bundles {
        h.update(format!("{:?}", b.types).as_bytes());
        h.update(format!("{:?}", b.calls).as_bytes());
        if let Some(df) = &b.dataflow {
            h.update(format!("{:?}{:?}{:?}", df.nodes, df.edges, df.loops).as_bytes());
            let mut allocators: Vec<_> = df.allocators.iter().collect();
            allocators.sort();
            h.update(format!("{:?}", allocators).as_bytes());
            h.update(
                format!(
                    "{:?}{:?}{:?}{:?}{:?}",
                    df.nests, df.param_pos, df.args, df.fields, df.lits
                )
                .as_bytes(),
            );
        }
    }
    h.finalize().to_hex().to_string()
}

fn generate_fixtures(root: &Path, count: usize) -> anyhow::Result<()> {
    for i in 0..count {
        let mut src = format!("//! deterministic fixture {i}\n\npub struct Record{i} {{ pub value: i64, pub label: String }}\n\n");
        for j in 0..64 {
            src.push_str(&format!(
                "/// Transform record {i}, lane {j}.\npub fn transform_{i}_{j}(input: i64) -> i64 {{\n    let seed = input + {j};\n    let values = vec![seed, seed * 2, seed * 3];\n    values.iter().map(|value| value + seed).sum()\n}}\n\n"
            ));
        }
        std::fs::write(root.join(format!("fixture_{i:05}.rs")), src)?;
    }
    Ok(())
}
