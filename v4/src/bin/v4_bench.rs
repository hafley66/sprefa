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

    eprintln!("mode={:?} workers={} cap={} batch={} trials={} pattern={:?} lang={:?} root={}",
              mode, workers, cap, batch, trials, pattern_src, lang, root.display());

    let mut walls = Vec::new();
    let mut last = (0u64, 0u64);

    for trial in 1..=trials {
        let mem = MemStore::new();
        let store: Arc<dyn Store> = mem.clone();
        if matches!(mode, Mode::Full) {
            for r in 0..rules {
                store.define_rule(&format!("hot_pattern_{}", r), RuleBody::GroupCount {
                    src: "matches".into(), key: "FS".into(),
                    min: 2, count_term: "COUNT".into(),
                });
            }
        }

        let matches    = Arc::new(AtomicU64::new(0));
        let bytes_seen = Arc::new(AtomicU64::new(0));

        let (eff_tx, mut eff_rx) = mpsc::unbounded_channel();
        let saga = tokio::spawn(async move { while eff_rx.recv().await.is_some() {} });

        let tele = Telemetry::new();
        mem.attach_tele(tele.clone());
        let hooks = Hooks {
            store:   store.clone(),
            effects: eff_tx.clone(),
            gen:     trial as u64,
            lineage: new_lineage(),
            tele:    tele.clone(),
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
                chain.push(Arc::new(Fact { name: "matches".into() }));
                if matches!(mode, Mode::Full) && commit_every > 0 {
                    chain.push(Arc::new(CommitEvery::new(commit_every)));
                }
            }
        }

        let t_run = Instant::now();
        drive_with(chain, hooks, cap).await;
        if matches!(mode, Mode::Full) { store.commit(trial as u64).await; }
        let wall = t_run.elapsed();

        drop(eff_tx);
        let _ = saga.await;

        let m  = matches.load(Ordering::Relaxed);
        let _b = bytes_seen.load(Ordering::Relaxed);
        let files_s = "n/a"; // Fs walks internally; we don't enumerate up-front
        let rss = rss_peak_kb() / 1024;
        eprintln!("trial {}: wall={:.3}s  matches={:>9}  files/s={}  rss_peak_MB={}",
                  trial, wall.as_secs_f64(), m, files_s, rss);
        eprint!("{}", tele.summary());
        if !matches!(mode, Mode::Bare) {
            eprint!("\n{}", mem.stats().await.summary());
        }
        walls.push(wall);
        last = (0, m);
    }

    walls.sort();
    let med = walls[walls.len()/2];
    eprintln!("───────────────────────────────────────────────────────────────────────────");
    eprintln!("median:  wall={:.3}s  matches={}", med.as_secs_f64(), last.1);
}
