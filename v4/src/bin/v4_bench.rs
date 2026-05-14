// v4-bench — perf test against v3's `ast_grep_v3_bench` shape, driving
// the v2 (effect_runtime) Component pipeline (Fs > AstNm > Count) end-
// to-end. Cpu parallelism comes from rayon par_iter inside the
// matcher Component.
//
// Three modes isolate which layer adds cost:
//   bare    Fs > AstNm > Count
//   insert  Fs > AstNm > FactWrite [+ CommitEvery]
//   full    not yet ported (rule infra still landing)
//
// Bulk shape is source-aware `fs > ast`; file bodies stay behind the
// ast operator and are not serialized through cursor.value.
//
// Usage:
//   cargo run --release --bin v4-bench -- --root <linux> \
//     --workers 8 --trials 3 --pattern 'printk($$$)' --lang c --mode bare

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ast_grep_language::SupportLang;
use ignore::WalkBuilder;

use effect_runtime::v2::{
    expand, Component, EffectTelemetry, ExpandOpts, FactStore, MemFactStore, MemQueue, MemoCache,
    Memoize, Node, PipeInstance, PriorChildIndex, QueueBackend, RenderCtx, SqliteFactStore,
};
use v4::fact::FactWrite;
use v4::store::SprfStore;
use v4::telemetry::{StageTiming, TimedComponent};
use v4::v2_ops::{
    AstNmComponent, AstTelemetry, CountComponent, FsComponent, FsTelemetry, MultiAstNmComponent,
    SinglePathComponent,
};

#[derive(Default)]
struct BenchCounters {
    source_rows: Arc<AtomicU64>,
}

/// Bench-local pass-through counter. Keeps telemetry out of the core
/// runtime while still exposing row counts at pipeline boundaries.
struct RowCountComponent {
    rows: Arc<AtomicU64>,
}

impl Component for RowCountComponent {
    type Next = v4::Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &v4::Cursor) -> Node<v4::Cursor> {
        self.rows.fetch_add(1, Ordering::Relaxed);
        Node::Emit(Arc::new(c.clone()))
    }
}

/// Bench-local: emit each cursor through, and call `store.commit(gen, None)`
/// every Nth render_batch. Drives commit-cost telemetry in Insert mode.
struct CommitEveryComponent {
    store: Arc<dyn FactStore<v4::Cursor>>,
    every: usize,
    seen: std::sync::atomic::AtomicUsize,
}
impl CommitEveryComponent {
    fn new(store: Arc<dyn FactStore<v4::Cursor>>, every: usize) -> Self {
        Self {
            store,
            every: every.max(1),
            seen: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}
impl Component for CommitEveryComponent {
    type Next = v4::Cursor;
    fn render_batch(&self, ctx: &RenderCtx, batch: &[&v4::Cursor]) -> Vec<Node<v4::Cursor>> {
        let n = self.seen.fetch_add(1, Ordering::Relaxed) + 1;
        if n % self.every == 0 {
            self.store.commit(ctx.expand_tick, None);
        }
        batch
            .iter()
            .map(|c| Node::Emit(Arc::new((*c).clone())))
            .collect()
    }
}

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Clone, Copy, Debug)]
enum Mode {
    Bare,
    Insert,
    Full,
}
impl Mode {
    fn parse(s: &str) -> Self {
        match s {
            "bare" => Mode::Bare,
            "insert" => Mode::Insert,
            "full" => Mode::Full,
            _ => panic!("bad mode"),
        }
    }
}

fn parse_lang(s: &str) -> SupportLang {
    match s {
        "c" | "cpp" | "c++" => SupportLang::Cpp,
        "rust" | "rs" => SupportLang::Rust,
        other => panic!("unknown --lang: {}", other),
    }
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

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut root = PathBuf::from("../v3/tests/smoke/.fixtures/linux");
    let mut workers: usize = 8;
    let mut trials: usize = 3;
    let mut pattern_src = String::from("printk($$$)");
    let mut lang_spec = String::from("c");
    let mut mode_str = String::from("bare");
    let mut batch: usize = 65536;
    let mut file: Option<PathBuf> = None;
    let mut multi: usize = 0;
    let mut multi_each: Vec<String> = Vec::new();
    let mut commit_every: usize = 0;
    let mut store_kind = String::from("mem");
    let mut sqlite_path: Option<PathBuf> = None;
    let mut sprf_store_enabled = false;
    let mut warm_page_cache = false;
    // --memoize off|on|reconcile. Wraps AstNm in Memoize.
    let mut memoize = String::from("off");
    let mut memoize_share = false;

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
            "--pattern" => {
                pattern_src = args[i + 1].clone();
                i += 2;
            }
            "--lang" => {
                lang_spec = args[i + 1].clone();
                i += 2;
            }
            "--mode" => {
                mode_str = args[i + 1].clone();
                i += 2;
            }
            "--batch" => {
                batch = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--file" => {
                file = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--multi" => {
                multi = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--pattern-each" => {
                multi_each.push(args[i + 1].clone());
                i += 2;
            }
            "--commit-every" => {
                commit_every = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--store" => {
                store_kind = args[i + 1].clone();
                i += 2;
            }
            "--sqlite-path" => {
                sqlite_path = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--materialize-read" => {
                panic!("--materialize-read was removed; v4-bench uses source-aware ast reads");
            }
            "--sprf-store" => {
                sprf_store_enabled = true;
                i += 1;
            }
            "--warm-page-cache" => {
                warm_page_cache = true;
                i += 1;
            }
            "--memoize" => {
                memoize = args[i + 1].clone();
                i += 2;
            }
            "--memoize-share" => {
                memoize_share = true;
                i += 1;
            }
            other => panic!("unknown arg: {}", other),
        }
    }
    let mode = Mode::parse(&mode_str);
    let lang = parse_lang(&lang_spec);
    let exts: Vec<String> = match lang_spec.as_str() {
        "c" | "cpp" | "c++" => vec!["c".into(), "h".into()],
        "rust" | "rs" => vec!["rs".into()],
        _ => panic!(),
    };

    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build_global()
        .ok();

    eprintln!(
        "mode={:?} workers={} batch={} trials={} pattern={:?} lang={:?} root={} memoize={} memoize_share={} sprf_store={} warm_page_cache={}",
        mode, workers, batch, trials, pattern_src, lang, root.display(), memoize, memoize_share, sprf_store_enabled, warm_page_cache,
    );
    eprintln!("ext_filter={:?} source_reads=ast", exts,);
    if !matches!(memoize.as_str(), "off" | "on" | "reconcile") {
        panic!("--memoize must be off|on|reconcile, got {memoize:?}");
    }
    if matches!(mode, Mode::Full) {
        panic!("--mode full not yet ported (rule infra not yet landed on v2)");
    }
    if warm_page_cache && file.is_none() {
        let t_enum = Instant::now();
        let paths = enumerate_paths(&root, &exts);
        let enumerate_ms = t_enum.elapsed().as_secs_f64() * 1000.0;
        let t_warm = Instant::now();
        let mut warm_bytes: u64 = 0;
        for p in &paths {
            if let Ok(bytes) = std::fs::read(p) {
                warm_bytes += bytes.len() as u64;
            }
        }
        let warm_ms = t_warm.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "warmup files={} MB={:.1} enumerate_ms={:.1} page_cache_ms={:.1}",
            paths.len(),
            warm_bytes as f64 / (1024.0 * 1024.0),
            enumerate_ms,
            warm_ms,
        );
    }
    // Shared cache + index across trials when --memoize-share.
    let shared_cache: Option<Arc<MemoCache<v4::Cursor>>> = if memoize_share && memoize != "off" {
        Some(Arc::new(MemoCache::new()))
    } else {
        None
    };
    let shared_idx: Option<Arc<PriorChildIndex>> = if memoize_share && memoize == "reconcile" {
        Some(Arc::new(PriorChildIndex::new()))
    } else {
        None
    };

    let mut walls = Vec::new();
    let mut last_matches: u64 = 0;
    for trial in 1..=trials {
        let counter = Arc::new(AtomicU64::new(0));
        let bench = Arc::new(BenchCounters::default());
        let fs_telemetry = Arc::new(FsTelemetry::default());
        let ast_telemetry = Arc::new(AstTelemetry::default());
        let mut timings: Vec<Arc<StageTiming>> = Vec::new();
        let sprf_store = if sprf_store_enabled {
            let backing: Arc<dyn FactStore<v4::Cursor>> =
                Arc::new(MemFactStore::<v4::Cursor>::new());
            Some(SprfStore::new(backing))
        } else {
            None
        };

        // Source: SinglePath (--file) or Fs (bulk corpus).
        let source: Arc<dyn Component<Next = v4::Cursor>> = match &file {
            Some(p) => Arc::new(SinglePathComponent::new(p.clone())),
            None => {
                let mut comp = FsComponent::new(root.clone(), batch)
                    .with_include_exts(exts.clone())
                    .with_telemetry(fs_telemetry.clone());
                if let Some(s) = &sprf_store {
                    comp = comp.with_sprf_store(s.clone());
                }
                Arc::new(comp)
            }
        };

        // Matcher: Multi(Ast)Nm if --multi/--pattern-each, else AstNm
        // wrapped in optional Memoize.
        let matcher: Arc<dyn Component<Next = v4::Cursor>> = if !multi_each.is_empty() {
            let pats: Vec<(String, String)> = multi_each
                .iter()
                .enumerate()
                .map(|(i, p)| (format!("p{}", i), p.clone()))
                .collect();
            Arc::new(MultiAstNmComponent::new(pats, lang))
        } else if multi > 0 {
            let pats: Vec<(String, String)> = (0..multi)
                .map(|i| (format!("p{}", i), pattern_src.clone()))
                .collect();
            Arc::new(MultiAstNmComponent::new(pats, lang))
        } else {
            match memoize.as_str() {
                "off" => {
                    let mut comp = AstNmComponent::new(pattern_src.clone(), lang)
                        .with_telemetry(ast_telemetry.clone());
                    if let Some(s) = &sprf_store {
                        comp = comp.with_sprf_store(s.clone());
                    }
                    Arc::new(comp)
                }
                "on" => {
                    let cache = shared_cache
                        .clone()
                        .unwrap_or_else(|| Arc::new(MemoCache::new()));
                    let mut comp = AstNmComponent::new(pattern_src.clone(), lang)
                        .with_telemetry(ast_telemetry.clone());
                    if let Some(s) = &sprf_store {
                        comp = comp.with_sprf_store(s.clone());
                    }
                    Arc::new(Memoize::new(comp, "astnm", cache).with_domain("fs"))
                }
                "reconcile" => {
                    let cache = shared_cache
                        .clone()
                        .unwrap_or_else(|| Arc::new(MemoCache::new()));
                    let idx = shared_idx
                        .clone()
                        .unwrap_or_else(|| Arc::new(PriorChildIndex::new()));
                    let mut comp = AstNmComponent::new(pattern_src.clone(), lang)
                        .with_telemetry(ast_telemetry.clone());
                    if let Some(s) = &sprf_store {
                        comp = comp.with_sprf_store(s.clone());
                    }
                    Arc::new(
                        Memoize::new(comp, "astnm", cache)
                            .with_domain("fs")
                            .with_prior_children(idx),
                    )
                }
                _ => unreachable!(),
            }
        };

        // Optional FactStore for Insert mode. Schema is declared once
        // up front so SqliteFactStore can materialize the table.
        let store_opt: Option<Arc<dyn FactStore<v4::Cursor>>> = if matches!(mode, Mode::Insert) {
            let store: Arc<dyn FactStore<v4::Cursor>> = match store_kind.as_str() {
                "mem" => Arc::new(MemFactStore::<v4::Cursor>::new()),
                "sqlite-mem" => Arc::new(
                    SqliteFactStore::<v4::Cursor>::open_in_memory().expect("sqlite open_in_memory"),
                ),
                "sqlite-disk" => {
                    let path = sqlite_path.clone().unwrap_or_else(|| {
                        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                        PathBuf::from(home)
                            .join(".cache/sprefa")
                            .join("v4-bench.db")
                    });
                    if trial == 1 {
                        let _ = std::fs::remove_file(&path);
                        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
                        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
                    }
                    Arc::new(
                        SqliteFactStore::<v4::Cursor>::open_file(&path).expect("sqlite open_file"),
                    )
                }
                other => panic!("--store must be mem|sqlite-mem|sqlite-disk, got {}", other),
            };
            store.declare("matches", &["FS", "CONTENT_HASH", "LO", "HI"]);
            Some(store)
        } else {
            None
        };

        // Build chain:
        //   source > matcher > Count
        //
        // Source bytes stay behind ast instead of being serialized through
        // cursor.value between queue stages.
        let mut steps: Vec<Arc<dyn Component<Next = v4::Cursor>>> = vec![
            TimedComponent::new("fs", source, &mut timings),
            TimedComponent::new(
                "count_fs_rows",
                Arc::new(RowCountComponent {
                    rows: bench.source_rows.clone(),
                }),
                &mut timings,
            ),
        ];
        steps.push(TimedComponent::new("ast", matcher, &mut timings));
        if matches!(mode, Mode::Insert) {
            let store = store_opt.as_ref().unwrap().clone();
            steps.push(TimedComponent::new(
                "fact_write",
                Arc::new(FactWrite::new(store.clone(), "matches")),
                &mut timings,
            ));
            if commit_every > 0 {
                steps.push(TimedComponent::new(
                    "commit_every",
                    Arc::new(CommitEveryComponent::new(store.clone(), commit_every)),
                    &mut timings,
                ));
            }
            steps.push(TimedComponent::new(
                "count_matches",
                Arc::new(CountComponent {
                    count: counter.clone(),
                }),
                &mut timings,
            ));
        } else {
            steps.push(TimedComponent::new(
                "count_matches",
                Arc::new(CountComponent {
                    count: counter.clone(),
                }),
                &mut timings,
            ));
        }

        let pipe = PipeInstance::new(steps);
        let queue: Arc<dyn QueueBackend<v4::Cursor>> = Arc::new(MemQueue::new());
        let effect_telemetry = Arc::new(EffectTelemetry::default());
        let opts = ExpandOpts::default()
            .with_batch_cap(batch)
            .with_telemetry(effect_telemetry.clone());

        let t_run = Instant::now();
        let stats = expand(&pipe, queue, vec![Arc::new(v4::Cursor::default())], opts);
        if let Some(store) = &store_opt {
            store.commit(trial as u64, None);
        }
        let wall = t_run.elapsed();

        let m = counter.load(Ordering::Relaxed);
        let rss = rss_peak_kb() / 1024;
        eprintln!(
            "trial {}: wall={:.3}s  matches={:>9}  fs_seen={}  fs_rows={}  fs_ext_skipped={}  fs_filter_skipped={}  fs_walk_ms={:.1}  fs_filter_ms={:.1}  fs_cursor_ms={:.1}  fs_splice_ms={:.1}  rendered={}  emitted={}  rss_peak_MB={}",
            trial,
            wall.as_secs_f64(),
            m,
            fs_telemetry.seen_files.load(Ordering::Relaxed),
            bench.source_rows.load(Ordering::Relaxed),
            fs_telemetry.ext_skipped.load(Ordering::Relaxed),
            fs_telemetry.filter_skipped.load(Ordering::Relaxed),
            fs_telemetry.walk_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            fs_telemetry.filter_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            fs_telemetry.cursor_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            fs_telemetry.splice_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            stats.rendered,
            stats.emitted,
            rss,
        );
        for timing in &timings {
            let wall_ms = timing.wall_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
            eprintln!(
                "  stage {:>14}: calls={} rows={} avg_batch={:.1} max_batch={} wall_ms={:.1}",
                timing.name,
                timing.calls.load(Ordering::Relaxed),
                timing.rows.load(Ordering::Relaxed),
                if timing.calls.load(Ordering::Relaxed) == 0 {
                    0.0
                } else {
                    timing.rows.load(Ordering::Relaxed) as f64
                        / timing.calls.load(Ordering::Relaxed) as f64
                },
                timing.max_batch.load(Ordering::Relaxed),
                wall_ms,
            );
        }
        eprintln!(
            "  ast telemetry: inputs={} source_reads={} source_MB={:.1} source_utf8={} source_utf8_MB={:.1} source_errors={} legacy_rows={} prefilter_skips={} parses={} matches={} pattern_ms={:.1} read_ms={:.1} prefilter_ms={:.1} utf8_ms={:.1} legacy_ms={:.1} parse_ms={:.1} match_ms={:.1} emit_stamp_ms={:.1}",
            ast_telemetry.input_rows.load(Ordering::Relaxed),
            ast_telemetry.source_read_rows.load(Ordering::Relaxed),
            ast_telemetry.source_read_bytes.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0),
            ast_telemetry.source_utf8_rows.load(Ordering::Relaxed),
            ast_telemetry.source_utf8_bytes.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0),
            ast_telemetry.source_read_errors.load(Ordering::Relaxed),
            ast_telemetry.legacy_rows.load(Ordering::Relaxed),
            ast_telemetry.prefilter_skips.load(Ordering::Relaxed),
            ast_telemetry.parses.load(Ordering::Relaxed),
            ast_telemetry.matches.load(Ordering::Relaxed),
            ast_telemetry.pattern_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            ast_telemetry.source_read_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            ast_telemetry.prefilter_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            ast_telemetry.utf8_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            ast_telemetry.legacy_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            ast_telemetry.parse_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            ast_telemetry.match_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
            ast_telemetry.emit_stamp_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0,
        );
        if let Some(store) = &store_opt {
            if let Some(s) = store.stats() {
                eprintln!(
                    "  store telemetry: insert_calls={} insert_rows={} insert_batch_calls={} insert_batch_rows={} commit_calls={} string_intern_calls={} string_intern_rows={} transactions={} insert_batch_ms={:.1} string_intern_ms={:.1} sqlite_insert_ms={:.1} transaction_commit_ms={:.1} commit_ms={:.1}",
                    s.insert_calls,
                    s.insert_rows,
                    s.insert_batch_calls,
                    s.insert_batch_rows,
                    s.commit_calls,
                    s.string_intern_calls,
                    s.string_intern_rows,
                    s.transactions,
                    s.insert_batch_ms,
                    s.string_intern_ms,
                    s.sqlite_insert_ms,
                    s.transaction_commit_ms,
                    s.commit_ms,
                );
            }
        }
        let effect = effect_telemetry.snapshot();
        eprintln!(
            "  effect runtime: pull_calls={} pull_rows={} pull_ms={:.1} dispatch_calls={} dispatch_rows={} dispatch_ms={:.1} put_calls={} put_ms={:.1}",
            effect.pull_calls,
            effect.pull_rows,
            effect.pull_ns as f64 / 1_000_000.0,
            effect.dispatch_calls,
            effect.dispatch_rows,
            effect.dispatch_ns as f64 / 1_000_000.0,
            effect.put_calls,
            effect.put_ns as f64 / 1_000_000.0,
        );
        walls.push(wall);
        last_matches = m;
    }
    walls.sort();
    let med = walls[walls.len() / 2];
    eprintln!("───────────────────────────────────────────────────────────────────────────");
    eprintln!(
        "median:  wall={:.3}s  matches={}",
        med.as_secs_f64(),
        last_matches
    );
}

fn enumerate_paths(root: &std::path::Path, exts: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
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
        if exts.iter().any(|want| want.eq_ignore_ascii_case(ext)) {
            out.push(p);
        }
    }
    out
}
