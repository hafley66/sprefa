//! oxc throughput: parse-only and import-extraction modes over the same corpus.
//!
//! Parse modes:
//!   shell-serial   fork `oxc_parse_one` once per file, collect.
//!   shell-batch    fork `oxc_parse_one` with N files per invocation
//!                  (amortizes fork cost; closer to realistic CLI use).
//!   rayon          `par_iter` over paths, parse in process, no framework.
//!   ctx-put        `ctx.put(OxcParse{path}).await` via BoundedWorkSteal.
//!   ctx-batch      one `ctx.put(OxcParseBatch{paths})`, handler par_iters.
//!
//! Import-extraction modes:
//!   imports-rayon      parse + walk import/export declarations, no framework.
//!   imports-ctx-put    `ctx.put(OxcImports{path}).await`.
//!   imports-ctx-batch  one `ctx.put(OxcImportsBatch{paths})`, handler par_iters.
//!
//! All modes read the file via std::fs, parse with oxc, and sum
//! (bytes, errors, files). Output is one line per trial and a final
//! best-of-N summary.
//!
//! Usage:
//!   cargo run --release --bin oxc_v3_bench -- \
//!     --root <DT-root> --mode rayon --workers 8 --trials 3

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use effect_proof::effects::oxc_imports::{extract_one, OxcImports, OxcImportsBatch};
use effect_proof::effects::oxc_parse::{parse_one, OxcParse, OxcParseBatch};
use effect_runtime::batchers::{BoundedWorkSteal, Passthrough};
use effect_runtime::RtCtxBuilder;
use ignore::WalkBuilder;
use rayon::prelude::*;

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn enumerate(root: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for entry in WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(false)
        .build()
    {
        let Ok(e) = entry else { continue };
        if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let p = e.into_path();
        let Some(ext) = p.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        if exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) {
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

fn find_shell_target() -> PathBuf {
    let candidates = [
        "target/release/oxc_parse_one",
        "../target/release/oxc_parse_one",
        "../../target/release/oxc_parse_one",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("oxc_parse_one")
}

fn run_shell_serial(paths: &[PathBuf], bin: &Path) -> (u64, u64, u64) {
    let mut bytes = 0u64;
    let mut errors = 0u64;
    let mut files = 0u64;
    for p in paths {
        let Ok(out) = Command::new(bin).arg(p).output() else {
            continue;
        };
        let s = String::from_utf8_lossy(&out.stdout);
        for tok in s.split_whitespace() {
            if let Some(v) = tok.strip_prefix("bytes=") {
                bytes += v.parse::<u64>().unwrap_or(0);
            } else if let Some(v) = tok.strip_prefix("errors=") {
                errors += v.parse::<u64>().unwrap_or(0);
            }
        }
        files += 1;
    }
    (bytes, errors, files)
}

fn run_shell_batch(paths: &[PathBuf], bin: &Path, chunk: usize, workers: usize) -> (u64, u64, u64) {
    let bytes = AtomicU64::new(0);
    let errors = AtomicU64::new(0);
    let files = AtomicU64::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    pool.install(|| {
        paths.par_chunks(chunk).for_each(|group| {
            let mut cmd = Command::new(bin);
            for p in group {
                cmd.arg(p);
            }
            let Ok(out) = cmd.output() else {
                return;
            };
            let s = String::from_utf8_lossy(&out.stdout);
            for tok in s.split_whitespace() {
                if let Some(v) = tok.strip_prefix("bytes=") {
                    bytes.fetch_add(v.parse::<u64>().unwrap_or(0), Ordering::Relaxed);
                } else if let Some(v) = tok.strip_prefix("errors=") {
                    errors.fetch_add(v.parse::<u64>().unwrap_or(0), Ordering::Relaxed);
                }
            }
            files.fetch_add(group.len() as u64, Ordering::Relaxed);
        });
    });
    (
        bytes.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
        files.load(Ordering::Relaxed),
    )
}

fn run_rayon(paths: &[PathBuf]) -> (u64, u64, u64) {
    paths
        .par_iter()
        .map(|p| {
            let s = parse_one(p);
            (s.bytes, s.errors, 1u64)
        })
        .reduce(|| (0, 0, 0), |(a, b, c), (x, y, z)| (a + x, b + y, c + z))
}

fn run_imports_rayon(paths: &[PathBuf]) -> (u64, u64, u64) {
    paths
        .par_iter()
        .map(|p| {
            let s = extract_one(p);
            (s.bytes, s.errors, 1u64)
        })
        .reduce(|| (0, 0, 0), |(a, b, c), (x, y, z)| (a + x, b + y, c + z))
}

async fn run_ctx_put_inner<F, Fut>(
    _ctx: effect_runtime::RtCtx,
    paths: &[PathBuf],
    submitters: usize,
    work: F,
) -> (u64, u64, u64)
where
    F: FnMut(PathBuf) -> Fut + Send + Clone + 'static,
    Fut: std::future::Future<Output = (u64, u64, u64)> + Send,
{
    let idx = Arc::new(AtomicU64::new(0));
    let bytes = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let files = Arc::new(AtomicU64::new(0));
    let n = paths.len() as u64;
    let paths_arc: Arc<Vec<PathBuf>> = Arc::new(paths.to_vec());
    let mut join = Vec::with_capacity(submitters);
    for _ in 0..submitters {
        let idx = idx.clone();
        let bytes = bytes.clone();
        let errors = errors.clone();
        let files = files.clone();
        let paths_arc = paths_arc.clone();
        let mut work = work.clone();
        join.push(tokio::spawn(async move {
            loop {
                let i = idx.fetch_add(1, Ordering::Relaxed);
                if i >= n {
                    return;
                }
                let p = paths_arc[i as usize].clone();
                let (b, e, f) = work(p).await;
                bytes.fetch_add(b, Ordering::Relaxed);
                errors.fetch_add(e, Ordering::Relaxed);
                files.fetch_add(f, Ordering::Relaxed);
            }
        }));
    }
    for h in join {
        let _ = h.await;
    }
    (
        bytes.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
        files.load(Ordering::Relaxed),
    )
}

async fn run_ctx_put(
    ctx: &effect_runtime::RtCtx,
    paths: &[PathBuf],
    submitters: usize,
) -> (u64, u64, u64) {
    let ctx = ctx.clone();
    run_ctx_put_inner(ctx.clone(), paths, submitters, move |p| {
        let ctx = ctx.clone();
        async move {
            let s = ctx.put(OxcParse { path: p }).await;
            (s.bytes, s.errors, 1u64)
        }
    })
    .await
}

async fn run_ctx_batch(ctx: &effect_runtime::RtCtx, paths: &[PathBuf]) -> (u64, u64, u64) {
    let req = OxcParseBatch {
        paths: Arc::new(paths.to_vec()),
    };
    ctx.put(req).await
}

async fn run_imports_ctx_put(
    ctx: &effect_runtime::RtCtx,
    paths: &[PathBuf],
    submitters: usize,
) -> (u64, u64, u64) {
    let ctx = ctx.clone();
    run_ctx_put_inner(ctx.clone(), paths, submitters, move |p| {
        let ctx = ctx.clone();
        async move {
            let s = ctx.put(OxcImports { path: p }).await;
            (s.bytes, s.errors, 1u64)
        }
    })
    .await
}

async fn run_imports_ctx_batch(ctx: &effect_runtime::RtCtx, paths: &[PathBuf]) -> (u64, u64, u64) {
    let req = OxcImportsBatch {
        paths: Arc::new(paths.to_vec()),
    };
    ctx.put(req).await
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut root = PathBuf::from("/Users/chrishafley/projects/ext/DefinitelyTyped/types");
    let mut workers: usize = 8;
    let mut trials: usize = 3;
    let mut mode: String = String::from("rayon");
    let mut cap: usize = 256;
    let mut submitters: usize = 0;
    let mut max_files: usize = 0;
    let mut shell_chunk: usize = 64;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                root = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--workers" => {
                workers = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--trials" => {
                trials = args[i + 1].parse::<usize>().unwrap().max(1);
                i += 2;
            }
            "--mode" => {
                mode = args[i + 1].clone();
                i += 2;
            }
            "--cap" => {
                cap = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--submitters" => {
                submitters = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--max-files" => {
                max_files = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--shell-chunk" => {
                shell_chunk = args[i + 1].parse().unwrap();
                i += 2;
            }
            other => panic!("unknown arg: {}", other),
        }
    }
    if submitters == 0 {
        submitters = workers * 2;
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .ok();

    let t_enum = Instant::now();
    let mut paths = enumerate(&root, &["ts", "tsx"]);
    let enumerate_ms = t_enum.elapsed().as_secs_f64() * 1000.0;
    if max_files > 0 && paths.len() > max_files {
        paths.truncate(max_files);
    }
    assert!(!paths.is_empty(), "no files enumerated under {:?}", root);
    let rss_after_enum_kb = rss_peak_kb();

    let t_pc = Instant::now();
    for p in &paths {
        let _ = std::fs::read(p);
    }
    let page_cache_ms = t_pc.elapsed().as_secs_f64() * 1000.0;
    let rss_after_pc_kb = rss_peak_kb();

    let ctx = RtCtxBuilder::new()
        .register::<OxcParse, _>(BoundedWorkSteal::<OxcParse>::new(
            cap,
            workers,
            |e: OxcParse| parse_one(&e.path),
        ))
        .register::<OxcParseBatch, _>(Passthrough::<OxcParseBatch, _>::new(|e: OxcParseBatch| {
            e.paths
                .par_iter()
                .map(|p| {
                    let s = parse_one(p);
                    (s.bytes, s.errors, 1u64)
                })
                .reduce(
                    || (0u64, 0u64, 0u64),
                    |(a, b, c), (x, y, z)| (a + x, b + y, c + z),
                )
        }))
        .register::<OxcImports, _>(BoundedWorkSteal::<OxcImports>::new(
            cap,
            workers,
            |e: OxcImports| extract_one(&e.path),
        ))
        .register::<OxcImportsBatch, _>(Passthrough::<OxcImportsBatch, _>::new(|e: OxcImportsBatch| {
            e.paths
                .par_iter()
                .map(|p| {
                    let s = extract_one(p);
                    (s.bytes, s.errors, 1u64)
                })
                .reduce(
                    || (0u64, 0u64, 0u64),
                    |(a, b, c), (x, y, z)| (a + x, b + y, c + z),
                )
        }))
        .build();

    let shell_bin = find_shell_target();

    eprintln!(
        "# harness: files={} enumerate_ms={:.1} page_cache_ms={:.1} rss_kb_enum={} rss_kb_pc={}",
        paths.len(),
        enumerate_ms,
        page_cache_ms,
        rss_after_enum_kb,
        rss_after_pc_kb,
    );
    eprintln!(
        "# config: mode={} workers={} submitters={} cap={} shell_chunk={} shell_bin={}",
        mode,
        workers,
        submitters,
        cap,
        shell_chunk,
        shell_bin.display(),
    );

    let mut walls_ms: Vec<f64> = Vec::with_capacity(trials);
    let mut last_triple: (u64, u64, u64) = (0, 0, 0);
    for t in 0..trials {
        ctx.telemetry().clear();
        let t0 = Instant::now();
        let triple = match mode.as_str() {
            "shell-serial" => run_shell_serial(&paths, &shell_bin),
            "shell-batch" => run_shell_batch(&paths, &shell_bin, shell_chunk, workers),
            "rayon" => run_rayon(&paths),
            "ctx-put" => run_ctx_put(&ctx, &paths, submitters).await,
            "ctx-batch" => run_ctx_batch(&ctx, &paths).await,
            "imports-rayon" => run_imports_rayon(&paths),
            "imports-ctx-put" => run_imports_ctx_put(&ctx, &paths, submitters).await,
            "imports-ctx-batch" => run_imports_ctx_batch(&ctx, &paths).await,
            other => panic!("unknown mode: {}", other),
        };
        let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
        walls_ms.push(wall_ms);
        last_triple = triple;
        let mbps = (triple.0 as f64 / 1_048_576.0) / (wall_ms / 1000.0);
        eprintln!(
            "# trial {}/{}: wall_ms={:.1} files={} bytes={} errors={} mbps={:.1} rss_kb={}",
            t + 1,
            trials,
            wall_ms,
            triple.2,
            triple.0,
            triple.1,
            mbps,
            rss_peak_kb(),
        );
        if mode.starts_with("ctx-") || mode.starts_with("imports-ctx-") {
            eprintln!("{}", ctx.telemetry().summary());
        }
    }

    let mut sorted = walls_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = sorted[sorted.len() / 2];
    let wmin = sorted.first().copied().unwrap();
    let wmax = sorted.last().copied().unwrap();
    let mbps_best = (last_triple.0 as f64 / 1_048_576.0) / (wmin / 1000.0);
    println!(
        "{}\tfiles={}\tbytes={}\terrors={}\twall_min_ms={:.1}\twall_p50_ms={:.1}\twall_max_ms={:.1}\tmbps_best={:.1}",
        mode, last_triple.2, last_triple.0, last_triple.1,
        wmin, p50, wmax, mbps_best,
    );
}
