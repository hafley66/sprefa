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
// Default bulk shape is now source-aware `fs > ast`; file bodies stay
// behind the ast operator. Use `--materialize-read` to measure the old
// explicit `fs > read > ast` materialization boundary.
//
// Usage:
//   cargo run --release --bin v4-bench -- --root <linux> \
//     --workers 8 --trials 3 --pattern 'printk($$$)' --lang c --mode bare

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ast_grep_language::SupportLang;

use effect_runtime::v2::{
    expand, BarrierScope, Component, ComponentLifecycle, ExpandOpts, FactStore,
    MemFactStore, MemQueue, MemoCache, Memoize, Node, PendingSummary,
    PipeInstance, PriorChildIndex, QueueBackend, QueueRow, RenderCtx,
    SqliteFactStore,
};
use v4::v2_ops::{
    AstNmComponent, CountComponent, FsComponent, MultiAstNmComponent, ReadComponent,
    SinglePathComponent,
};
use v4::fact::FactWrite;

#[derive(Default)]
struct BenchCounters {
    source_rows:  Arc<AtomicU64>,
    ext_kept:     Arc<AtomicU64>,
    ext_dropped:  Arc<AtomicU64>,
    read_rows:    Arc<AtomicU64>,
    read_bytes:   Arc<AtomicU64>,
}

#[derive(Default)]
struct StageTiming {
    name:    &'static str,
    calls:   AtomicU64,
    rows:    AtomicU64,
    wall_ns: AtomicU64,
}

impl StageTiming {
    fn new(name: &'static str) -> Self {
        Self { name, ..Default::default() }
    }
}

struct TimedComponent {
    inner:  Arc<dyn Component<Next = v4::Cursor>>,
    timing: Arc<StageTiming>,
}

impl TimedComponent {
    fn new(
        name: &'static str,
        inner: Arc<dyn Component<Next = v4::Cursor>>,
        timings: &mut Vec<Arc<StageTiming>>,
    ) -> Arc<dyn Component<Next = v4::Cursor>> {
        let timing = Arc::new(StageTiming::new(name));
        timings.push(timing.clone());
        Arc::new(Self { inner, timing })
    }
}

impl Component for TimedComponent {
    type Next = v4::Cursor;

    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<v4::Cursor>],
        queue: &dyn QueueBackend<v4::Cursor>,
    ) {
        let t0 = Instant::now();
        self.inner.dispatch(ctx, rows, queue);
        let dt = t0.elapsed();
        self.timing.calls.fetch_add(1, Ordering::Relaxed);
        self.timing.rows.fetch_add(rows.len() as u64, Ordering::Relaxed);
        self.timing.wall_ns.fetch_add(dt.as_nanos() as u64, Ordering::Relaxed);
    }

    fn batch_hint(&self) -> Option<usize> { self.inner.batch_hint() }
    fn lifecycle(&self) -> ComponentLifecycle { self.inner.lifecycle() }
    fn idle(
        &self,
        ctx:     &RenderCtx,
        scope:   BarrierScope,
        pending: PendingSummary,
        queue:   &dyn QueueBackend<v4::Cursor>,
    ) {
        self.inner.idle(ctx, scope, pending, queue);
    }
    fn complete(
        &self,
        ctx:   &RenderCtx,
        scope: BarrierScope,
        queue: &dyn QueueBackend<v4::Cursor>,
    ) {
        self.inner.complete(ctx, scope, queue);
    }
    fn kind(&self) -> &'static str { self.timing.name }
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

/// Bench-local extension filter. V3 enumerates `.c/.h` before dispatch;
/// this keeps the v4 bench measuring the same corpus without changing
/// language-level `fs` semantics.
struct ExtFilterComponent {
    exts:    Arc<Vec<String>>,
    kept:    Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
}

impl Component for ExtFilterComponent {
    type Next = v4::Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &v4::Cursor) -> Node<v4::Cursor> {
        let path = std::path::Path::new(c.value.as_ref());
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return Node::Done;
        };
        let keep = self.exts.iter().any(|want| want.eq_ignore_ascii_case(ext));
        if keep {
            self.kept.fetch_add(1, Ordering::Relaxed);
            Node::Emit(Arc::new(c.clone()))
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            Node::Done
        }
    }
}

struct ReadTelemetryComponent {
    rows:  Arc<AtomicU64>,
    bytes: Arc<AtomicU64>,
}

impl Component for ReadTelemetryComponent {
    type Next = v4::Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &v4::Cursor) -> Node<v4::Cursor> {
        self.rows.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(c.value.len() as u64, Ordering::Relaxed);
        Node::Emit(Arc::new(c.clone()))
    }
}

/// Bench-local: emit each cursor through, and call `store.commit(gen, None)`
/// every Nth render_batch. Drives commit-cost telemetry in Insert mode.
struct CommitEveryComponent {
    store: Arc<dyn FactStore<v4::Cursor>>,
    every: usize,
    seen:  std::sync::atomic::AtomicUsize,
}
impl CommitEveryComponent {
    fn new(store: Arc<dyn FactStore<v4::Cursor>>, every: usize) -> Self {
        Self { store, every: every.max(1), seen: std::sync::atomic::AtomicUsize::new(0) }
    }
}
impl Component for CommitEveryComponent {
    type Next = v4::Cursor;
    fn render_batch(&self, ctx: &RenderCtx, batch: &[&v4::Cursor]) -> Vec<Node<v4::Cursor>> {
        let n = self.seen.fetch_add(1, Ordering::Relaxed) + 1;
        if n % self.every == 0 {
            self.store.commit(ctx.expand_tick, None);
        }
        batch.iter().map(|c| Node::Emit(Arc::new((*c).clone()))).collect()
    }
}

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
    let mut batch: usize = 4096;
    let mut file: Option<PathBuf> = None;
    let mut multi: usize = 0;
    let mut multi_each: Vec<String> = Vec::new();
    let mut commit_every: usize = 0;
    let mut store_kind = String::from("mem");
    let mut sqlite_path: Option<PathBuf> = None;
    let mut materialize_read = false;
    // --memoize off|on|reconcile. Wraps AstNm in Memoize.
    let mut memoize = String::from("off");
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
            "--batch"   => { batch       = args[i+1].parse().unwrap(); i += 2; }
            "--file"    => { file        = Some(PathBuf::from(&args[i+1])); i += 2; }
            "--multi"   => { multi       = args[i+1].parse().unwrap(); i += 2; }
            "--pattern-each" => { multi_each.push(args[i+1].clone()); i += 2; }
            "--commit-every" => { commit_every = args[i+1].parse().unwrap(); i += 2; }
            "--store"        => { store_kind   = args[i+1].clone(); i += 2; }
            "--sqlite-path"  => { sqlite_path  = Some(PathBuf::from(&args[i+1])); i += 2; }
            "--materialize-read" => { materialize_read = true; i += 1; }
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
        "mode={:?} workers={} batch={} trials={} pattern={:?} lang={:?} root={} memoize={} memoize_share={}",
        mode, workers, batch, trials, pattern_src, lang, root.display(), memoize, memoize_share,
    );
    eprintln!(
        "ext_filter={:?} materialize_read={}",
        exts, materialize_read,
    );
    if !matches!(memoize.as_str(), "off" | "on" | "reconcile") {
        panic!("--memoize must be off|on|reconcile, got {memoize:?}");
    }
    if matches!(mode, Mode::Full) {
        panic!("--mode full not yet ported (rule infra not yet landed on v2)");
    }
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
        let bench = Arc::new(BenchCounters::default());
        let mut timings: Vec<Arc<StageTiming>> = Vec::new();

        // Source: SinglePath (--file) or Fs (bulk corpus).
        let source: Arc<dyn Component<Next = v4::Cursor>> = match &file {
            Some(p) => Arc::new(SinglePathComponent::new(p.clone())),
            None    => Arc::new(FsComponent::new(root.clone(), batch)),
        };

        // Matcher: Multi(Ast)Nm if --multi/--pattern-each, else AstNm
        // wrapped in optional Memoize.
        let matcher: Arc<dyn Component<Next = v4::Cursor>> = if !multi_each.is_empty() {
            let pats: Vec<(String, String)> = multi_each.iter().enumerate()
                .map(|(i, p)| (format!("p{}", i), p.clone())).collect();
            Arc::new(MultiAstNmComponent::new(pats, lang))
        } else if multi > 0 {
            let pats: Vec<(String, String)> = (0..multi)
                .map(|i| (format!("p{}", i), pattern_src.clone())).collect();
            Arc::new(MultiAstNmComponent::new(pats, lang))
        } else {
            match memoize.as_str() {
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
            }
        };

        // Optional FactStore for Insert mode. Schema is declared once
        // up front so SqliteFactStore can materialize the table.
        let store_opt: Option<Arc<dyn FactStore<v4::Cursor>>> =
            if matches!(mode, Mode::Insert) {
                let store: Arc<dyn FactStore<v4::Cursor>> = match store_kind.as_str() {
                    "mem"        => Arc::new(MemFactStore::<v4::Cursor>::new()),
                    "sqlite-mem" => Arc::new(
                        SqliteFactStore::<v4::Cursor>::open_in_memory()
                            .expect("sqlite open_in_memory")),
                    "sqlite-disk" => {
                        let path = sqlite_path.clone().unwrap_or_else(|| {
                            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                            PathBuf::from(home).join(".cache/sprefa").join("v4-bench.db")
                        });
                        if trial == 1 {
                            let _ = std::fs::remove_file(&path);
                            let _ = std::fs::remove_file(format!("{}-wal", path.display()));
                            let _ = std::fs::remove_file(format!("{}-shm", path.display()));
                        }
                        Arc::new(
                            SqliteFactStore::<v4::Cursor>::open_file(&path)
                                .expect("sqlite open_file"))
                    }
                    other => panic!("--store must be mem|sqlite-mem|sqlite-disk, got {}", other),
                };
                store.declare("matches", &["FS", "CONTENT_HASH", "LO", "HI"]);
                Some(store)
            } else { None };

        // Build chain:
        //   default             source > matcher > Count
        //   --materialize-read  source > read > matcher > Count
        //
        // The default exercises the source-aware matcher path: source
        // bytes stay behind ast instead of being serialized through
        // cursor.value between queue stages.
        let mut steps: Vec<Arc<dyn Component<Next = v4::Cursor>>> = vec![
            TimedComponent::new("fs", source, &mut timings),
            TimedComponent::new(
                "count_fs_rows",
                Arc::new(RowCountComponent { rows: bench.source_rows.clone() }),
                &mut timings,
            ),
        ];
        if file.is_none() {
            steps.push(TimedComponent::new(
                "ext_filter",
                Arc::new(ExtFilterComponent {
                    exts: Arc::new(exts.clone()),
                    kept: bench.ext_kept.clone(),
                    dropped: bench.ext_dropped.clone(),
                }),
                &mut timings,
            ));
        }
        if materialize_read {
            let read: Arc<dyn Component<Next = v4::Cursor>> = Arc::new(ReadComponent::new());
            steps.push(TimedComponent::new("read", read, &mut timings));
            steps.push(TimedComponent::new(
                "read_telemetry",
                Arc::new(ReadTelemetryComponent {
                    rows: bench.read_rows.clone(),
                    bytes: bench.read_bytes.clone(),
                }),
                &mut timings,
            ));
        }
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
                Arc::new(CountComponent { count: counter.clone() }),
                &mut timings,
            ));
        } else {
            steps.push(TimedComponent::new(
                "count_matches",
                Arc::new(CountComponent { count: counter.clone() }),
                &mut timings,
            ));
        }

        let pipe = PipeInstance::new(steps);
        let queue: Arc<dyn QueueBackend<v4::Cursor>> = Arc::new(MemQueue::new());
        let opts = ExpandOpts::default().with_batch_cap(batch.max(65536));

        let t_run = Instant::now();
        let stats = expand(&pipe, queue, vec![Arc::new(v4::Cursor::default())], opts);
        if let Some(store) = &store_opt {
            store.commit(trial as u64, None);
        }
        let wall = t_run.elapsed();

        let m = counter.load(Ordering::Relaxed);
        let rss = rss_peak_kb() / 1024;
        eprintln!(
            "trial {}: wall={:.3}s  matches={:>9}  fs_rows={}  ext_kept={}  ext_dropped={}  read_rows={}  read_MB={:.1}  rendered={}  emitted={}  rss_peak_MB={}",
            trial,
            wall.as_secs_f64(),
            m,
            bench.source_rows.load(Ordering::Relaxed),
            bench.ext_kept.load(Ordering::Relaxed),
            bench.ext_dropped.load(Ordering::Relaxed),
            bench.read_rows.load(Ordering::Relaxed),
            bench.read_bytes.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0),
            stats.rendered,
            stats.emitted,
            rss,
        );
        for timing in &timings {
            let wall_ms = timing.wall_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
            eprintln!(
                "  stage {:>14}: calls={} rows={} wall_ms={:.1}",
                timing.name,
                timing.calls.load(Ordering::Relaxed),
                timing.rows.load(Ordering::Relaxed),
                wall_ms,
            );
        }
        walls.push(wall);
        last_matches = m;
    }
    walls.sort();
    let med = walls[walls.len()/2];
    eprintln!("───────────────────────────────────────────────────────────────────────────");
    eprintln!("median:  wall={:.3}s  matches={}", med.as_secs_f64(), last_matches);
}
