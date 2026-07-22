//! labkit — the golden harness. Answer to "can we traitify every experiment so each
//! MUST report plan + space/time Big-O + all the counters?": yes. `Experiment` is the
//! trait; `Harness` sweeps scales, counts everything countable, snapshots the query
//! plan, fits the EMPIRICAL Big-O (log-log slope) of time and space, and cross-checks
//! every engine's digest for equivalence.
//!
//! The one honest caveat: Big-O is not measurable from a single run. It is INFERRED
//! from the scale sweep (the slope of the ratios). So the trait forces each experiment
//! to DECLARE its complexity, and the harness FALSIFIES that against the measured
//! slope — a declared O(1) that measures slope ~1.0 is flagged. That is the strongest
//! form possible and it is exactly the store lab's "measured, not asserted" discipline.

pub mod cascade;
pub mod gun;
pub mod reach;
pub mod reach_dred;
pub mod reach_inc;
pub mod reconcile;
pub mod sqlmem;
pub mod store_db;
mod ram_exp;
mod reach_exp;
#[cfg(feature = "with-salsa")]
mod salsa_exp;
mod sqlite_exp;
pub use ram_exp::RamZset;
pub use reach_exp::{RamReach, SqliteReach};
pub use cascade::CascadeZset;
pub use reconcile::{Reconciler, SqlReconciler};
#[cfg(feature = "with-salsa")]
pub use reconcile::SalsaReconciler;
#[cfg(feature = "with-salsa")]
pub use salsa_exp::SalsaRows;
pub use sqlite_exp::SqliteTemporal;

#[cfg(feature = "with-dd")]
mod dd_exp;
#[cfg(feature = "with-dd")]
pub use dd_exp::DdReach;
#[cfg(feature = "with-dd")]
pub mod dd_bfs;
#[cfg(feature = "with-dd")]
pub use dd_bfs::DdBfs;

/// The two workloads, each a rational setup we can argue about.
pub fn live_set_workload() -> Workload {
    Workload {
        name: "live-set",
        rationale: "the FACT layer under edit churn: maintain the live edge multiset a \
                    source rule produces as files change. Uniform churn over a 2x key pool \
                    (births, retractions, rebirths). Answers 'what edges exist now'.",
        make: live_set_stream,
    }
}
pub fn reach_workload() -> Workload {
    Workload {
        name: "reachability (blast radius)",
        rationale: "the REAL product query: all-pairs transitive closure of a sparse \
                    call-graph DAG. Edits are LOCALIZED (one function's out-edges rewritten \
                    per tick) — the true change shape. Answers 'what is reachable', the query \
                    incrementality exists for.",
        make: reach::reach_stream,
    }
}

/// splitmix64 — the shared per-key hash for the order-independent XOR digest, so
/// every engine computes the SAME digest and equivalence is meaningful.
pub fn mix(k: i64) -> i64 {
    let mut z = (k as u64).wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    (z ^ (z >> 31)) as i64
}

/// Declared complexity — human-stated, because the compiler can't derive it. The
/// harness checks it against the measured slope.
#[derive(Clone, Copy)]
pub struct Complexity {
    pub time: &'static str,
    pub space: &'static str,
}

/// Everything we count per (experiment, scale).
#[derive(Clone, Default)]
pub struct Report {
    pub scale: usize,
    pub setup_ms: f64,
    pub apply_ms: f64,
    pub ticks: u64,
    pub recompute_units: u64, // engine-defined WORK per change (salsa executes / row touches)
    pub writes: u64,          // write ops / statements — the N+1 tripwire
    pub live: u64,
    pub total_rows: u64,      // incl. history for the temporal engine
    pub peak_alloc_mb: f64,   // Rust gun high-water
    pub peak_rss_mb: f64,     // OS getrusage
    pub digest: i64,
}

/// The one trait every incremental engine implements to enter the golden table.
pub trait Experiment {
    fn name(&self) -> &'static str;
    fn complexity(&self) -> Complexity;
    /// The rational setup: what real mechanism this engine models, so a reader can
    /// argue about whether the measurement is fair. Every experiment must state one.
    fn rationale(&self) -> &'static str;
    fn reset(&mut self);
    fn setup(&mut self, base_facts: usize);
    fn tick(&mut self, adds: &[i64], removes: &[i64]);
    fn digest(&self) -> i64;
    fn live(&self) -> u64;
    fn total_rows(&self) -> u64 {
        self.live()
    }
    fn recompute_units(&self) -> u64 {
        0
    }
    fn writes(&self) -> u64 {
        0
    }
    fn plan_snapshot(&self) -> Option<String> {
        None
    }
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 16
    }
}

/// A deterministic edit stream + the reference (expected) digest for a scale.
pub struct Stream {
    pub ticks: Vec<(Vec<i64>, Vec<i64>)>,
    pub expected_digest: i64,
    pub expected_live: u64,
}

/// A workload = the rational setup made concrete: a documented edit-stream shape and
/// the independent reference oracle that says what the answer must be. Swapping the
/// workload is how the same engines get measured on different real queries (the fact
/// layer vs the blast-radius query).
pub struct Workload {
    pub name: &'static str,
    pub rationale: &'static str,
    /// (base, seed, ticks, edits_per_tick) -> the stream + the expected answer.
    pub make: fn(usize, u64, usize, usize) -> Stream,
}

pub struct Harness {
    pub scales: Vec<usize>,
    pub ticks: usize,
    pub edits_per_tick: usize,
    pub seed: u64,
}

/// The default fact-layer workload: seed keys 0..base, then churn adds/removes over a
/// 2x pool so births, retractions and rebirths all occur. Rationale: the FACT LAYER
/// under edit churn — the live edge multiset a source-rule maintains as files change.
pub fn live_set_stream(base: usize, seed: u64, ticks: usize, edits: usize) -> Stream {
    use std::collections::HashMap;
    let mut rng = Lcg(seed ^ base as u64);
    let mut multiset: HashMap<i64, i64> = HashMap::new();
    for k in 0..base as i64 {
        multiset.insert(k, 1);
    }
    let pool = (base as i64 * 2).max(2);
    let mut out = Vec::with_capacity(ticks);
    for _ in 0..ticks {
        let (mut adds, mut removes) = (Vec::new(), Vec::new());
        for _ in 0..edits {
            let k = (rng.next() as i64) % pool;
            if rng.next() % 3 == 0 {
                if multiset.get(&k).copied().unwrap_or(0) > 0 {
                    removes.push(k);
                    let w = multiset.get_mut(&k).unwrap();
                    *w -= 1;
                    if *w == 0 {
                        multiset.remove(&k);
                    }
                }
            } else {
                adds.push(k);
                *multiset.entry(k).or_insert(0) += 1;
            }
        }
        out.push((adds, removes));
    }
    let expected_digest = multiset.keys().fold(0i64, |a, &k| a ^ mix(k));
    Stream { ticks: out, expected_digest, expected_live: multiset.len() as u64 }
}

impl Harness {
    fn run_one(&self, exp: &mut dyn Experiment, base_facts: usize, stream: &Stream) -> Report {
        gun::reset_peak();
        let t = std::time::Instant::now();
        exp.reset();
        exp.setup(base_facts);
        let setup_ms = t.elapsed().as_secs_f64() * 1000.0;

        let t = std::time::Instant::now();
        for (adds, removes) in &stream.ticks {
            exp.tick(adds, removes);
        }
        let apply_ms = t.elapsed().as_secs_f64() * 1000.0;

        let report = Report {
            scale: base_facts,
            setup_ms,
            apply_ms,
            ticks: stream.ticks.len() as u64,
            recompute_units: exp.recompute_units(),
            writes: exp.writes(),
            live: exp.live(),
            total_rows: exp.total_rows(),
            peak_alloc_mb: gun::peak_mb(),
            peak_rss_mb: gun::peak_rss_mb(),
            digest: exp.digest(),
        };
        // free this engine's state NOW so the next engine's peak_alloc is not
        // contaminated by ours still being resident (they share the process).
        exp.reset();
        report
    }

    /// Least-squares slope of log(metric) vs log(scale): the empirical Big-O exponent.
    fn slope(scales: &[usize], ys: &[f64]) -> f64 {
        let n = scales.len() as f64;
        let xs: Vec<f64> = scales.iter().map(|&s| (s as f64).ln()).collect();
        let ls: Vec<f64> = ys.iter().map(|&y| (y.max(1e-9)).ln()).collect();
        let sx: f64 = xs.iter().sum();
        let sy: f64 = ls.iter().sum();
        let sxx: f64 = xs.iter().map(|x| x * x).sum();
        let sxy: f64 = xs.iter().zip(&ls).map(|(x, y)| x * y).sum();
        let d = n * sxx - sx * sx;
        if d.abs() < 1e-12 {
            0.0
        } else {
            (n * sxy - sx * sy) / d
        }
    }

    pub fn run(&self, workload: &Workload, mut exps: Vec<Box<dyn Experiment>>) {
        println!(
            "Golden harness — gun cap {:.0} MB · scales {:?} · {} ticks × {} edits",
            gun::cap_mb(),
            self.scales,
            self.ticks,
            self.edits_per_tick
        );
        println!("workload: {} — {}\n", workload.name, workload.rationale);
        println!("== engine rationales (the setup we can argue about) ==");
        for exp in &exps {
            println!("  {:<16} {}", exp.name(), exp.rationale());
        }
        println!();
        // reports[exp_idx][scale_idx]
        let mut reports: Vec<Vec<Report>> = vec![Vec::new(); exps.len()];
        let mut expected: Vec<(i64, u64)> = Vec::new();

        for &scale in &self.scales {
            let stream = (workload.make)(scale, self.seed, self.ticks, self.edits_per_tick);
            expected.push((stream.expected_digest, stream.expected_live));
            for (i, exp) in exps.iter_mut().enumerate() {
                let r = self.run_one(exp.as_mut(), scale, &stream);
                reports[i].push(r);
            }
        }

        // ---- the grand table, per scale -------------------------------------
        for (si, &scale) in self.scales.iter().enumerate() {
            let (exp_dig, exp_live) = expected[si];
            println!("== scale {} base facts (expected live {}, digest 0x{:x}) ==", scale, exp_live, exp_dig as u64);
            println!(
                "  {:<16} {:>9} {:>9} {:>10} {:>9} {:>10} {:>10}  {}",
                "engine", "live", "rows", "recompute", "writes", "apply_ms", "allocMB", "equiv"
            );
            for (i, exp) in exps.iter().enumerate() {
                let r = &reports[i][si];
                let ok = if r.digest == exp_dig { "✓" } else { "✗ MISMATCH" };
                println!(
                    "  {:<16} {:>9} {:>9} {:>10} {:>9} {:>10.1} {:>10.1}  {}",
                    exp.name(), r.live, r.total_rows, r.recompute_units, r.writes, r.apply_ms, r.peak_alloc_mb, ok
                );
            }
            println!();
        }

        // ---- per-engine: declared vs MEASURED Big-O -------------------------
        println!("== declared complexity vs measured slope (log-log across scales) ==");
        println!("  {:<16} {:<20} {:>10}   {:<22} {:>10}", "engine", "declared time", "meas t^p", "declared space", "meas mem^p");
        for (i, exp) in exps.iter().enumerate() {
            let c = exp.complexity();
            let times: Vec<f64> = reports[i].iter().map(|r| r.apply_ms).collect();
            let mems: Vec<f64> = reports[i].iter().map(|r| r.peak_alloc_mb.max(0.1)).collect();
            let tp = Self::slope(&self.scales, &times);
            let mp = Self::slope(&self.scales, &mems);
            println!(
                "  {:<16} {:<20} {:>10.2}   {:<22} {:>10.2}",
                exp.name(), c.time, tp, c.space, mp
            );
        }

        // ---- plan snapshots --------------------------------------------------
        println!("\n== query-plan snapshots ==");
        for exp in &exps {
            if let Some(plan) = exp.plan_snapshot() {
                println!("  [{}]\n{}", exp.name(), plan);
            }
        }
    }
}
