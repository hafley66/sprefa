//! Perf affirmation: run the same linux-kernel scan the `walk-parallel`
//! probe does, but dispatched through the v3 plugin registry via
//! `ctx.put(ScanFile).await` bound to `BoundedWorkSteal`.
//!
//! Goal: match or come within small single-digit % of the baseline
//! reference measurement (walk-parallel + prefilter + jemalloc, W=8:
//! ~3.64s on 63,482 .c/.h files).
//!
//! Usage:
//!   cargo run --release --bin ast_grep_v3_bench -- \
//!     --root <path-to-linux> --workers 8 --trials 3 \
//!     --pattern 'printk($$$)' --lang c
//!
//! The op code path is:
//!   for each path in Vec<PathBuf>:
//!     ctx.put(ScanFile { path }).await  -->
//!        mpsc(cap) --> rayon.spawn --> read+prefilter+find_all --> oneshot reply
//!
//! Measured wall == submitter wall (all paths enumerated up-front, K
//! concurrent submitter tasks fan through the inbox).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use ast_grep_core::{AstGrep, Language, Pattern, source::StrDoc};
use ast_grep_language::SupportLang;
use ignore::WalkBuilder;

use rayon::prelude::*;
use effect_runtime::batchers::{BoundedWorkSteal, Passthrough};
use effect_runtime::{EffectKind, RtCtxBuilder};

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// --- effect definitions ---------------------------------------------------

struct ScanFile {
    path: PathBuf,
}

impl EffectKind for ScanFile {
    type Response = (u64, u64); // (bytes_read, matches)
    /// Payload size hint for telemetry: no bytes until the file is
    /// read inside the handler, so the framework uses the response
    /// side to attribute throughput.
    fn payload_bytes(&self) -> Option<usize> { Some(0) }
    /// Response carries (bytes_read, matches). Feed bytes_read back
    /// as the payload attribution so `MB/s` in the telemetry table
    /// reports parse throughput.
    fn response_bytes(r: &Self::Response) -> Option<usize> { Some(r.0 as usize) }
}

/// Batch variant: one effect put per op invocation instead of per file.
/// Handler uses rayon `par_iter` internally. This closes the per-item
/// dispatch overhead that per-file emission incurs.
struct ScanBatch {
    paths: Vec<PathBuf>,
}

impl EffectKind for ScanBatch {
    type Response = (u64, u64); // (total_bytes, total_matches)
    fn payload_bytes(&self) -> Option<usize> { Some(0) }
    fn response_bytes(r: &Self::Response) -> Option<usize> { Some(r.0 as usize) }
}

// --- helpers --------------------------------------------------------------

fn parse_lang(s: &str) -> SupportLang {
    match s {
        "tsx" => SupportLang::Tsx,
        "typescript" | "ts" => SupportLang::TypeScript,
        "javascript" | "js" => SupportLang::JavaScript,
        "cpp" | "c++" | "c" => SupportLang::Cpp,
        "rust" | "rs" => SupportLang::Rust,
        "python" | "py" => SupportLang::Python,
        "go" => SupportLang::Go,
        other => panic!("unknown --lang: {}", other),
    }
}

fn enumerate(root: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for entry in WalkBuilder::new(root).hidden(true).git_ignore(false).build() {
        let Ok(e) = entry else { continue };
        if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let p = e.into_path();
        let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
        if exts.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
            v.push(p);
        }
    }
    v
}

fn rss_peak_kb() -> u64 {
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 {
            return 0;
        }
        #[cfg(target_os = "macos")]
        {
            (u.ru_maxrss as u64) / 1024
        }
        #[cfg(not(target_os = "macos"))]
        {
            u.ru_maxrss as u64
        }
    }
}

// --- main -----------------------------------------------------------------

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    // CLI
    let mut root = PathBuf::from("tests/smoke/.fixtures/linux");
    let mut workers: usize = 8;
    let mut trials: usize = 3;
    let mut pattern_src = String::from("printk($$$)");
    let mut lang_spec = String::from("c");
    let mut cap: usize = 256;
    let mut submitters: usize = 0; // 0 == workers * 2
    let mut mode: String = String::from("per-file"); // per-file | batch

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root"       => { root = PathBuf::from(&args[i + 1]); i += 2; }
            "--workers"    => { workers = args[i + 1].parse().unwrap(); i += 2; }
            "--trials"     => { trials = args[i + 1].parse::<usize>().unwrap().max(1); i += 2; }
            "--pattern"    => { pattern_src = args[i + 1].clone(); i += 2; }
            "--lang"       => { lang_spec = args[i + 1].clone(); i += 2; }
            "--cap"        => { cap = args[i + 1].parse().unwrap(); i += 2; }
            "--submitters" => { submitters = args[i + 1].parse().unwrap(); i += 2; }
            "--mode"       => { mode = args[i + 1].clone(); i += 2; }
            other => panic!("unknown arg: {}", other),
        }
    }
    if submitters == 0 {
        submitters = workers * 2;
    }

    let lang = parse_lang(&lang_spec);
    let pattern: Arc<Pattern<SupportLang>> =
        Arc::new(Pattern::try_new(&pattern_src, lang).expect("pattern compile"));

    // Configure the global rayon pool — used by the batch-mode handler
    // for `par_iter`. Per-file mode uses BoundedWorkSteal's own pool,
    // not this one.
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .ok();

    // Per-file compute, shared across both modes.
    let pattern_for_handler = pattern.clone();
    let scan_one_path = Arc::new(move |path: &Path| -> (u64, u64) {
        let Ok(src) = std::fs::read_to_string(path) else {
            return (0, 0);
        };
        let bytes = src.len() as u64;
        let fixed = pattern_for_handler.fixed_string();
        if !fixed.is_empty() && !src.contains(&*fixed) {
            return (bytes, 0);
        }
        let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(&src);
        let matches = grep.root().find_all(&*pattern_for_handler).count() as u64;
        (bytes, matches)
    });

    // Build the effect handlers. Both ScanFile (per-file emission) and
    // ScanBatch (batch emission) are registered, benchmark picks one
    // via --mode.
    let scan_for_file = scan_one_path.clone();
    let scan_for_batch = scan_one_path.clone();
    let ctx = RtCtxBuilder::new()
        .register::<ScanFile, _>(BoundedWorkSteal::<ScanFile>::new(
            cap,
            workers,
            move |e: ScanFile| -> (u64, u64) { scan_for_file(&e.path) },
        ))
        .register::<ScanBatch, _>(Passthrough::<ScanBatch, _>::new(
            move |e: ScanBatch| -> (u64, u64) {
                // par_iter on the GLOBAL rayon pool, which is sized
                // to `workers` above. Fold (bytes, matches) across
                // the chunked range.
                e.paths
                    .par_iter()
                    .map(|p| scan_for_batch(p))
                    .reduce(|| (0u64, 0u64), |(a, b), (c, d)| (a + c, b + d))
            },
        ))
        .build();

    // Enumerate + warm page cache.
    let t_enum = Instant::now();
    let paths = enumerate(&root, &["c", "h"]);
    let enumerate_ms = t_enum.elapsed().as_secs_f64() * 1000.0;
    assert!(!paths.is_empty(), "no files enumerated under {:?}", root);
    let rss_after_enum_kb = rss_peak_kb();

    let t_pc = Instant::now();
    for p in &paths { let _ = std::fs::read(p); }
    let page_cache_ms = t_pc.elapsed().as_secs_f64() * 1000.0;
    let rss_after_pc_kb = rss_peak_kb();

    // Warmup trial (not measured).
    let warm_n = paths.len().min(8192);
    let t_warm = Instant::now();
    run_one_trial(&ctx, &paths[..warm_n], submitters, &mode).await;
    let warm_ms = t_warm.elapsed().as_secs_f64() * 1000.0;
    let rss_after_warm_kb = rss_peak_kb();

    eprintln!(
        "# harness: files={} enumerate_ms={:.1} page_cache_ms={:.1} warm_n={} warm_ms={:.1}",
        paths.len(), enumerate_ms, page_cache_ms, warm_n, warm_ms,
    );
    eprintln!(
        "# rss_kb: after_enum={} after_pc={} after_warm={}",
        rss_after_enum_kb, rss_after_pc_kb, rss_after_warm_kb,
    );
    eprintln!(
        "# config: v3 mode={} cap={} workers={} submitters={} pattern={:?} lang={}",
        mode, cap, workers, submitters, pattern_src, lang_spec,
    );

    // Trial loop. Top-level wall is a single `Instant::now()`; every
    // per-effect breakdown (count, p50, p95, p99, MB/s) comes from
    // the framework telemetry sink with zero measurement code in
    // this binary beyond the outer clock.
    let mut last_bytes: u64 = 0;
    let mut last_matches: u64 = 0;
    let mut walls_ms: Vec<f64> = Vec::with_capacity(trials);
    for t in 0..trials {
        ctx.telemetry().clear();
        let t0 = Instant::now();
        let (bytes, matches) = run_one_trial(&ctx, &paths, submitters, &mode).await;
        let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
        walls_ms.push(wall_ms);
        eprintln!(
            "# trial {}/{}: end_to_end_wall_ms={:.1} bytes={} matches={} rss_kb={}",
            t + 1, trials, wall_ms, bytes, matches, rss_peak_kb(),
        );
        eprintln!("{}", ctx.telemetry().summary());
        last_bytes = bytes;
        last_matches = matches;
    }

    let mut sorted = walls_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = sorted[sorted.len() / 2];
    let wmin = sorted.first().copied().unwrap();
    let wmax = sorted.last().copied().unwrap();
    let variant = match mode.as_str() {
        "batch" => format!("v3-Batch:{}", workers),
        _ => format!("v3-PerFile:{}:{}", cap, workers),
    };
    println!(
        "{}\tfiles={}\tbytes={}\tmatches={}\twall_min_ms={:.1}\twall_p50_ms={:.1}\twall_max_ms={:.1}",
        variant,
        paths.len(), last_bytes, last_matches,
        wmin, p50, wmax,
    );
}

async fn run_one_trial(
    ctx: &effect_runtime::RtCtx,
    paths: &[PathBuf],
    submitters: usize,
    mode: &str,
) -> (u64, u64) {
    // Batch mode: one put per op invocation. Handler rayon-fans inside.
    if mode == "batch" {
        let batch = ScanBatch { paths: paths.to_vec() };
        return ctx.put(batch).await;
    }
    // Per-file mode: N submitter tasks hammer ctx.put(ScanFile).

    use std::sync::atomic::{AtomicU64, Ordering};
    let idx = Arc::new(AtomicU64::new(0));
    let total_bytes = Arc::new(AtomicU64::new(0));
    let total_matches = Arc::new(AtomicU64::new(0));
    let n = paths.len() as u64;
    let paths_arc: Arc<Vec<PathBuf>> = Arc::new(paths.to_vec());
    let mut join = Vec::with_capacity(submitters);
    for _ in 0..submitters {
        let ctx = ctx.clone();
        let idx = idx.clone();
        let total_bytes = total_bytes.clone();
        let total_matches = total_matches.clone();
        let paths_arc = paths_arc.clone();
        join.push(tokio::spawn(async move {
            loop {
                let i = idx.fetch_add(1, Ordering::Relaxed);
                if i >= n { return; }
                let p = paths_arc[i as usize].clone();
                let (b, m) = ctx.put(ScanFile { path: p }).await;
                total_bytes.fetch_add(b, Ordering::Relaxed);
                total_matches.fetch_add(m, Ordering::Relaxed);
            }
        }));
    }
    for h in join { let _ = h.await; }
    (total_bytes.load(Ordering::Relaxed), total_matches.load(Ordering::Relaxed))
}
