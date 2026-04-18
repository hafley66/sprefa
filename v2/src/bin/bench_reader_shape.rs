//! bench_reader_shape — isolate the claimed perf levers in the refactor.
//!
//! Reads N blobs from a real git repo through four variants. Each variant
//! does the same per-file work (find_tree → get_path → peel_to_blob → copy).
//! tree_oid is precomputed once (matches current reader's trees cache).
//!
//! Variants:
//!   A  tokio join_all + Arc<Mutex<Repository>>         — current shape
//!   B  rayon par_iter + Arc<Mutex<Repository>>          — swap executor
//!   C  rayon par_iter + per-worker Repository           — + thread-local handle
//!   D  sequential loop + single Repository              — baseline
//!
//! Isolates:
//!   D vs A : does tokio parallelism help at all with shared mutex?
//!   A vs B : does rayon vs tokio matter when mutex is the floor?
//!   B vs C : does per-worker Repository actually unblock parallelism?
//!
//! Usage: cargo run --release --bin bench_reader_shape -- <repo_path> [N]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ast_grep_core::{AstGrep, Language, Pattern, source::StrDoc};
use ast_grep_language::SupportLang;
use bytes::Bytes;
use futures::future::join_all;
use git2::{Oid, Repository};
use rayon::prelude::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let repo_path = PathBuf::from(
        args.get(1)
            .cloned()
            .unwrap_or_else(|| "tests/smoke/.fixtures/swc".into()),
    );
    let n_requested: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1067);

    eprintln!("repo: {}", repo_path.display());
    eprintln!("target N: {}", n_requested);

    // --- enumerate blobs at HEAD once via libgit2 tree walk ----------------
    let list_t0 = Instant::now();
    let (tree_oid, paths) = enumerate(&repo_path);
    let list_ms = list_t0.elapsed().as_secs_f64() * 1000.0;
    let n = paths.len().min(n_requested);
    let paths: Vec<PathBuf> = paths.into_iter().take(n).collect();
    eprintln!(
        "enumerate: {} files in {:.1}ms (tree_oid={})",
        n, list_ms, tree_oid
    );
    eprintln!();

    // --- warm the OS page cache (first run pays IO we don't want to measure)
    eprintln!("[warmup pass — not measured]");
    variant_d_sequential(&repo_path, tree_oid, &paths);

    eprintln!();
    eprintln!("{:>3}  {:>10}  {:>9}  {}", "var", "total_ms", "ms/file", "description");
    eprintln!("{:->3}  {:->10}  {:->9}  {:->40}", "", "", "", "");

    // --- A: tokio join_all + shared Mutex<Repository> ----------------------
    let t0 = Instant::now();
    let bytes_a = variant_a_tokio_mutex(&repo_path, tree_oid, &paths);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    report("A", ms, n, &bytes_a, "tokio join_all + shared Mutex<Repository>");

    // --- B: rayon par_iter + shared Mutex<Repository> ----------------------
    let t0 = Instant::now();
    let bytes_b = variant_b_rayon_mutex(&repo_path, tree_oid, &paths);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    report("B", ms, n, &bytes_b, "rayon par_iter + shared Mutex<Repository>");

    // --- C: rayon par_iter + per-worker Repository -------------------------
    let t0 = Instant::now();
    let bytes_c = variant_c_rayon_perworker(&repo_path, tree_oid, &paths);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    report("C", ms, n, &bytes_c, "rayon par_iter + per-worker Repository");

    // --- D: sequential (single Repository) ---------------------------------
    let t0 = Instant::now();
    let bytes_d = variant_d_sequential(&repo_path, tree_oid, &paths);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    report("D", ms, n, &bytes_d, "sequential (baseline)");

    // Assert all variants produced identical byte totals — otherwise the
    // benchmark is lying.
    let sum_a: usize = bytes_a.iter().map(|b| b.len()).sum();
    let sum_b: usize = bytes_b.iter().map(|b| b.len()).sum();
    let sum_c: usize = bytes_c.iter().map(|b| b.len()).sum();
    let sum_d: usize = bytes_d.iter().map(|b| b.len()).sum();
    eprintln!();
    eprintln!(
        "correctness: sum(bytes) A={} B={} C={} D={}  {}",
        sum_a, sum_b, sum_c, sum_d,
        if sum_a == sum_b && sum_b == sum_c && sum_c == sum_d {
            "✓ all variants match"
        } else {
            "✗ MISMATCH"
        }
    );
    eprintln!("rayon threads: {}", rayon::current_num_threads());

    // =====================================================================
    // Phase 2 — parse + match (the ~86% residual not explained by reader)
    // =====================================================================
    // Take bytes from variant C (fastest, identical content to others) and
    // run parse + find_all across the same four executor shapes.
    //
    // Pattern: `fn $NAME` — matches every Rust function declaration.
    // Same ast-grep API as `ops/_9_ast_grep.rs`: Pattern::try_new + lang.ast_grep + find_all.
    let lang = SupportLang::Rust;
    let pattern: Arc<Pattern<SupportLang>> =
        Arc::new(Pattern::try_new("fn $NAME", lang).expect("build pattern"));

    eprintln!();
    eprintln!("--- phase 2: parse + match (same bytes, same pattern) ---");
    eprintln!();

    // Warmup.
    let _ = parse_seq(&bytes_c, lang, &pattern);

    eprintln!("{:>3}  {:>10}  {:>9}  {:>10}  {}", "var", "total_ms", "ms/file", "matches", "description");
    eprintln!("{:->3}  {:->10}  {:->9}  {:->10}  {:->40}", "", "", "", "", "");

    // E — tokio join_all + shared pattern
    let t0 = Instant::now();
    let (matches_e, _) = parse_tokio_joinall(&bytes_c, lang, &pattern);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    report_p2("E", ms, n, matches_e, "tokio join_all + shared Arc<Pattern>");

    // F — rayon par_iter + shared pattern (no per-worker init)
    let t0 = Instant::now();
    let matches_f = parse_rayon_shared(&bytes_c, lang, &pattern);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    report_p2("F", ms, n, matches_f, "rayon par_iter + shared Arc<Pattern>");

    // G — rayon par_iter + map_init (per-worker scratch; Pattern is already Arc
    //     so not per-worker, but mirrors the future shape where a per-worker
    //     ast-grep Parser or regex cache would live here)
    let t0 = Instant::now();
    let matches_g = parse_rayon_init(&bytes_c, lang, &pattern);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    report_p2("G", ms, n, matches_g, "rayon par_iter + map_init (per-worker slot)");

    // H — sequential baseline
    let t0 = Instant::now();
    let matches_h = parse_seq(&bytes_c, lang, &pattern);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    report_p2("H", ms, n, matches_h, "sequential (baseline)");

    eprintln!();
    eprintln!(
        "correctness: matches E={} F={} G={} H={}  {}",
        matches_e, matches_f, matches_g, matches_h,
        if matches_e == matches_f && matches_f == matches_g && matches_g == matches_h {
            "✓ all variants match"
        } else {
            "✗ MISMATCH"
        }
    );
}

// ---------------------------------------------------------------------------
// Parse + match variants
// ---------------------------------------------------------------------------

fn parse_one(bytes: &Bytes, lang: SupportLang, pattern: &Pattern<SupportLang>) -> usize {
    let Ok(s) = std::str::from_utf8(bytes.as_ref()) else {
        return 0;
    };
    let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(s);
    grep.root().find_all(pattern).count()
}

fn parse_tokio_joinall(
    bytes: &[Bytes],
    lang: SupportLang,
    pattern: &Arc<Pattern<SupportLang>>,
) -> (usize, ()) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio rt");
    let total = rt.block_on(async move {
        let futs = bytes.iter().map(|b| {
            let pat = pattern.clone();
            let b = b.clone();
            async move { parse_one(&b, lang, &pat) }
        });
        join_all(futs).await.into_iter().sum::<usize>()
    });
    (total, ())
}

fn parse_rayon_shared(
    bytes: &[Bytes],
    lang: SupportLang,
    pattern: &Arc<Pattern<SupportLang>>,
) -> usize {
    bytes.par_iter().map(|b| parse_one(b, lang, pattern)).sum()
}

fn parse_rayon_init(
    bytes: &[Bytes],
    lang: SupportLang,
    pattern: &Arc<Pattern<SupportLang>>,
) -> usize {
    bytes
        .par_iter()
        .map_init(
            // Placeholder per-worker scratch slot — stands in for a future
            // per-worker Parser/regex cache. Pattern itself is Arc-cloned.
            || pattern.clone(),
            |pat, b| parse_one(b, lang, pat),
        )
        .sum()
}

fn parse_seq(
    bytes: &[Bytes],
    lang: SupportLang,
    pattern: &Pattern<SupportLang>,
) -> usize {
    bytes.iter().map(|b| parse_one(b, lang, pattern)).sum()
}

fn report_p2(var: &str, ms: f64, n: usize, matches: usize, desc: &str) {
    let per_file = ms / n as f64;
    eprintln!(
        "{:>3}  {:>10.1}  {:>9.3}  {:>10}  {}",
        var, ms, per_file, matches, desc
    );
}

// ---------------------------------------------------------------------------
// Enumerate blobs at HEAD — one tree walk, produces (tree_oid, paths)
// ---------------------------------------------------------------------------

fn enumerate(repo_path: &Path) -> (Oid, Vec<PathBuf>) {
    let repo = Repository::open(repo_path).expect("open repo");
    let head = repo.revparse_single("HEAD").expect("revparse HEAD");
    let commit = head.peel_to_commit().expect("peel to commit");
    let tree_oid = commit.tree_id();
    let tree = repo.find_tree(tree_oid).expect("find_tree");

    let mut paths = Vec::with_capacity(2048);
    tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            let name = entry.name().unwrap_or("");
            if name.ends_with(".rs") {
                let full = if root.is_empty() {
                    name.to_owned()
                } else {
                    format!("{}{}", root, name)
                };
                paths.push(PathBuf::from(full));
            }
        }
        git2::TreeWalkResult::Ok
    })
    .expect("tree walk");
    (tree_oid, paths)
}

// ---------------------------------------------------------------------------
// Core per-file work — identical across variants
// ---------------------------------------------------------------------------

fn read_blob(repo: &Repository, tree_oid: Oid, path: &Path) -> Bytes {
    // Matches current reader: find_tree → get_path → to_object → peel_to_blob
    //                       → Bytes::copy_from_slice
    let tree = match repo.find_tree(tree_oid) {
        Ok(t) => t,
        Err(_) => return Bytes::new(),
    };
    let entry = match tree.get_path(path) {
        Ok(e) => e,
        Err(_) => return Bytes::new(),
    };
    let obj = match entry.to_object(repo) {
        Ok(o) => o,
        Err(_) => return Bytes::new(),
    };
    let blob = match obj.peel_to_blob() {
        Ok(b) => b,
        Err(_) => return Bytes::new(),
    };
    Bytes::copy_from_slice(blob.content())
}

// ---------------------------------------------------------------------------
// Variant A — tokio join_all + shared Mutex<Repository>
// ---------------------------------------------------------------------------

fn variant_a_tokio_mutex(
    repo_path: &Path,
    tree_oid: Oid,
    paths: &[PathBuf],
) -> Vec<Bytes> {
    let repo = Arc::new(Mutex::new(
        Repository::open(repo_path).expect("open repo"),
    ));
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio rt");

    rt.block_on(async move {
        let futs = paths.iter().map(|p| {
            let repo = repo.clone();
            let p = p.clone();
            async move {
                // Exactly mimics _2_git.rs:407 — lock held for the chain.
                let guard = repo.lock().unwrap();
                read_blob(&*guard, tree_oid, &p)
            }
        });
        join_all(futs).await
    })
}

// ---------------------------------------------------------------------------
// Variant B — rayon par_iter + shared Mutex<Repository>
// ---------------------------------------------------------------------------

fn variant_b_rayon_mutex(
    repo_path: &Path,
    tree_oid: Oid,
    paths: &[PathBuf],
) -> Vec<Bytes> {
    let repo = Arc::new(Mutex::new(
        Repository::open(repo_path).expect("open repo"),
    ));
    paths
        .par_iter()
        .map(|p| {
            let guard = repo.lock().unwrap();
            read_blob(&*guard, tree_oid, p)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Variant C — rayon par_iter + per-worker Repository via map_init
// ---------------------------------------------------------------------------

fn variant_c_rayon_perworker(
    repo_path: &Path,
    tree_oid: Oid,
    paths: &[PathBuf],
) -> Vec<Bytes> {
    let repo_path = repo_path.to_path_buf();
    paths
        .par_iter()
        .map_init(
            // One Repository per rayon worker thread. Opened lazily on first
            // use; reused across all files this worker pulls.
            || Repository::open(&repo_path).expect("open repo (per-worker)"),
            |repo, p| read_blob(repo, tree_oid, p),
        )
        .collect()
}

// ---------------------------------------------------------------------------
// Variant D — sequential baseline
// ---------------------------------------------------------------------------

fn variant_d_sequential(
    repo_path: &Path,
    tree_oid: Oid,
    paths: &[PathBuf],
) -> Vec<Bytes> {
    let repo = Repository::open(repo_path).expect("open repo");
    paths
        .iter()
        .map(|p| read_blob(&repo, tree_oid, p))
        .collect()
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

fn report(var: &str, ms: f64, n: usize, bytes: &[Bytes], desc: &str) {
    let per_file = ms / n as f64;
    eprintln!("{:>3}  {:>10.1}  {:>9.3}  {}", var, ms, per_file, desc);
    let _ = bytes;
}
