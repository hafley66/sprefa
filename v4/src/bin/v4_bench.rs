// v4-bench — perf test against v3's `ast_grep_v3_bench` shape, but
// drives the REAL v4 Op pipeline (Fs > AstNm > [Fact]) end-to-end.
// The cpu parallelism comes from AstNm's spawn_blocking + rayon par_iter,
// which is what production v4 ops actually do.
//
// Three modes isolate which layer adds cost:
//   bare    Fs > AstNm > Count           (no Store)
//   insert  Fs > AstNm > Fact            (Store::insert_many per batch, no commit)
//   full    Fs > AstNm > Fact + commit + 1 GroupCount rule
//
// Usage:
//   cargo run --release --bin v4-bench -- --root <linux> \
//     --workers 8 --trials 3 --pattern 'printk($$$)' --lang c --mode bare

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ast_grep_language::SupportLang;
use tokio::sync::mpsc;

use v4::*;

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Clone, Copy, Debug)]
enum Mode { Bare, Insert, Full }
impl Mode {
    fn parse(s: &str) -> Self {
        match s { "bare"=>Mode::Bare, "insert"=>Mode::Insert, "full"=>Mode::Full, _ => panic!("bad mode") }
    }
}

fn parse_lang(s: &str) -> SupportLang {
    // Match v3's bench: route `c` → Cpp because tree-sitter-cpp is more
    // tolerant of kernel-flavored C (macro adjacency, GCC extensions).
    // tree-sitter-c rejects most kernel TUs and silently yields ~0 hits.
    match s {
        "c" | "cpp" | "c++" => SupportLang::Cpp,
        "rust" | "rs"       => SupportLang::Rust,
        other => panic!("unknown --lang: {}", other),
    }
}

fn rss_peak_kb() -> u64 {
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 { return 0; }
        #[cfg(target_os = "macos")] { (u.ru_maxrss as u64) / 1024 }
        #[cfg(not(target_os = "macos"))] { u.ru_maxrss as u64 }
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut root = PathBuf::from("../v3/tests/smoke/.fixtures/linux");
    let mut workers: usize = 8;
    let mut trials: usize = 3;
    let mut pattern_src = String::from("printk($$$)");
    let mut lang_spec = String::from("c");
    let mut mode_str = String::from("bare");
    // Defaults tuned for bulk scan (linux fixture, 63k files, 8 workers):
    // batch ≈ files/workers and cap big enough to fully decouple Fs from
    // AstNm. Small-corpus / incremental workloads should pass --batch
    // small (~256) and --cap 8 to preserve tail latency.
    let mut cap: usize = 64;
    let mut batch: usize = 4096;
    let mut file: Option<PathBuf> = None;
    let mut multi: usize = 0;     // 0 = single AstNm; >0 = MultiAstNm w/ N copies of --pattern
    let mut multi_each: Vec<String> = Vec::new(); // alternative: --pattern-each foo --pattern-each bar
    // Store-stress knobs (Full mode only):
    //   --commit-every N: insert a CommitEvery op that fires store.commit
    //     every N batches. 0 (default) = commit once at end, current shape.
    //   --rules N: define N copies of the GroupCount rule. Tests fan-out.
    let mut commit_every: usize = 0;
    let mut rules: usize = 1;
    // mem | dd | sqlite-mem | sqlite-disk. Picks Store impl in Insert/Full modes.
    let mut store_kind = String::from("mem");
    // Override default sqlite-disk path (default: $HOME/.cache/sprefa/v4-bench.db).
    let mut sqlite_path: Option<PathBuf> = None;
    // When true, run the SAME workload twice (once mem, once dd) and
    // compare rule snapshots row-for-row. Sanity check; bypasses --store.
    let mut verify = false;
    // Layer-3 OpCache: wrap AstNm matcher in OpCache, persist across
    // trials. Trial 1 = cold (all misses); trial 2..N = warm (all hits
    // when corpus + pattern unchanged).
    let mut cache = false;
    // --substrate: replace the v4 stream pipeline with the
    // effect_runtime::v2 Component pipeline (Fs > AstNm > Count). Bare
    // mode only. Direct A/B against the native v4 path.
    let mut substrate = false;
    // --memoize off|on|reconcile (substrate path only). Wraps AstNm in
    // Memoize. `on` adds cache lookup cost; `reconcile` adds cache +
    // PriorChildIndex diff. Useful for measuring (b)/(c) overhead on
    // a workload where reconcile gains nothing (one-shot scan).
    let mut memoize = String::from("off");
    // --memoize-share: reuse the same MemoCache + PriorChildIndex
    // across trials. Trials 2..N see warm cache (cache hits = inner
    // skipped). Default off (fresh per trial = cold cost).
    let mut memoize_share = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root"    => { root = PathBuf::from(&args[i+1]); i += 2; }
            "--workers" => { workers = args[i+1].parse().unwrap(); i += 2; }
            "--trials"  => { trials  = args[i+1].parse::<usize>().unwrap().max(1); i += 2; }
            "--pattern" => { pattern_src = args[i+1].clone(); i += 2; }
            "--lang"    => { lang_spec   = args[i+1].clone(); i += 2; }
            "--mode"    => { mode_str    = args[i+1].clone(); i += 2; }
            "--cap"     => { cap         = args[i+1].parse().unwrap(); i += 2; }
            "--batch"   => { batch       = args[i+1].parse().unwrap(); i += 2; }
            "--file"    => { file        = Some(PathBuf::from(&args[i+1])); i += 2; }
            "--multi"   => { multi       = args[i+1].parse().unwrap(); i += 2; }
            "--pattern-each" => { multi_each.push(args[i+1].clone()); i += 2; }
            "--commit-every" => { commit_every = args[i+1].parse().unwrap(); i += 2; }
            "--rules"        => { rules        = args[i+1].parse::<usize>().unwrap().max(1); i += 2; }
            "--store"        => { store_kind   = args[i+1].clone(); i += 2; }
            "--sqlite-path"  => { sqlite_path  = Some(PathBuf::from(&args[i+1])); i += 2; }
            "--verify"       => { verify       = true; i += 1; }
            "--cache"        => { cache        = true; i += 1; }
            "--substrate"    => { substrate    = true; i += 1; }
            "--memoize"      => { memoize      = args[i+1].clone(); i += 2; }
            "--memoize-share"=> { memoize_share= true; i += 1; }
            other       => panic!("unknown arg: {}", other),
        }
    }
    let mode = Mode::parse(&mode_str);
    let lang = parse_lang(&lang_spec);
    let exts: Vec<String> = match lang_spec.as_str() {
        "c" | "cpp" | "c++" => vec!["c".into(), "h".into()],
        "rust" | "rs"       => vec!["rs".into()],
        _ => panic!(),
    };

    rayon::ThreadPoolBuilder::new().num_threads(workers).build_global().ok();

    eprintln!(
        "mode={:?} workers={} cap={} batch={} trials={} pattern={:?} lang={:?} root={} cache={} substrate={} memoize={} memoize_share={}",
        mode, workers, cap, batch, trials, pattern_src, lang, root.display(), cache, substrate, memoize, memoize_share,
    );
    if !matches!(memoize.as_str(), "off" | "on" | "reconcile") {
        panic!("--memoize must be off|on|reconcile, got {memoize:?}");
    }
    if memoize != "off" && !substrate {
        panic!("--memoize requires --substrate");
    }

    // Substrate (effect_runtime::v2) path: bare-mode A/B against native v4.
    // Skips Hooks/Store/saga entirely. Three Components: Fs > AstNm > Count.
    if substrate {
        if !matches!(mode, Mode::Bare) {
            panic!("--substrate currently supports --mode bare only");
        }
        use std::sync::Arc;
        use effect_runtime::v2::{
            expand, Component, ExpandOpts, MemQueue, MemoCache, Memoize,
            PipeInstance, PriorChildIndex, QueueBackend,
        };
        use v4::substrate_ops::{AstNmComponent, CountComponent, FsComponent};

        // Shared cache + index across trials when --memoize-share.
        let shared_cache: Option<Arc<MemoCache<v4::Cursor>>> =
            if memoize_share && memoize != "off" {
                Some(Arc::new(MemoCache::new()))
            } else { None };
        let shared_idx: Option<Arc<PriorChildIndex>> =
            if memoize_share && memoize == "reconcile" {
                Some(Arc::new(PriorChildIndex::new()))
            } else { None };

        let mut walls = Vec::new();
        let mut last_matches: u64 = 0;
        for trial in 1..=trials {
            let counter = Arc::new(AtomicU64::new(0));

            let astnm: Arc<dyn Component<Next = v4::Cursor>> = match memoize.as_str() {
                "off" => Arc::new(AstNmComponent::new(pattern_src.clone(), lang)),
                "on"  => {
                    let cache = shared_cache.clone().unwrap_or_else(|| Arc::new(MemoCache::new()));
                    Arc::new(Memoize::new(
                        AstNmComponent::new(pattern_src.clone(), lang),
                        "astnm",
                        cache,
                    ).with_domain("fs"))
                }
                "reconcile" => {
                    let cache = shared_cache.clone().unwrap_or_else(|| Arc::new(MemoCache::new()));
                    let idx   = shared_idx.clone().unwrap_or_else(|| Arc::new(PriorChildIndex::new()));
                    Arc::new(Memoize::new(
                        AstNmComponent::new(pattern_src.clone(), lang),
                        "astnm",
                        cache,
                    ).with_domain("fs").with_prior_children(idx))
                }
                _ => unreachable!(),
            };

            let pipe = PipeInstance::new(vec![
                Arc::new(FsComponent::new(root.clone(), exts.clone(), batch))
                    as Arc<dyn Component<Next = v4::Cursor>>,
                astnm,
                Arc::new(CountComponent { count: counter.clone() }),
            ]);
            let queue: Arc<dyn QueueBackend<v4::Cursor>> = Arc::new(MemQueue::new());
            // batch_cap drives rayon tail-sync frequency in AstNm:
            // each pull_runnable_batch chunks the queue into one
            // par_render call. Smaller cap = more sync points = slow
            // files block N-1 workers per batch. For one-shot scan we
            // want ONE big batch. Take max(batch, 65536) so callers
            // who set --batch up to control FS streaming chunk size
            // don't accidentally also constrain the AstNm batch.
            let opts = ExpandOpts::default().with_batch_cap(batch.max(65536));

            let t_run = Instant::now();
            let stats = expand(&pipe, queue, vec![Arc::new(v4::Cursor::default())], opts);
            let wall = t_run.elapsed();

            let m = counter.load(Ordering::Relaxed);
            let rss = rss_peak_kb() / 1024;
            eprintln!(
                "trial {}: wall={:.3}s  matches={:>9}  rendered={}  emitted={}  rss_peak_MB={}",
                trial, wall.as_secs_f64(), m, stats.rendered, stats.emitted, rss,
            );
            walls.push(wall);
            last_matches = m;
        }
        walls.sort();
        let med = walls[walls.len()/2];
        eprintln!("───────────────────────────────────────────────────────────────────────────");
        eprintln!("median:  wall={:.3}s  matches={}  (substrate)", med.as_secs_f64(), last_matches);
        return;
    }

    // OpCache lifted outside the trial loop so trial 2..N see warm hits.
    // Built only when --cache and matcher is single AstNm (Multi/SinglePath
    // cache wrapping is a follow-up).
    let cache_op: Option<Arc<OpCache>> = if cache && multi == 0 && multi_each.is_empty() {
        Some(OpCache::wrap(Arc::new(
            AstNm::new(&pattern_src, lang, &[]).with_match_text(false),
        )))
    } else { None };

    let mut walls = Vec::new();
    let mut last = (0u64, 0u64);

    for trial in 1..=trials {
        let rule_set: Vec<(String, RuleBody)> = (0..rules).map(|r| (
            format!("hot_pattern_{}", r),
            RuleBody::GroupCount {
                src: "matches".into(), key: "FS".into(),
                min: 2, count_term: "COUNT".into(),
            }
        )).collect();
        let interner = Interner::new();
        let mem_only;
        let sqlite_only;
        let (store, attach_tele): (Arc<dyn Store>, Box<dyn FnOnce(Telemetry)>) = match store_kind.as_str() {
            "mem" => {
                mem_only = MemStore::new();
                if matches!(mode, Mode::Full) {
                    for (n, b) in &rule_set { mem_only.define_rule(n, b.clone()); }
                }
                let m = mem_only.clone();
                (mem_only.clone() as Arc<dyn Store>, Box::new(move |t| m.attach_tele(t)))
            }
            "sqlite-mem" => {
                sqlite_only = SqliteStore::open_memory();
                let s = sqlite_only.clone();
                (sqlite_only.clone() as Arc<dyn Store>, Box::new(move |t| s.attach_tele(t)))
            }
            "sqlite-disk" | "sqlite-fast" => {
                let path = sqlite_path.clone().unwrap_or_else(|| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                    let name = if store_kind == "sqlite-fast" { "v4-bench-fast.db" } else { "v4-bench.db" };
                    PathBuf::from(home).join(".cache/sprefa").join(name)
                });
                // Trial 1 is the cold cost. Wipe stale benchmark db so trial 1
                // is deterministic across runs. (Trials 2..N reuse same db.)
                if trial == 1 {
                    let _ = std::fs::remove_file(&path);
                    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
                    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
                }
                sqlite_only = if store_kind == "sqlite-fast" {
                    SqliteStore::open_path_fast(&path)
                } else {
                    SqliteStore::open_path(&path)
                };
                let s = sqlite_only.clone();
                (sqlite_only.clone() as Arc<dyn Store>, Box::new(move |t| s.attach_tele(t)))
            }
            other => panic!("--store must be mem|sqlite-mem|sqlite-disk|sqlite-fast, got {}", other),
        };

        let matches    = Arc::new(AtomicU64::new(0));
        let bytes_seen = Arc::new(AtomicU64::new(0));

        let (eff_tx, mut eff_rx) = mpsc::unbounded_channel();
        let saga = tokio::spawn(async move { while eff_rx.recv().await.is_some() {} });

        let tele = Telemetry::new();
        attach_tele(tele.clone());
        let hooks = Hooks {
            store:    store.clone(),
            effects:  eff_tx.clone(),
            gen:      trial as u64,
            lineage:  new_lineage(),
            tele:     tele.clone(),
            interner: interner.clone(),
        };

        // Build the real pipeline. Source is SinglePath when --file is
        // set (single-file scaling probe), else Fs (bulk corpus).
        // Matcher op is MultiAstNm when --multi N or --pattern-each are used,
        // else single AstNm.
        let source: Arc<dyn Op> = match &file {
            Some(p) => Arc::new(SinglePath { path: p.clone() }),
            None    => Arc::new(Fs { root: root.clone(), exts: exts.clone(), batch, cap }),
        };
        let matcher: Arc<dyn Op> = if !multi_each.is_empty() {
            let pats: Vec<(String, String)> = multi_each.iter().enumerate()
                .map(|(i, p)| (format!("p{}", i), p.clone())).collect();
            Arc::new(MultiAstNm::new(pats, lang))
        } else if multi > 0 {
            let pats: Vec<(String, String)> = (0..multi)
                .map(|i| (format!("p{}", i), pattern_src.clone())).collect();
            Arc::new(MultiAstNm::new(pats, lang))
        } else if let Some(c) = &cache_op {
            c.clone() as Arc<dyn Op>
        } else {
            Arc::new(AstNm::new(&pattern_src, lang, &[]).with_match_text(false))
        };
        let mut chain: Vec<Arc<dyn Op>> = vec![source, matcher];
        match mode {
            Mode::Bare => chain.push(Arc::new(Count {
                matches: matches.clone(), bytes_seen: bytes_seen.clone(),
            })),
            Mode::Insert | Mode::Full => {
                // count first, then store. (Count is pass-through.)
                chain.push(Arc::new(Count {
                    matches: matches.clone(), bytes_seen: bytes_seen.clone(),
                }));
                // FactWrite declares the schema upfront — required for
                // SqliteStore. mem/dd ignore ensure_schema, so this is
                // safe across all four stores. Cols come from AstNm's
                // stamp set with want_match=false.
                chain.push(Arc::new(FactWrite::new(
                    "matches",
                    &["FS", "CONTENT_HASH", "LO", "HI"],
                )));
                // CommitEvery applies to Insert + Full both — useful to
                // measure store commit overhead under reactive-shaped
                // (many small commits) workloads, not just one big tx.
                if commit_every > 0 {
                    chain.push(Arc::new(CommitEvery::new(commit_every)));
                }
            }
        }

        let t_run = Instant::now();
        drive_with(chain, hooks, cap).await;
        // Insert AND Full both need commit on stores that buffer pending
        // rows (Sqlite). Mem/Dd treat commit as a no-op or boundary tick.
        if matches!(mode, Mode::Insert | Mode::Full) {
            store.commit(trial as u64).await;
        }
        let wall = t_run.elapsed();
        // Correctness probe. Snapshot rule 0 (always defined in Full mode).
        let rule0_cardinality = if matches!(mode, Mode::Full) {
            store.snapshot("hot_pattern_0").await.len()
        } else { 0 };

        drop(eff_tx);
        let _ = saga.await;

        let m  = matches.load(Ordering::Relaxed);
        let _b = bytes_seen.load(Ordering::Relaxed);
        let files_s = "n/a"; // Fs walks internally; we don't enumerate up-front
        let rss = rss_peak_kb() / 1024;
        let cache_line = if let Some(c) = &cache_op {
            format!("  cache(hits={} misses={})", c.hits(), c.misses())
        } else { String::new() };
        eprintln!("trial {}: wall={:.3}s  matches={:>9}  rule0_rows={}  files/s={}  rss_peak_MB={}{}",
                  trial, wall.as_secs_f64(), m, rule0_cardinality, files_s, rss, cache_line);
        eprint!("{}", tele.summary());
        // Mem-only stats footer (DdStore arrangements are inside the worker).
        if !matches!(mode, Mode::Bare) && store_kind == "mem" {
            // Re-fetch the MemStore behind the Arc<dyn Store>.
            // (We held a raw clone in `mem_only`; reach back through that.)
            // Skipping the cast-back to keep the bench short; the per-fact
            // numbers are reproducible by the spans alone.
        }
        walls.push(wall);
        last = (0, m);
    }

    walls.sort();
    let med = walls[walls.len()/2];
    eprintln!("───────────────────────────────────────────────────────────────────────────");
    eprintln!("median:  wall={:.3}s  matches={}", med.as_secs_f64(), last.1);
}
