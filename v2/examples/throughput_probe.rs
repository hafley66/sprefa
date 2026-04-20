//! throughput_probe — single-shot variant runner for push/pull/dam topology bench.
//!
//! One invocation = one (variant, pattern) run. Emits JSON to stdout so the
//! bash harness can parse. `/usr/bin/time -l` wraps each invocation to get
//! per-variant peak RSS.
//!
//! Usage:
//!   throughput_probe --variant A|B|Cdam:<cap>|Dtokio:<conc>
//!                    --pattern baseline|medium|heavy|nomatch
//!                    [--root <path>] [--max <n>]
//!
//! Output (stdout, one line):
//!   {"variant":"Cdam:256","pattern":"baseline","files":2000,"bytes":8000000,
//!    "matches":272,"wall_ms":106.3}

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

use ast_grep_core::{AstGrep, Language, Pattern, source::StrDoc};
use ast_grep_language::SupportLang;
use futures::stream::{self, StreamExt};
use ignore::WalkBuilder;
use rayon::prelude::*;

fn pattern_src(tag: &str) -> &'static str {
    match tag {
        "baseline" => "console.log($$$)",
        "medium"   => "useEffect(() => { $$$ })",
        "heavy"    => "function $NAME($$$) { $$$ }",
        "nomatch"  => "___never_match_sigil_xyz_$Q___",
        other => panic!("unknown pattern tag: {}", other),
    }
}

fn scan_one(path: &Path, pattern: &Pattern<SupportLang>, lang: SupportLang) -> (u64, u64) {
    let Ok(src) = std::fs::read_to_string(path) else { return (0, 0) };
    let bytes = src.len() as u64;
    let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(&src);
    let matches = grep.root().find_all(pattern).count() as u64;
    (bytes, matches)
}

fn is_js_ts(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|s| s.to_str()),
        Some("js") | Some("jsx") | Some("ts") | Some("tsx") | Some("mjs") | Some("cjs")
    )
}

fn enumerate(root: &Path, cap: Option<usize>) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(64 * 1024);
    for dent in WalkBuilder::new(root).hidden(false).git_ignore(false).build() {
        let Ok(dent) = dent else { continue };
        if !dent.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
        if !is_js_ts(dent.path()) { continue; }
        out.push(dent.path().to_path_buf());
        if let Some(c) = cap { if out.len() >= c { break; } }
    }
    out
}

fn variant_a(paths: &[PathBuf], pat: &Pattern<SupportLang>) -> (u64, u64, u64) {
    let (mut b, mut m) = (0u64, 0u64);
    for p in paths {
        let (bb, mm) = scan_one(p, pat, SupportLang::Tsx);
        b += bb; m += mm;
    }
    (paths.len() as u64, b, m)
}

fn variant_b(root: &Path, pat: Arc<Pattern<SupportLang>>, cap: Option<usize>) -> (u64, u64, u64) {
    let files = Arc::new(AtomicU64::new(0));
    let bytes = Arc::new(AtomicU64::new(0));
    let matches = Arc::new(AtomicU64::new(0));
    let cap_remaining = cap.map(|c| Arc::new(AtomicU64::new(c as u64)));
    WalkBuilder::new(root).hidden(false).git_ignore(false).build_parallel().run(|| {
        let pat = pat.clone();
        let files = files.clone();
        let bytes = bytes.clone();
        let matches = matches.clone();
        let cap_remaining = cap_remaining.clone();
        Box::new(move |dent| {
            use ignore::WalkState;
            let Ok(dent) = dent else { return WalkState::Continue };
            if !dent.file_type().map(|t| t.is_file()).unwrap_or(false) { return WalkState::Continue; }
            if !is_js_ts(dent.path()) { return WalkState::Continue; }
            if let Some(r) = &cap_remaining {
                // fetch_sub wraps past zero; use CAS loop that refuses at 0.
                let took = r.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                    if v == 0 { None } else { Some(v - 1) }
                });
                if took.is_err() { return WalkState::Quit; }
            }
            let (b, m) = scan_one(dent.path(), &pat, SupportLang::Tsx);
            files.fetch_add(1, Ordering::Relaxed);
            bytes.fetch_add(b, Ordering::Relaxed);
            matches.fetch_add(m, Ordering::Relaxed);
            WalkState::Continue
        })
    });
    (files.load(Ordering::Relaxed), bytes.load(Ordering::Relaxed), matches.load(Ordering::Relaxed))
}

fn variant_c(paths: &[PathBuf], pat: Arc<Pattern<SupportLang>>, cap: usize) -> (u64, u64, u64) {
    let (tx, rx) = mpsc::sync_channel::<(u64, u64)>(cap);
    let consumer = std::thread::spawn(move || {
        let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
        for (bb, mm) in rx.iter() { f += 1; b += bb; m += mm; }
        (f, b, m)
    });
    paths.par_iter().for_each_with(tx.clone(), |tx, p| {
        let (b, m) = scan_one(p, &pat, SupportLang::Tsx);
        let _ = tx.send((b, m));
    });
    drop(tx);
    consumer.join().unwrap()
}

async fn variant_d(paths: &[PathBuf], pat: Arc<Pattern<SupportLang>>, conc: usize) -> (u64, u64, u64) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(u64, u64)>(conc * 2);
    // Drain side: folds on the fly so no Vec<> accumulates.
    let collector = tokio::spawn(async move {
        let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
        while let Some((bb, mm)) = rx.recv().await { f += 1; b += bb; m += mm; }
        (f, b, m)
    });
    stream::iter(paths.iter().cloned())
        .map(|p| {
            let pat = pat.clone();
            let tx = tx.clone();
            tokio::task::spawn_blocking(move || {
                let r = scan_one(&p, &pat, SupportLang::Tsx);
                let _ = tx.blocking_send(r);
            })
        })
        .buffer_unordered(conc)
        .for_each(|_| async {}).await;
    drop(tx);
    collector.await.unwrap()
}

// C with batched sends: each par_iter worker collects `batch` results into a
// Vec before one `tx.send(Vec)`. Reduces channel traffic by a factor of `batch`.
fn variant_c_batch(paths: &[PathBuf], pat: Arc<Pattern<SupportLang>>, cap: usize, batch: usize) -> (u64, u64, u64) {
    let (tx, rx) = mpsc::sync_channel::<Vec<(u64, u64)>>(cap);
    let consumer = std::thread::spawn(move || {
        let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
        for chunk in rx.iter() {
            for (bb, mm) in chunk { f += 1; b += bb; m += mm; }
        }
        (f, b, m)
    });
    paths.par_chunks(batch).for_each_with(tx.clone(), |tx, chunk| {
        let mut out = Vec::with_capacity(chunk.len());
        for p in chunk {
            out.push(scan_one(p, &pat, SupportLang::Tsx));
        }
        let _ = tx.send(out);
    });
    drop(tx);
    consumer.join().unwrap()
}

// D with batched spawn_blocking: each async task handles `batch` files.
// Amortizes spawn_blocking thread-hop cost across the batch.
async fn variant_d_batch(paths: &[PathBuf], pat: Arc<Pattern<SupportLang>>, conc: usize, batch: usize) -> (u64, u64, u64) {
    let chunks: Vec<Vec<PathBuf>> = paths.chunks(batch).map(|c| c.to_vec()).collect();
    let results: Vec<(u64, u64, u64)> = stream::iter(chunks)
        .map(|chunk| {
            let pat = pat.clone();
            tokio::task::spawn_blocking(move || {
                let (mut b, mut m) = (0u64, 0u64);
                for p in &chunk {
                    let (bb, mm) = scan_one(p, &pat, SupportLang::Tsx);
                    b += bb; m += mm;
                }
                (chunk.len() as u64, b, m)
            })
        })
        .buffer_unordered(conc)
        .filter_map(|r| async move { r.ok() })
        .collect()
        .await;
    let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
    for (ff, bb, mm) in &results { f += ff; b += bb; m += mm; }
    (f, b, m)
}

// Gopp: opportunistic drain (single consumer). Producer is rayon par_iter
// sending (bytes, matches) per file. Consumer wakes on blocking recv, then
// try_recv drains up to MAX_BATCH. "Batch" here is a bookkeeping unit — we
// count it to validate the arrival-rate self-tuning claim (shallow queue →
// small batches, saturated → MAX_BATCH).
//
// Returns (files, bytes, matches, batch_sizes) where batch_sizes is a Vec of
// per-dispatch drain depths. Caller computes percentiles.
fn variant_g_opp(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    cap: usize,
    max_batch: usize,
) -> (u64, u64, u64, Vec<u32>) {
    let (tx, rx) = mpsc::sync_channel::<(u64, u64)>(cap);
    let consumer = std::thread::spawn(move || {
        let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
        let mut batch_sizes: Vec<u32> = Vec::with_capacity(1024);
        loop {
            let Ok(first) = rx.recv() else { break };
            let mut depth: u32 = 1;
            f += 1; b += first.0; m += first.1;
            while depth < max_batch as u32 {
                match rx.try_recv() {
                    Ok((bb, mm)) => { f += 1; b += bb; m += mm; depth += 1; }
                    Err(_) => break,
                }
            }
            batch_sizes.push(depth);
        }
        (f, b, m, batch_sizes)
    });
    paths.par_iter().for_each_with(tx.clone(), |tx, p| {
        let (b, m) = scan_one(p, &pat, SupportLang::Tsx);
        let _ = tx.send((b, m));
    });
    drop(tx);
    consumer.join().unwrap()
}

// Hopp: opportunistic drain with W consumer workers over an MPMC channel.
// Producer rayon par_iter → crossbeam channel. Each worker runs its own
// recv→try_recv drain loop. Tests whether opportunistic composes with
// consumer fanout (vs serializing on a single recv site).
fn variant_h_opp_mc(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    cap: usize,
    max_batch: usize,
    workers: usize,
) -> (u64, u64, u64, Vec<u32>) {
    let (tx, rx) = crossbeam_channel::bounded::<(u64, u64)>(cap);
    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let rx = rx.clone();
        handles.push(std::thread::spawn(move || {
            let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
            let mut batch_sizes: Vec<u32> = Vec::with_capacity(512);
            loop {
                let Ok(first) = rx.recv() else { break };
                let mut depth: u32 = 1;
                f += 1; b += first.0; m += first.1;
                while depth < max_batch as u32 {
                    match rx.try_recv() {
                        Ok((bb, mm)) => { f += 1; b += bb; m += mm; depth += 1; }
                        Err(_) => break,
                    }
                }
                batch_sizes.push(depth);
            }
            (f, b, m, batch_sizes)
        }));
    }
    drop(rx);
    paths.par_iter().for_each_with(tx.clone(), |tx, p| {
        let (b, m) = scan_one(p, &pat, SupportLang::Tsx);
        let _ = tx.send((b, m));
    });
    drop(tx);
    let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
    let mut batch_sizes: Vec<u32> = Vec::new();
    for h in handles {
        let (ff, bb, mm, mut bs) = h.join().unwrap();
        f += ff; b += bb; m += mm;
        batch_sizes.append(&mut bs);
    }
    (f, b, m, batch_sizes)
}

// Efat: OOM-bait. Materializes (full_src, Vec<match_ranges>) per file into
// one giant Vec before reducing. This is the "materialize everything" anti-
// pattern — Salsa-style memo without bounds, naive par_iter().collect().
fn variant_e_fat(paths: &[PathBuf], pat: Arc<Pattern<SupportLang>>) -> (u64, u64, u64) {
    let results: Vec<(String, Vec<std::ops::Range<usize>>)> = paths
        .par_iter()
        .map(|p| {
            let Ok(src) = std::fs::read_to_string(p) else { return (String::new(), Vec::new()) };
            let grep: AstGrep<StrDoc<SupportLang>> = SupportLang::Tsx.ast_grep(&src);
            let ranges: Vec<_> = grep.root().find_all(&*pat).map(|n| n.range()).collect();
            (src, ranges)
        })
        .collect();
    let mut bytes = 0u64;
    let mut matches = 0u64;
    for (src, rs) in &results {
        bytes += src.len() as u64;
        matches += rs.len() as u64;
    }
    (results.len() as u64, bytes, matches)
}

// ============================================================================
// v2 variants (consumer-side scan). Built to isolate topology from scan work.
// In these variants the producer only enumerates + sends PathBuf; the consumer
// reads + parses + matches. That makes batch size a real dispatch-cost knob.
// Per-dispatch fixed cost is a standalone sleep (no Mutex) so Hopp workers
// still parallelize — the amortization target is the sleep itself, not lock
// serialization.
// ============================================================================

// Work distribution for the consumer's per-item labor. `Real` runs the actual
// ast-grep scan; the synthetic variants let us inject controlled variance.
#[derive(Clone, Copy, Debug)]
enum WorkDist {
    Real,
    Fixed { per_item_us: u64 },
    // shape_x10 avoids float CLI parsing; divide by 10 to get true shape.
    Pareto { median_us: u64, shape_x10: u64 },
}

// Per-dispatch fixed cost, sampled from a distribution. This is the "BEGIN/
// COMMIT paid once per batch" cost in the batching-jutsu model, now with
// controlled variance so we can probe tail sensitivity.
#[derive(Clone, Copy, Debug)]
enum CostSample {
    Fixed(u64),
    Pareto { median_us: u64, shape_x10: u64 },
    Normal { mean_us: u64, sd_us: u64 },
    Exp { mean_us: u64 },
}

#[inline]
fn sample_unit(rng: &mut u64) -> f64 {
    (xorshift64(rng) >> 11) as f64 / (1u64 << 53) as f64
}

fn sample_cost_us(rng: &mut u64, cs: &CostSample) -> u64 {
    match *cs {
        CostSample::Fixed(us) => us,
        CostSample::Pareto { median_us, shape_x10 } =>
            pareto_sample_us(rng, median_us, shape_x10),
        CostSample::Normal { mean_us, sd_us } => {
            // Box-Muller, clamp to nonnegative.
            let u1 = sample_unit(rng).max(1e-12);
            let u2 = sample_unit(rng);
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            (mean_us as f64 + z * sd_us as f64).max(0.0).min(1e9) as u64
        }
        CostSample::Exp { mean_us } => {
            let u = sample_unit(rng).max(1e-12);
            (mean_us as f64 * -u.ln()).min(1e9) as u64
        }
    }
}

// Producer inter-send pacing. Either a per-item distribution sample or a
// bursty pattern (send COUNT items back-to-back, sleep GAP, repeat). Bursty
// models LSP-style interactive arrival (edit event → N dependent requests).
#[derive(Clone, Copy, Debug)]
enum ProducerPace {
    Dist(CostSample),
    Burst { count: u64, gap_us: u64 },
}

fn apply_producer_pace(rng: &mut u64, pace: &ProducerPace, idx: u64) {
    match *pace {
        ProducerPace::Dist(cs) => {
            let us = sample_cost_us(rng, &cs);
            if us > 0 {
                std::thread::sleep(std::time::Duration::from_micros(us));
            }
        }
        ProducerPace::Burst { count, gap_us } => {
            // Sleep only on burst boundaries: after every `count` items.
            if count > 0 && idx > 0 && idx % count == 0 && gap_us > 0 {
                std::thread::sleep(std::time::Duration::from_micros(gap_us));
            }
        }
    }
}

fn parse_cost_sample(spec: &str) -> CostSample {
    if let Some(rest) = spec.strip_prefix("fixed:") {
        CostSample::Fixed(rest.parse().unwrap())
    } else if let Some(rest) = spec.strip_prefix("pareto:") {
        let (m, s) = rest.split_once(':').expect("pareto:MED:SHAPEX10");
        CostSample::Pareto { median_us: m.parse().unwrap(), shape_x10: s.parse().unwrap() }
    } else if let Some(rest) = spec.strip_prefix("normal:") {
        let (m, s) = rest.split_once(':').expect("normal:MEAN:SD");
        CostSample::Normal { mean_us: m.parse().unwrap(), sd_us: s.parse().unwrap() }
    } else if let Some(rest) = spec.strip_prefix("exp:") {
        CostSample::Exp { mean_us: rest.parse().unwrap() }
    } else {
        panic!("unknown cost-sample spec: {} (expect fixed:N | pareto:M:S | normal:M:S | exp:M)", spec);
    }
}

// How per-batch effect cost scales with batch size. The point of carrying
// this as a knob (rather than just per-item work) is to probe both regimes
// in one harness:
//
//   Linear       cost(N) = fixed + N * per_item
//     Batching neither helps nor hurts amortization; only pays off through
//     amortizing `fixed`. If per_item is big and fixed is small, batching
//     actively hurts wall time because a worker holds the batch serially
//     while other workers go idle (straggler effect).
//
//   Flat         cost(N) = fixed + flat_us
//     Group-commit / DataLoader / RPC-coalesce shape. The whole batch
//     completes in a batch-size-independent time. Classic batching win.
//     Bigger B = more items processed per dispatch = pure throughput gain.
//
//   Quadratic    cost(N) = fixed + per-item work + coeff * N^2 / 1000
//     Cache-pressure / memory-alloc / coordination-overhead shape. Large
//     batches introduce super-linear cost. The crossover point is where
//     `coeff * N^2 / 1000` exceeds `fixed` amortization saved.
//
// WorkDist and fixed_cost compose with all three shapes — set per-item work
// and per-dispatch setup separately, and choose the batch shape to model
// what the effect-layer cost actually looks like.
#[derive(Clone, Copy, Debug)]
enum BatchShape {
    Linear,
    Flat { flat_us: u64 },
    Quadratic { coeff_us: u64 },
}

#[derive(Clone, Copy)]
struct DispatchCfg {
    fixed_cost: CostSample,
    work: WorkDist,
    batch_shape: BatchShape,
}

// Xorshift64*: fast non-crypto RNG. Per-consumer state avoids sync.
#[inline]
fn xorshift64(s: &mut u64) -> u64 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

// Guarded seed mixer: xorshift traps on state 0. Any input that xors to zero
// with its site-constant would otherwise produce an all-zero sample stream
// silently. `| 1` also breaks the thinner class where state becomes 0 via the
// shift chain. Every RNG init in this file goes through here.
#[inline]
fn mix_seed(base: u64, tag: u64) -> u64 {
    let s = base ^ tag;
    if s == 0 { tag.wrapping_add(0xA5A5_A5A5_A5A5_A5A5) | 1 } else { s | 1 }
}

// Per-variant tags chosen to be distinct so w_idx=0 Hopp2 does not alias Gopp2
// or Cdam2 under the same seed_base (noted in session 4 audit).
const TAG_SHUFFLE: u64 = 0x1234_5678_90AB_CDEF;
const TAG_CDAM2:   u64 = 0xCDA2_CDA2_CDA2_CDA2;
const TAG_GOPP2:   u64 = 0x0097_0097_0097_0097;
const TAG_HOPP2:   u64 = 0x8097_8097_8097_8097;
const TAG_JOPP:    u64 = 0xD0BB_D0BB_D0BB_D0BB;
const TAG_CDAM2BUF:    u64 = 0xCB2B_CB2B_CB2B_CB2B;
const TAG_RXRUST1:     u64 = 0x5252_5252_5252_5252;
const TAG_RXRUST1_BUF: u64 = 0x524B_524B_524B_524B;

// Pareto: x = xm / (1-U)^(1/alpha), with xm chosen so median == median_us.
// Median of Pareto(xm, alpha) = xm * 2^(1/alpha), so xm = median / 2^(1/alpha).
#[inline]
fn pareto_sample_us(rng: &mut u64, median_us: u64, shape_x10: u64) -> u64 {
    let u = (xorshift64(rng) >> 11) as f64 / (1u64 << 53) as f64;
    let alpha = (shape_x10.max(1) as f64) / 10.0;
    let xm = (median_us as f64) / 2f64.powf(1.0 / alpha);
    // Guard against U==0 → 0 exponent; clamp 1-U to a floor.
    let denom = (1.0 - u).max(1e-12);
    let x = xm * denom.powf(-1.0 / alpha);
    x.min(1.0e9) as u64 // cap at 1s so a pathological tail sample does not hang
}

fn dispatch_batch_v3(
    batch: &[PathBuf],
    pat: &Pattern<SupportLang>,
    cfg: &DispatchCfg,
    rng: &mut u64,
) -> (u64, u64, u64) {
    let t0 = Instant::now();
    let fc = sample_cost_us(rng, &cfg.fixed_cost);
    if fc > 0 {
        std::thread::sleep(std::time::Duration::from_micros(fc));
    }
    let (mut b, mut m) = (0u64, 0u64);
    let n = batch.len() as u64;

    // Flat shape short-circuits the per-item work loop entirely: one sleep
    // represents the whole batch (group-commit semantics). Real items are
    // still scanned under WorkDist::Real so byte/match counts stay honest.
    if let BatchShape::Flat { flat_us } = cfg.batch_shape {
        if matches!(cfg.work, WorkDist::Real) {
            for p in batch {
                let (bb, mm) = scan_one(p, pat, SupportLang::Tsx);
                b += bb; m += mm;
            }
        }
        if flat_us > 0 {
            std::thread::sleep(std::time::Duration::from_micros(flat_us));
        }
        let dispatch_us = t0.elapsed().as_micros() as u64;
        return (b, m, dispatch_us);
    }

    match cfg.work {
        WorkDist::Real => {
            for p in batch {
                let (bb, mm) = scan_one(p, pat, SupportLang::Tsx);
                b += bb; m += mm;
            }
        }
        WorkDist::Fixed { per_item_us } => {
            for _ in batch {
                if per_item_us > 0 {
                    std::thread::sleep(std::time::Duration::from_micros(per_item_us));
                }
            }
        }
        WorkDist::Pareto { median_us, shape_x10 } => {
            for _ in batch {
                let us = pareto_sample_us(rng, median_us, shape_x10);
                if us > 0 {
                    std::thread::sleep(std::time::Duration::from_micros(us));
                }
            }
        }
    }
    // Quadratic overhead: coeff µs per N² items / 1000. Models cache pressure,
    // large-Vec alloc, coordination cost. Grows faster than any amortization
    // savings past the crossover batch size.
    if let BatchShape::Quadratic { coeff_us, .. } = cfg.batch_shape {
        let overhead = coeff_us.saturating_mul(n).saturating_mul(n) / 1000;
        if overhead > 0 {
            std::thread::sleep(std::time::Duration::from_micros(overhead));
        }
    }
    let dispatch_us = t0.elapsed().as_micros() as u64;
    (b, m, dispatch_us)
}

// Seeded Fisher-Yates. Deterministic given (paths, seed); different seeds
// produce different orderings for the same path set. Used for Monte Carlo
// across-run variance studies where the workload is held fixed but the
// consumer sees items in varying order.
fn shuffle_paths_seeded(paths: &mut Vec<PathBuf>, seed: u64) {
    let mut rng = mix_seed(seed, TAG_SHUFFLE);
    let n = paths.len();
    for i in (1..n).rev() {
        let j = (xorshift64(&mut rng) as usize) % (i + 1);
        paths.swap(i, j);
    }
}

fn variant_cdam2(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    cap: usize,
    cfg: DispatchCfg,
    seed_base: u64,
    producer_pace: ProducerPace,
) -> (u64, u64, u64, Vec<u32>, Vec<u64>) {
    let (tx, rx) = crossbeam_channel::bounded::<PathBuf>(cap);
    let pat_c = pat.clone();
    let consumer = std::thread::spawn(move || {
        let mut batch_sizes: Vec<u32> = Vec::with_capacity(1024);
        let mut dispatch_us_vec: Vec<u64> = Vec::with_capacity(1024);
        let mut rng = mix_seed(seed_base, TAG_CDAM2);
        let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
        while let Ok(p) = rx.recv() {
            let (bb, mm, du) = dispatch_batch_v3(std::slice::from_ref(&p), &pat_c, &cfg, &mut rng);
            f += 1; b += bb; m += mm;
            batch_sizes.push(1);
            dispatch_us_vec.push(du);
        }
        (f, b, m, batch_sizes, dispatch_us_vec)
    });
    let mut prng = mix_seed(seed_base, 0xF00D_F00D_F00D_F00D);
    for (i, p) in paths.iter().enumerate() {
        apply_producer_pace(&mut prng, &producer_pace, i as u64);
        let _ = tx.send(p.clone());
    }
    drop(tx);
    consumer.join().unwrap()
}

fn variant_gopp2(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    cap: usize,
    max_batch: usize,
    cfg: DispatchCfg,
    seed_base: u64,
    producer_pace: ProducerPace,
) -> (u64, u64, u64, Vec<u32>, Vec<u64>) {
    let (tx, rx) = crossbeam_channel::bounded::<PathBuf>(cap);
    let pat_c = pat.clone();
    let consumer = std::thread::spawn(move || {
        let mut batch_sizes: Vec<u32> = Vec::with_capacity(1024);
        let mut dispatch_us_vec: Vec<u64> = Vec::with_capacity(1024);
        let mut rng = mix_seed(seed_base, TAG_GOPP2);
        let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
        let mut batch: Vec<PathBuf> = Vec::with_capacity(max_batch);
        loop {
            let Ok(first) = rx.recv() else { break };
            batch.clear();
            batch.push(first);
            while batch.len() < max_batch {
                match rx.try_recv() {
                    Ok(p) => batch.push(p),
                    Err(_) => break,
                }
            }
            let depth = batch.len() as u32;
            let (bb, mm, du) = dispatch_batch_v3(&batch, &pat_c, &cfg, &mut rng);
            f += depth as u64; b += bb; m += mm;
            batch_sizes.push(depth);
            dispatch_us_vec.push(du);
        }
        (f, b, m, batch_sizes, dispatch_us_vec)
    });
    let mut prng = mix_seed(seed_base, 0xF00D_F00D_F00D_F00D);
    for (i, p) in paths.iter().enumerate() {
        apply_producer_pace(&mut prng, &producer_pace, i as u64);
        let _ = tx.send(p.clone());
    }
    drop(tx);
    consumer.join().unwrap()
}

fn variant_hopp2(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    cap: usize,
    max_batch: usize,
    workers: usize,
    cfg: DispatchCfg,
    seed_base: u64,
    producer_pace: ProducerPace,
) -> (u64, u64, u64, Vec<u32>, Vec<u64>) {
    let (tx, rx) = crossbeam_channel::bounded::<PathBuf>(cap);
    let mut handles = Vec::with_capacity(workers);
    for w_idx in 0..workers {
        let rx = rx.clone();
        let pat = pat.clone();
        let cfg = cfg;
        // w_idx+1 so worker 0 does not land on seed_base unchanged.
        let worker_tag = TAG_HOPP2 ^ ((w_idx as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let worker_seed = mix_seed(seed_base, worker_tag);
        handles.push(std::thread::spawn(move || {
            let mut batch_sizes: Vec<u32> = Vec::with_capacity(512);
            let mut dispatch_us_vec: Vec<u64> = Vec::with_capacity(512);
            let mut rng = worker_seed;
            let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
            let mut batch: Vec<PathBuf> = Vec::with_capacity(max_batch);
            loop {
                let Ok(first) = rx.recv() else { break };
                batch.clear();
                batch.push(first);
                while batch.len() < max_batch {
                    match rx.try_recv() {
                        Ok(p) => batch.push(p),
                        Err(_) => break,
                    }
                }
                let depth = batch.len() as u32;
                let (bb, mm, du) = dispatch_batch_v3(&batch, &pat, &cfg, &mut rng);
                f += depth as u64; b += bb; m += mm;
                batch_sizes.push(depth);
                dispatch_us_vec.push(du);
            }
            (f, b, m, batch_sizes, dispatch_us_vec)
        }));
    }
    drop(rx);
    let mut prng = mix_seed(seed_base, 0xF00D_F00D_F00D_F00D);
    for (i, p) in paths.iter().enumerate() {
        apply_producer_pace(&mut prng, &producer_pace, i as u64);
        let _ = tx.send(p.clone());
    }
    drop(tx);
    let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
    let mut batch_sizes: Vec<u32> = Vec::new();
    let mut dispatch_us_vec: Vec<u64> = Vec::new();
    for h in handles {
        let (ff, bb, mm, mut bs, mut du) = h.join().unwrap();
        f += ff; b += bb; m += mm;
        batch_sizes.append(&mut bs);
        dispatch_us_vec.append(&mut du);
    }
    (f, b, m, batch_sizes, dispatch_us_vec)
}

// Cdam2Buf: Cdam2 topology with fixed-window Vec buffer instead of
// opportunistic drain. Buffers exactly max_batch items before dispatch (with
// a final partial flush on channel close). 1-for-1 batching-model match
// against Rxrust1Buf, so delta isolates rxrust layer cost.
fn variant_cdam2_buf(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    cap: usize,
    max_batch: usize,
    cfg: DispatchCfg,
    seed_base: u64,
    producer_pace: ProducerPace,
) -> (u64, u64, u64, Vec<u32>, Vec<u64>) {
    let (tx, rx) = crossbeam_channel::bounded::<PathBuf>(cap);
    let pat_c = pat.clone();
    let consumer = std::thread::spawn(move || {
        let mut batch_sizes: Vec<u32> = Vec::with_capacity(1024);
        let mut dispatch_us_vec: Vec<u64> = Vec::with_capacity(1024);
        let mut rng = mix_seed(seed_base, TAG_CDAM2BUF);
        let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
        let mut batch: Vec<PathBuf> = Vec::with_capacity(max_batch);
        while let Ok(p) = rx.recv() {
            batch.push(p);
            if batch.len() >= max_batch {
                let depth = batch.len() as u32;
                let (bb, mm, du) = dispatch_batch_v3(&batch, &pat_c, &cfg, &mut rng);
                f += depth as u64; b += bb; m += mm;
                batch_sizes.push(depth);
                dispatch_us_vec.push(du);
                batch.clear();
            }
        }
        if !batch.is_empty() {
            let depth = batch.len() as u32;
            let (bb, mm, du) = dispatch_batch_v3(&batch, &pat_c, &cfg, &mut rng);
            f += depth as u64; b += bb; m += mm;
            batch_sizes.push(depth);
            dispatch_us_vec.push(du);
        }
        (f, b, m, batch_sizes, dispatch_us_vec)
    });
    let mut prng = mix_seed(seed_base, 0xF00D_F00D_F00D_F00D);
    for (i, p) in paths.iter().enumerate() {
        apply_producer_pace(&mut prng, &producer_pace, i as u64);
        let _ = tx.send(p.clone());
    }
    drop(tx);
    consumer.join().unwrap()
}

// Rxrust1: Cdam2 topology with rxrust on the consumer side.
// Thread boundary = crossbeam (same as Cdam2), so producer tax is identical.
// Each recv fires subject.next(p) into a rxrust Local::subject; subscriber
// runs dispatch_batch_v3 for that single item. Head-to-head vs Cdam2 isolates
// per-emission rxrust cost (boxing, observer indirection).
fn variant_rxrust1(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    cap: usize,
    cfg: DispatchCfg,
    seed_base: u64,
    producer_pace: ProducerPace,
) -> (u64, u64, u64, Vec<u32>, Vec<u64>) {
    let (tx, rx) = crossbeam_channel::bounded::<PathBuf>(cap);
    let pat_c = pat.clone();
    let consumer = std::thread::spawn(move || {
        use rxrust::prelude::*;
        use std::cell::RefCell;
        use std::convert::Infallible;
        use std::rc::Rc;

        type Stats = (u64, u64, u64, Vec<u32>, Vec<u64>);
        let stats: Rc<RefCell<Stats>> =
            Rc::new(RefCell::new((0, 0, 0, Vec::with_capacity(1024), Vec::with_capacity(1024))));
        let rng: Rc<RefCell<u64>> = Rc::new(RefCell::new(mix_seed(seed_base, TAG_RXRUST1)));
        let subject = Local::subject::<PathBuf, Infallible>();
        let sub = {
            let stats = stats.clone();
            let rng = rng.clone();
            let pat = pat_c.clone();
            subject.clone().subscribe(move |p: PathBuf| {
                let mut r = rng.borrow_mut();
                let (bb, mm, du) = dispatch_batch_v3(std::slice::from_ref(&p), &pat, &cfg, &mut *r);
                drop(r);
                let mut s = stats.borrow_mut();
                s.0 += 1;
                s.1 += bb;
                s.2 += mm;
                s.3.push(1);
                s.4.push(du);
            })
        };
        while let Ok(p) = rx.recv() {
            subject.clone().next(p);
        }
        subject.clone().complete();
        drop(sub);
        drop(subject);
        let mut s = stats.borrow_mut();
        (s.0, s.1, s.2, std::mem::take(&mut s.3), std::mem::take(&mut s.4))
    });
    let mut prng = mix_seed(seed_base, 0xF00D_F00D_F00D_F00D);
    for (i, p) in paths.iter().enumerate() {
        apply_producer_pace(&mut prng, &producer_pace, i as u64);
        let _ = tx.send(p.clone());
    }
    drop(tx);
    consumer.join().unwrap()
}

// Rxrust1Buf: Cdam2 topology + rxrust buffer_count(max_batch) on consumer.
// Fixed-window batching (closes at exactly max_batch, plus a final partial on
// complete). Head-to-head vs Hopp2:cap:max_batch:1 isolates fixed-window
// buffer tax vs opportunistic drain.
fn variant_rxrust1_buf(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    cap: usize,
    max_batch: usize,
    cfg: DispatchCfg,
    seed_base: u64,
    producer_pace: ProducerPace,
) -> (u64, u64, u64, Vec<u32>, Vec<u64>) {
    let (tx, rx) = crossbeam_channel::bounded::<PathBuf>(cap);
    let pat_c = pat.clone();
    let consumer = std::thread::spawn(move || {
        use rxrust::prelude::*;
        use std::cell::RefCell;
        use std::convert::Infallible;
        use std::rc::Rc;

        type Stats = (u64, u64, u64, Vec<u32>, Vec<u64>);
        let stats: Rc<RefCell<Stats>> =
            Rc::new(RefCell::new((0, 0, 0, Vec::with_capacity(1024), Vec::with_capacity(1024))));
        let rng: Rc<RefCell<u64>> = Rc::new(RefCell::new(mix_seed(seed_base, TAG_RXRUST1_BUF)));
        let subject = Local::subject::<PathBuf, Infallible>();
        let sub = {
            let stats = stats.clone();
            let rng = rng.clone();
            let pat = pat_c.clone();
            subject.clone().buffer_count(max_batch).subscribe(move |batch: Vec<PathBuf>| {
                let depth = batch.len() as u32;
                let mut r = rng.borrow_mut();
                let (bb, mm, du) = dispatch_batch_v3(&batch, &pat, &cfg, &mut *r);
                drop(r);
                let mut s = stats.borrow_mut();
                s.0 += depth as u64;
                s.1 += bb;
                s.2 += mm;
                s.3.push(depth);
                s.4.push(du);
            })
        };
        while let Ok(p) = rx.recv() {
            subject.clone().next(p);
        }
        subject.clone().complete();
        drop(sub);
        drop(subject);
        let mut s = stats.borrow_mut();
        (s.0, s.1, s.2, std::mem::take(&mut s.3), std::mem::take(&mut s.4))
    });
    let mut prng = mix_seed(seed_base, 0xF00D_F00D_F00D_F00D);
    for (i, p) in paths.iter().enumerate() {
        apply_producer_pace(&mut prng, &producer_pace, i as u64);
        let _ = tx.send(p.clone());
    }
    drop(tx);
    consumer.join().unwrap()
}

// Jopp: two-channel priority select. Simulates LSP-hover (hi) coexisting
// with bulk-scan (lo) on a single consumer. Biased select prefers hi. Both
// channels drain opportunistically up to max_batch. Hi items carry a send
// timestamp so we can report hi-latency percentiles — the real ask is
// "does the interactive path stay fast under bulk saturation?"
//
// Producer: first `hi_count` paths go into hi channel, one every
// producer_delay_us (simulates user interactions). Rest go into lo channel
// as fast as possible.
fn variant_jopp(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    hi_cap: usize,
    lo_cap: usize,
    max_batch: usize,
    hi_count: usize,
    hi_interval_us: u64,
    fixed_cost_us: u64,
) -> (u64, u64, u64, Vec<u32>, Vec<u64>) {
    use std::time::Instant;
    // Channel item carries an optional send-timestamp (None for lo).
    type Item = (PathBuf, Option<Instant>);
    let (hi_tx, hi_rx) = crossbeam_channel::bounded::<Item>(hi_cap);
    let (lo_tx, lo_rx) = crossbeam_channel::bounded::<Item>(lo_cap);

    let pat_c = pat.clone();
    let consumer = std::thread::spawn(move || {
        use crossbeam_channel::TryRecvError;
        let mut batch_sizes: Vec<u32> = Vec::with_capacity(1024);
        let mut hi_latencies_us: Vec<u64> = Vec::with_capacity(1024);
        let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
        let mut batch: Vec<Item> = Vec::with_capacity(max_batch);
        let mut hi_done = false;
        let mut lo_done = false;
        let cfg = DispatchCfg { fixed_cost: CostSample::Fixed(fixed_cost_us), work: WorkDist::Real, batch_shape: BatchShape::Linear };
        let mut rng = mix_seed(0, TAG_JOPP);
        loop {
            if hi_done && lo_done { break; }

            // Priority: always probe hi first (non-blocking).
            let first: Option<Item> = if !hi_done {
                match hi_rx.try_recv() {
                    Ok(item) => Some(item),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => { hi_done = true; None }
                }
            } else { None };

            let (from_hi, first) = if let Some(f) = first {
                (true, f)
            } else {
                // Hi is empty (or dead). Block for lo. If lo also dead, exit.
                if lo_done {
                    // Both can be dead here. One more poll of hi in case of
                    // disconnect-after-empty; otherwise exit.
                    continue;
                }
                match lo_rx.recv() {
                    Ok(item) => (false, item),
                    Err(_) => { lo_done = true; continue; }
                }
            };
            batch.clear();
            batch.push(first);
            // Drain the SAME channel that produced first. Don't mix.
            let source = if from_hi { &hi_rx } else { &lo_rx };
            while batch.len() < max_batch {
                match source.try_recv() {
                    Ok(p) => batch.push(p),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if from_hi { hi_done = true; } else { lo_done = true; }
                        break;
                    }
                }
            }
            let depth = batch.len() as u32;
            let now = Instant::now();
            if from_hi {
                for (_, ts) in &batch {
                    if let Some(sent) = ts {
                        hi_latencies_us.push(now.duration_since(*sent).as_micros() as u64);
                    }
                }
            }
            let only_paths: Vec<PathBuf> = batch.iter().map(|(p, _)| p.clone()).collect();
            let (bb, mm, _du) = dispatch_batch_v3(&only_paths, &pat_c, &cfg, &mut rng);
            f += depth as u64; b += bb; m += mm;
            batch_sizes.push(depth);
        }
        (f, b, m, batch_sizes, hi_latencies_us)
    });

    // Producer: two threads. Hi throttled, lo as-fast-as-possible.
    let hi_paths: Vec<PathBuf> = paths.iter().take(hi_count).cloned().collect();
    let lo_paths: Vec<PathBuf> = paths.iter().skip(hi_count).cloned().collect();
    let hi_handle = std::thread::spawn(move || {
        for p in hi_paths {
            if hi_interval_us > 0 {
                std::thread::sleep(std::time::Duration::from_micros(hi_interval_us));
            }
            let _ = hi_tx.send((p, Some(Instant::now())));
        }
        drop(hi_tx);
    });
    let lo_handle = std::thread::spawn(move || {
        for p in lo_paths {
            let _ = lo_tx.send((p, None));
        }
        drop(lo_tx);
    });
    hi_handle.join().unwrap();
    lo_handle.join().unwrap();
    consumer.join().unwrap()
}

fn main() {
    // Dead-simple CLI parse; no clap to keep the binary small and startup cheap.
    let mut variant: Option<String> = None;
    let mut pattern_tag: Option<String> = None;
    let mut root = PathBuf::from("tests/smoke/.fixtures/swc");
    let mut max_files: Option<usize> = None;
    let mut fixed_cost_us: u64 = 0;
    let mut producer_delay_us: u64 = 0;
    let mut producer_pace_spec: Option<String> = None;
    let mut producer_burst: Option<(u64, u64)> = None;
    let mut rayon_threads: Option<usize> = None;
    let mut hi_count: usize = 100;
    let mut hi_interval_us: u64 = 1000;
    let mut seed: u64 = 0xDEAD_BEEF_CAFE_BABE;
    let mut work_dist: WorkDist = WorkDist::Real;
    let mut cost_dist_spec: Option<String> = None;
    let mut shuffle: bool = false;
    let mut trials: usize = 1;
    let mut tsv: bool = false;
    let mut no_chart: bool = false;
    let mut batch_shape: BatchShape = BatchShape::Linear;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--variant" => { variant = Some(args[i + 1].clone()); i += 2; }
            "--pattern" => { pattern_tag = Some(args[i + 1].clone()); i += 2; }
            "--root"    => { root = PathBuf::from(&args[i + 1]); i += 2; }
            "--max"     => { max_files = args[i + 1].parse().ok(); i += 2; }
            "--fixed-cost-us"     => { fixed_cost_us = args[i + 1].parse().unwrap(); i += 2; }
            "--producer-delay-us" => { producer_delay_us = args[i + 1].parse().unwrap(); i += 2; }
            "--producer-pace"     => { producer_pace_spec = Some(args[i + 1].clone()); i += 2; }
            "--producer-burst"    => {
                let spec = &args[i + 1];
                let (c, g) = spec.split_once(':').expect("--producer-burst COUNT:GAP_US");
                producer_burst = Some((c.parse().unwrap(), g.parse().unwrap()));
                i += 2;
            }
            "--rayon-threads"     => { rayon_threads = args[i + 1].parse().ok(); i += 2; }
            "--hi-count"          => { hi_count = args[i + 1].parse().unwrap(); i += 2; }
            "--hi-interval-us"    => { hi_interval_us = args[i + 1].parse().unwrap(); i += 2; }
            "--seed"              => { seed = args[i + 1].parse().unwrap(); i += 2; }
            "--work-dist"         => {
                let spec = &args[i + 1];
                work_dist = if spec == "real" {
                    WorkDist::Real
                } else if let Some(rest) = spec.strip_prefix("fixed:") {
                    WorkDist::Fixed { per_item_us: rest.parse().unwrap() }
                } else if let Some(rest) = spec.strip_prefix("pareto:") {
                    let (med, sh) = rest.split_once(':').expect("pareto:MEDIAN:SHAPEX10");
                    WorkDist::Pareto {
                        median_us: med.parse().unwrap(),
                        shape_x10: sh.parse().unwrap(),
                    }
                } else {
                    panic!("unknown --work-dist: {}", spec);
                };
                i += 2;
            }
            "--fixed-cost-dist"   => { cost_dist_spec = Some(args[i + 1].clone()); i += 2; }
            "--shuffle"           => { shuffle = true; i += 1; }
            "--trials"            => { trials = args[i + 1].parse::<usize>().unwrap().max(1); i += 2; }
            "--tsv"               => { tsv = true; i += 1; }
            "--no-chart"          => { no_chart = true; i += 1; }
            "--batch-shape"       => {
                let spec = &args[i + 1];
                batch_shape = if spec == "linear" {
                    BatchShape::Linear
                } else if let Some(rest) = spec.strip_prefix("flat:") {
                    BatchShape::Flat { flat_us: rest.parse().unwrap() }
                } else if let Some(rest) = spec.strip_prefix("quad:") {
                    BatchShape::Quadratic { coeff_us: rest.parse().unwrap() }
                } else {
                    panic!("unknown --batch-shape: {} (expect linear | flat:US | quad:COEFF)", spec);
                };
                i += 2;
            }
            other => panic!("unknown arg: {}", other),
        }
    }
    let variant = variant.expect("--variant required");
    let pattern_tag = pattern_tag.expect("--pattern required");

    if let Some(n) = rayon_threads {
        rayon::ThreadPoolBuilder::new().num_threads(n).build_global().ok();
    }

    let lang = SupportLang::Tsx;
    let src = pattern_src(&pattern_tag);
    let pat = Arc::new(Pattern::try_new(src, lang).expect("pattern compile"));

    let mut paths = enumerate(&root, max_files);
    if shuffle {
        shuffle_paths_seeded(&mut paths, seed);
    }

    // Full cache warm: read every file once, discard. Amortized across many
    // invocations by page cache, but we pay first-process cost anyway.
    let mut _warm = 0u64;
    for p in &paths {
        if let Ok(b) = std::fs::read(p) { _warm += b.len() as u64; }
    }

    // Warm ast-grep grammar so parse JIT cost is not in the timed path.
    if let Some(p) = paths.first() { let _ = scan_one(p, &pat, lang); }

    // Only build tokio runtime if the variant actually uses it. Saves ~8
    // extra worker threads when running Cdam/Gopp/Hopp variants.
    let rt = if variant.starts_with("Dtokio:") || variant.starts_with("Dbatch:") {
        Some(tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap())
    } else {
        None
    };

    // Jopp has its own output shape (hi-latency percentiles). Special-case
    // it before the standard warmup/time/emit path.
    if let Some(rest) = variant.strip_prefix("Jopp:") {
        let parts: Vec<&str> = rest.split(':').collect();
        assert_eq!(parts.len(), 3, "Jopp:<hi_cap>:<lo_cap>:<max_batch>");
        let hi_cap: usize = parts[0].parse().unwrap();
        let lo_cap: usize = parts[1].parse().unwrap();
        let mb: usize = parts[2].parse().unwrap();

        // Warmup once at 2k cap with same knobs.
        let warmup_n = paths.len().min(2000);
        let warmup_paths: Vec<PathBuf> = paths.iter().take(warmup_n).cloned().collect();
        let _ = variant_jopp(&warmup_paths, pat.clone(), hi_cap, lo_cap, mb,
                             hi_count.min(warmup_n / 4), hi_interval_us, fixed_cost_us);

        let t0 = Instant::now();
        let (files, bytes, matches, batch_sizes, hi_latencies) =
            variant_jopp(&paths, pat.clone(), hi_cap, lo_cap, mb,
                         hi_count.min(paths.len()), hi_interval_us, fixed_cost_us);
        let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;

        let pct = |mut v: Vec<u64>, q: f64| -> u64 {
            if v.is_empty() { return 0; }
            v.sort_unstable();
            v[((v.len() as f64 - 1.0) * q).round() as usize]
        };
        let pct32 = |mut v: Vec<u32>, q: f64| -> u32 {
            if v.is_empty() { return 0; }
            v.sort_unstable();
            v[((v.len() as f64 - 1.0) * q).round() as usize]
        };
        let hi_count_seen = hi_latencies.len();
        let hi_p50 = pct(hi_latencies.clone(), 0.50);
        let hi_p95 = pct(hi_latencies.clone(), 0.95);
        let hi_p99 = pct(hi_latencies.clone(), 0.99);
        let hi_max = hi_latencies.iter().copied().max().unwrap_or(0);
        let b_p50 = pct32(batch_sizes.clone(), 0.50);
        let b_p95 = pct32(batch_sizes.clone(), 0.95);
        let b_max = batch_sizes.iter().copied().max().unwrap_or(0);

        println!(
            "{{\"variant\":\"{}\",\"pattern\":\"{}\",\"files\":{},\"bytes\":{},\"matches\":{},\"wall_ms\":{:.3},\"fixed_cost_us\":{},\"hi_count\":{},\"hi_interval_us\":{},\"hi_p50_us\":{},\"hi_p95_us\":{},\"hi_p99_us\":{},\"hi_max_us\":{},\"batch_p50\":{},\"batch_p95\":{},\"batch_max\":{}}}",
            variant, pattern_tag, files, bytes, matches, wall_ms,
            fixed_cost_us, hi_count_seen, hi_interval_us,
            hi_p50, hi_p95, hi_p99, hi_max,
            b_p50, b_p95, b_max
        );
        return;
    }

    // Build cost sampler: --fixed-cost-dist overrides --fixed-cost-us.
    let fixed_cost = if let Some(spec) = cost_dist_spec.as_deref() {
        if let Some(rest) = spec.strip_prefix("fixed:") {
            CostSample::Fixed(rest.parse().unwrap())
        } else if let Some(rest) = spec.strip_prefix("pareto:") {
            let (med, sh) = rest.split_once(':').expect("pareto:MED:SHAPEX10");
            CostSample::Pareto { median_us: med.parse().unwrap(), shape_x10: sh.parse().unwrap() }
        } else if let Some(rest) = spec.strip_prefix("normal:") {
            let (mean, sd) = rest.split_once(':').expect("normal:MEAN:SD");
            CostSample::Normal { mean_us: mean.parse().unwrap(), sd_us: sd.parse().unwrap() }
        } else if let Some(rest) = spec.strip_prefix("exp:") {
            CostSample::Exp { mean_us: rest.parse().unwrap() }
        } else {
            panic!("unknown --fixed-cost-dist: {}", spec);
        }
    } else {
        CostSample::Fixed(fixed_cost_us)
    };
    let cfg = DispatchCfg { fixed_cost, work: work_dist, batch_shape };

    // Resolve producer pacing. Priority: --producer-burst > --producer-pace >
    // --producer-delay-us (the old fixed-delay flag). Defaults to Fixed(0).
    let producer_pace: ProducerPace = if let Some((c, g)) = producer_burst {
        ProducerPace::Burst { count: c, gap_us: g }
    } else if let Some(spec) = producer_pace_spec.as_deref() {
        ProducerPace::Dist(parse_cost_sample(spec))
    } else {
        ProducerPace::Dist(CostSample::Fixed(producer_delay_us))
    };
    let producer_pace_str = match producer_pace {
        ProducerPace::Dist(CostSample::Fixed(us)) => format!("fixed:{}", us),
        ProducerPace::Dist(CostSample::Pareto { median_us, shape_x10 }) =>
            format!("pareto:{}:{}", median_us, shape_x10),
        ProducerPace::Dist(CostSample::Normal { mean_us, sd_us }) =>
            format!("normal:{}:{}", mean_us, sd_us),
        ProducerPace::Dist(CostSample::Exp { mean_us }) => format!("exp:{}", mean_us),
        ProducerPace::Burst { count, gap_us } => format!("burst:{}:{}", count, gap_us),
    };

    // Warm the executor. Warmup uses the SAME variant on a subset of paths —
    // at least min(paths.len(), 2000) so rayon's internal work-stealing queues
    // reach steady state.
    let warmup_n = paths.len().min(2000);
    let warmup_paths: Vec<PathBuf> = paths.iter().take(warmup_n).cloned().collect();
    run_once(
        &variant, &root, &warmup_paths, pat.clone(),
        max_files.map(|_| warmup_n), rt.as_ref(),
        cfg, seed, producer_pace,
    );

    // Multi-trial. Each trial reshuffles with seed+i (if --shuffle) so the
    // consumer sees a fresh ordering; RNG site-tags keep topology seeds
    // distinct. Wall + per-trial dispatch percentiles collected for aggregate.
    struct Trial {
        wall_ms: f64,
        files: u64,
        bytes: u64,
        matches: u64,
        b_p50: u32,
        b_p95: u32,
        d_p50: u64,
        d_p95: u64,
        d_p99: u64,
    }
    let mut results: Vec<Trial> = Vec::with_capacity(trials);
    for t in 0..trials {
        let trial_seed = seed.wrapping_add(t as u64);
        if shuffle {
            shuffle_paths_seeded(&mut paths, trial_seed);
        }
        let t0 = Instant::now();
        let (files, bytes, matches, batch_sizes, dispatch_us_vec) =
            run_once(&variant, &root, &paths, pat.clone(), max_files, rt.as_ref(),
                     cfg, trial_seed, producer_pace);
        let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
        results.push(Trial {
            wall_ms, files, bytes, matches,
            b_p50: pct_u32(&batch_sizes, 0.50),
            b_p95: pct_u32(&batch_sizes, 0.95),
            d_p50: pct_u64(&dispatch_us_vec, 0.50),
            d_p95: pct_u64(&dispatch_us_vec, 0.95),
            d_p99: pct_u64(&dispatch_us_vec, 0.99),
        });
    }

    let wall_ms_vec: Vec<f64> = results.iter().map(|t| t.wall_ms).collect();
    let wall_p10 = pct_f64(&wall_ms_vec, 0.10);
    let wall_p50 = pct_f64(&wall_ms_vec, 0.50);
    let wall_p90 = pct_f64(&wall_ms_vec, 0.90);
    let wall_min = wall_ms_vec.iter().copied().fold(f64::MAX, f64::min);
    let wall_max = wall_ms_vec.iter().copied().fold(f64::MIN, f64::max);
    let d_p50_across = pct_u64(&results.iter().map(|t| t.d_p50).collect::<Vec<_>>(), 0.50);
    let d_p95_across = pct_u64(&results.iter().map(|t| t.d_p95).collect::<Vec<_>>(), 0.50);
    let d_p99_across = pct_u64(&results.iter().map(|t| t.d_p99).collect::<Vec<_>>(), 0.50);
    let files = results[0].files;
    let bytes = results[0].bytes;
    let matches = results[0].matches;
    let b_p50 = results.iter().map(|t| t.b_p50).sum::<u32>() / results.len() as u32;
    let b_p95 = results.iter().map(|t| t.b_p95).sum::<u32>() / results.len() as u32;

    let work_dist_str = match work_dist {
        WorkDist::Real => "real".to_string(),
        WorkDist::Fixed { per_item_us } => format!("fixed:{}", per_item_us),
        WorkDist::Pareto { median_us, shape_x10 } => format!("pareto:{}:{}", median_us, shape_x10),
    };
    let cost_dist_str = match fixed_cost {
        CostSample::Fixed(us) => format!("fixed:{}", us),
        CostSample::Pareto { median_us, shape_x10 } => format!("pareto:{}:{}", median_us, shape_x10),
        CostSample::Normal { mean_us, sd_us } => format!("normal:{}:{}", mean_us, sd_us),
        CostSample::Exp { mean_us } => format!("exp:{}", mean_us),
    };
    let batch_shape_str = match batch_shape {
        BatchShape::Linear => "linear".to_string(),
        BatchShape::Flat { flat_us } => format!("flat:{}", flat_us),
        BatchShape::Quadratic { coeff_us } => format!("quad:{}", coeff_us),
    };

    // Chart to stderr when trials > 1 (or explicitly requested).
    if trials > 1 && !no_chart {
        render_trial_bars(
            &format!("{} pattern={} cost={} work={} batch={} trials={}",
                     variant, pattern_tag, cost_dist_str, work_dist_str, batch_shape_str, trials),
            &wall_ms_vec, 50,
        );
        eprintln!(
            "  wall_ms  p10={:.1}  p50={:.1}  p90={:.1}  min={:.1}  max={:.1}  ratio={:.2}x",
            wall_p10, wall_p50, wall_p90, wall_min, wall_max,
            if wall_min > 0.0 { wall_max / wall_min } else { 0.0 }
        );
        eprintln!(
            "  disp_us  p50={}  p95={}  p99={}  (median of per-trial)",
            d_p50_across, d_p95_across, d_p99_across
        );
    }

    if tsv {
        // Single TSV row per invocation. Header printed by bash harness.
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{}\t{}\t{}\t{}\t{}",
            variant, pattern_tag, cost_dist_str, work_dist_str, batch_shape_str,
            producer_pace_str, seed, trials,
            files, bytes, matches,
            wall_p10, wall_p50, wall_p90, wall_min, wall_max,
            b_p50, b_p95, d_p50_across, d_p95_across, d_p99_across,
        );
    } else {
        println!(
            "{{\"variant\":\"{}\",\"pattern\":\"{}\",\"files\":{},\"bytes\":{},\"matches\":{},\"trials\":{},\"wall_p10\":{:.3},\"wall_p50\":{:.3},\"wall_p90\":{:.3},\"wall_min\":{:.3},\"wall_max\":{:.3},\"cost_dist\":\"{}\",\"producer_pace\":\"{}\",\"work_dist\":\"{}\",\"batch_shape\":\"{}\",\"seed\":{},\"shuffle\":{},\"batch_p50\":{},\"batch_p95\":{},\"disp_p50_us\":{},\"disp_p95_us\":{},\"disp_p99_us\":{}}}",
            variant, pattern_tag, files, bytes, matches, trials,
            wall_p10, wall_p50, wall_p90, wall_min, wall_max,
            cost_dist_str, producer_pace_str, work_dist_str, batch_shape_str, seed, shuffle,
            b_p50, b_p95, d_p50_across, d_p95_across, d_p99_across,
        );
    }
}

// ----------------------------------------------------------------------------
// Percentile + ASCII chart helpers (no extra deps).
// ----------------------------------------------------------------------------

fn pct_u32(v: &[u32], q: f64) -> u32 {
    if v.is_empty() { return 0; }
    let mut v = v.to_vec(); v.sort_unstable();
    v[((v.len() as f64 - 1.0) * q).round() as usize]
}
fn pct_u64(v: &[u64], q: f64) -> u64 {
    if v.is_empty() { return 0; }
    let mut v = v.to_vec(); v.sort_unstable();
    v[((v.len() as f64 - 1.0) * q).round() as usize]
}
fn pct_f64(v: &[f64], q: f64) -> f64 {
    if v.is_empty() { return 0.0; }
    let mut v = v.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[((v.len() as f64 - 1.0) * q).round() as usize]
}

fn render_trial_bars(title: &str, values: &[f64], width: usize) {
    if values.is_empty() { return; }
    let mx = values.iter().copied().fold(f64::MIN, f64::max);
    eprintln!("{}", title);
    for (i, &v) in values.iter().enumerate() {
        let n = if mx > 0.0 { ((v / mx) * width as f64).round() as usize } else { 0 };
        let bar: String = std::iter::repeat('#').take(n.min(width)).collect();
        eprintln!("  t{:>3} {:>9.2} ms |{}", i, v, bar);
    }
}

fn run_once(
    variant: &str,
    root: &Path,
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    max_files: Option<usize>,
    rt: Option<&tokio::runtime::Runtime>,
    cfg: DispatchCfg,
    seed: u64,
    producer_pace: ProducerPace,
) -> (u64, u64, u64, Vec<u32>, Vec<u64>) {
    // Lift variants without dispatch tracking into the 5-tuple shape.
    let lift3 = |t: (u64, u64, u64)| (t.0, t.1, t.2, Vec::new(), Vec::new());
    let lift4 = |t: (u64, u64, u64, Vec<u32>)| (t.0, t.1, t.2, t.3, Vec::new());
    if variant == "A" {
        lift3(variant_a(paths, &pat))
    } else if variant == "B" {
        lift3(variant_b(root, pat, max_files))
    } else if variant == "Efat" {
        lift3(variant_e_fat(paths, pat))
    } else if let Some(cap) = variant.strip_prefix("Cdam:") {
        let cap: usize = cap.parse().expect("Cdam:<cap>");
        lift3(variant_c(paths, pat, cap))
    } else if let Some(rest) = variant.strip_prefix("Cbatch:") {
        let (cap, batch) = rest.split_once(':').expect("Cbatch:<cap>:<batch>");
        lift3(variant_c_batch(paths, pat, cap.parse().unwrap(), batch.parse().unwrap()))
    } else if let Some(conc) = variant.strip_prefix("Dtokio:") {
        let conc: usize = conc.parse().expect("Dtokio:<conc>");
        lift3(rt.expect("tokio runtime required").block_on(variant_d(paths, pat, conc)))
    } else if let Some(rest) = variant.strip_prefix("Dbatch:") {
        let (conc, batch) = rest.split_once(':').expect("Dbatch:<conc>:<batch>");
        lift3(rt.expect("tokio runtime required").block_on(variant_d_batch(paths, pat, conc.parse().unwrap(), batch.parse().unwrap())))
    } else if let Some(rest) = variant.strip_prefix("Cdam2:") {
        let cap: usize = rest.parse().expect("Cdam2:<cap>");
        variant_cdam2(paths, pat, cap, cfg, seed, producer_pace)
    } else if let Some(rest) = variant.strip_prefix("Gopp2:") {
        let (cap, mb) = rest.split_once(':').expect("Gopp2:<cap>:<max_batch>");
        variant_gopp2(paths, pat, cap.parse().unwrap(), mb.parse().unwrap(), cfg, seed, producer_pace)
    } else if let Some(rest) = variant.strip_prefix("Hopp2:") {
        let parts: Vec<&str> = rest.split(':').collect();
        assert_eq!(parts.len(), 3, "Hopp2:<cap>:<max_batch>:<workers>");
        variant_hopp2(
            paths, pat,
            parts[0].parse().unwrap(),
            parts[1].parse().unwrap(),
            parts[2].parse().unwrap(),
            cfg, seed, producer_pace,
        )
    } else if let Some(rest) = variant.strip_prefix("Cdam2Buf:") {
        let parts: Vec<&str> = rest.split(':').collect();
        assert_eq!(parts.len(), 2, "Cdam2Buf:<cap>:<max_batch>");
        variant_cdam2_buf(
            paths, pat,
            parts[0].parse().unwrap(),
            parts[1].parse().unwrap(),
            cfg, seed, producer_pace,
        )
    } else if let Some(rest) = variant.strip_prefix("Rxrust1Buf:") {
        let parts: Vec<&str> = rest.split(':').collect();
        assert_eq!(parts.len(), 2, "Rxrust1Buf:<cap>:<max_batch>");
        variant_rxrust1_buf(
            paths, pat,
            parts[0].parse().unwrap(),
            parts[1].parse().unwrap(),
            cfg, seed, producer_pace,
        )
    } else if let Some(rest) = variant.strip_prefix("Rxrust1:") {
        let cap: usize = rest.parse().expect("Rxrust1:<cap>");
        variant_rxrust1(paths, pat, cap, cfg, seed, producer_pace)
    } else if let Some(rest) = variant.strip_prefix("Gopp:") {
        let (cap, mb) = rest.split_once(':').expect("Gopp:<cap>:<max_batch>");
        lift4(variant_g_opp(paths, pat, cap.parse().unwrap(), mb.parse().unwrap()))
    } else if let Some(rest) = variant.strip_prefix("Hopp:") {
        let parts: Vec<&str> = rest.split(':').collect();
        assert_eq!(parts.len(), 3, "Hopp:<cap>:<max_batch>:<workers>");
        lift4(variant_h_opp_mc(
            paths,
            pat,
            parts[0].parse().unwrap(),
            parts[1].parse().unwrap(),
            parts[2].parse().unwrap(),
        ))
    } else {
        panic!("unknown variant: {}", variant);
    }
}
