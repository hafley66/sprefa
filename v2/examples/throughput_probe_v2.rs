//! throughput_probe_v2 — scaffold introducing two swappable axes on top of the
//! v1 probe: `EffectCost` (per-dispatch work shape) and `ArrivalGen` (producer
//! pacing). The v1 probe in `throughput_probe.rs` is untouched; this file
//! duplicates the few primitives it needs so the two live side by side.
//!
//! Two axes the scaffold makes swappable:
//!
//!   --effect   EffectCost impl — what per-dispatch actually costs
//!       synthetic:<shape>[,k=v,...]   Linear | Flat | Quadratic sleep model
//!           shape ∈ { linear | flat:US | quad:COEFF }
//!           kvs:   fixed=US, work=real|fixed/US|pareto/MED/SHAPEX10
//!           (values inside a kv use `/` so `:` stays shape-reserved)
//!       git-odb:fixed_us=N[,scan=bool]  Mutex-serialized fixed cost (mock ODB)
//!       sqlite:tx_fixed_us=N,per_row_us=M[,scan=bool]  stepped sleep model
//!       rayon-ast-grep                par_iter scan (already-parallel baseline)
//!
//!       git-odb-real:repo=PATH[,oid_cap=N,scan=bool]
//!           real git2 ODB reads. Walks HEAD tree at construct time to
//!           collect up to oid_cap blob OIDs, then cycles through them
//!           inside a Mutex<Repository>::odb().read(oid) loop. If scan=true,
//!           also runs ast-grep on the input PathBuf (outside the ODB lock).
//!
//!       sqlite-real[:k=v,...]    real rusqlite :memory: BEGIN/INSERT/COMMIT
//!           kvs: journal=memory|wal|off, sync=off|normal|full, scan=bool
//!           Single shared Mutex<Connection> models the write-serialized
//!           production path. Schema: hits (id, path, bytes, matches).
//!
//!   --arrival  ArrivalGen impl — how items arrive at the consumer
//!       dirac                         zero inter-arrival (as-fast-as-possible)
//!       fixed:US                      deterministic gap
//!       pareto:MED:SHAPEX10           heavy-tail inter-arrival
//!       burst:COUNT:GAP_US            N back-to-back, sleep GAP, repeat
//!       sine:period=P:amp=A:base=B    oscillatory (exposes buffer_count stalls)
//!       ar1:base=B:rho=R:sd=S         autocorrelated (realistic LSP arrival)
//!       stepfn:UNTIL1:GAP1,UNTIL2:GAP2,...   piecewise-constant rate schedule
//!
//!   --policy   BatchPolicy impl — how the consumer groups incoming items
//!       opportunistic         recv + try_recv drain until max_batch or empty
//!                             (default; collapses to 1-item batches when
//!                              arrival is paced)
//!       windowed:US           throttle-time buffer — after first item,
//!                             wait up to US µs for more items, flush on
//!                             max_batch or deadline. Decouples batch size
//!                             from instantaneous arrival rate.
//!
//! One topology in this scaffold: Hopp-shape (crossbeam bounded + W workers).
//! Composes `Arc<dyn EffectCost>` + `Box<dyn ArrivalGen>` +
//! `Arc<dyn BatchPolicy>` with no per-variant function.
//!
//!   --ext      file extension filter on enumeration
//!       js-ts        .js|.jsx|.ts|.tsx|.mjs|.cjs  (default)
//!       any          every file
//!       a,b,c        comma-separated whitelist (lowercase, no dot)
//!
//! Follow-ups (out of scope here): real rusqlite in SqliteInsertTx, real
//! git2 ODB read in GitOdbRead, Stack trait (tokio::mpsc / flume).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use ast_grep_core::{AstGrep, Language, Pattern, source::StrDoc};
use ast_grep_language::SupportLang;
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::cell::RefCell;
thread_local! {
    // Reusable tree-sitter Parser per thread. Allocated once per worker-thread
    // first call; subsequent calls parse without re-allocating. Eliminates
    // the ~KB-per-file malloc that ast-grep's lang.ast_grep() does internally.
    static TS_PARSER: RefCell<Option<(SupportLang, tree_sitter::Parser)>> =
        RefCell::new(None);
}
fn parse_with_reused(src: &str, lang: SupportLang) -> Option<tree_sitter::Tree> {
    // This path intentionally only supports Cpp — it's the lens for the
    // ast-grep-per-file-Parser-alloc probe. Adding more languages means
    // adding more grammar dev-deps. Not the goal here.
    if !matches!(lang, SupportLang::Cpp) { return None; }
    TS_PARSER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.as_ref().map(|(l, _)| *l != lang).unwrap_or(true) {
            let mut p = tree_sitter::Parser::new();
            let ts_lang: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
            p.set_language(&ts_lang).ok()?;
            *slot = Some((lang, p));
        }
        let (_, parser) = slot.as_mut().unwrap();
        parser.parse(src.as_bytes(), None)
    })
}

// Active scan language, set once from --lang at main(). scan_one reads it.
// Using a static avoids threading a Language param through every EffectCost
// impl; language is uniform across a probe run.
static SCAN_LANG: OnceLock<SupportLang> = OnceLock::new();
fn scan_lang() -> SupportLang { SCAN_LANG.get().copied().unwrap_or(SupportLang::Tsx) }

// ============================================================================
// Primitives copied from v1 (keep in sync by convention, not by link).
// ============================================================================

fn pattern_src(tag: &str) -> &'static str {
    match tag {
        "baseline" => "console.log($$$)",
        "medium"   => "useEffect(() => { $$$ })",
        "heavy"    => "function $NAME($$$) { $$$ }",
        "nomatch"  => "___never_match_sigil_xyz_$Q___",
        // C/C++ patterns for linux kernel scans
        "c-printk" => "printk($$$)",
        "c-if"     => "if ($$$) { $$$ }",
        "c-func"   => "$RET $NAME($$$) { $$$ }",
        other => panic!("unknown pattern tag: {}", other),
    }
}

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

fn scan_one(path: &Path, pattern: &Pattern<SupportLang>, lang: SupportLang) -> (u64, u64) {
    let Ok(src) = std::fs::read_to_string(path) else { return (0, 0) };
    let bytes = src.len() as u64;
    // Fixed-string pre-filter, same shape as ast-grep CLI's filter_file_pattern:
    // skip the CST walk entirely when the pattern's longest literal substring
    // is absent from the file bytes. SIMD byte-search is ~100x cheaper than
    // tree-sitter parse on a miss; on corpora where most files don't hit, this
    // is the dominant lever. Toggled off via SPRF_NO_FIXED_PREFILTER env.
    if PREFILTER_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        let fixed = pattern.fixed_string();
        if !fixed.is_empty() && !src.contains(&*fixed) {
            return (bytes, 0);
        }
    }
    let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(&src);
    let matches = grep.root().find_all(pattern).count() as u64;
    (bytes, matches)
}

static PREFILTER_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

// Breakdown scan_one into phases so we can see where the 1.69ms/file goes.
// Returns (bytes, matches, read_ns, parse_ns, match_ns, ts_only_ns).
// `ts_only_ns` parses the same src through a reused thread-local Parser
// (bypassing ast-grep's per-file Parser::new allocation) and walks the tree
// to count nodes. Gives a lower-bound on "pure tree-sitter parse" cost.
fn scan_one_timed(path: &Path, pattern: &Pattern<SupportLang>, lang: SupportLang)
    -> (u64, u64, u64, u64, u64, u64)
{
    let t_read = Instant::now();
    let Ok(src) = std::fs::read_to_string(path) else { return (0, 0, 0, 0, 0, 0) };
    let read_ns = t_read.elapsed().as_nanos() as u64;
    let bytes = src.len() as u64;
    let t_parse = Instant::now();
    let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(&src);
    let parse_ns = t_parse.elapsed().as_nanos() as u64;
    let t_match = Instant::now();
    let matches = grep.root().find_all(pattern).count() as u64;
    let match_ns = t_match.elapsed().as_nanos() as u64;
    let t_ts = Instant::now();
    let _tree = parse_with_reused(&src, lang);
    let ts_only_ns = t_ts.elapsed().as_nanos() as u64;
    (bytes, matches, read_ns, parse_ns, match_ns, ts_only_ns)
}

enum ExtFilter {
    JsTs,
    Any,
    List(Vec<String>),
}

impl ExtFilter {
    fn keep(&self, p: &Path) -> bool {
        match self {
            ExtFilter::Any => true,
            ExtFilter::JsTs => matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("js") | Some("jsx") | Some("ts") | Some("tsx") | Some("mjs") | Some("cjs")
            ),
            ExtFilter::List(exts) => p.extension().and_then(|s| s.to_str())
                .map(|e| exts.iter().any(|allow| allow == e))
                .unwrap_or(false),
        }
    }
    fn label(&self) -> String {
        match self {
            ExtFilter::JsTs => "js-ts".into(),
            ExtFilter::Any => "any".into(),
            ExtFilter::List(v) => v.join(","),
        }
    }
}

fn parse_ext_filter(spec: &str) -> ExtFilter {
    match spec {
        "js-ts" => ExtFilter::JsTs,
        "any"   => ExtFilter::Any,
        other   => ExtFilter::List(other.split(',').map(|s| s.to_string()).collect()),
    }
}

fn enumerate(root: &Path, cap: Option<usize>, ext: &ExtFilter) -> Vec<PathBuf> {
    enumerate_with_size(root, cap, ext, None)
}

// Size-filtered enumeration. `max_bytes` is a per-file ceiling; None = keep all.
// Enables stripping auto-generated / giant files out of perf-probe corpora
// without having to delete them from the fixture. Tree-sitter parse cost is
// superlinear in file size, so giant register-mask headers dominate wall time
// on real-repo fixtures (linux kernel: 14% of bytes in top-10 files).
fn enumerate_with_size(
    root: &Path, cap: Option<usize>, ext: &ExtFilter, max_bytes: Option<u64>,
) -> Vec<PathBuf> {
    let walker = WalkBuilder::new(root).standard_filters(true).build();
    let mut out: Vec<PathBuf> = Vec::new();
    for entry in walker.flatten() {
        if !entry.file_type().map_or(false, |t| t.is_file()) { continue; }
        let p = entry.into_path();
        if !ext.keep(&p) { continue; }
        if let Some(lim) = max_bytes {
            if std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > lim { continue; }
        }
        out.push(p);
        if let Some(n) = cap { if out.len() >= n { break; } }
    }
    out
}

#[inline]
fn xorshift64(s: &mut u64) -> u64 {
    let mut x = *s;
    x ^= x << 13; x ^= x >> 7; x ^= x << 17;
    *s = x;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

#[inline]
fn mix_seed(base: u64, tag: u64) -> u64 {
    let s = base ^ tag;
    if s == 0 { tag.wrapping_add(0xA5A5_A5A5_A5A5_A5A5) | 1 } else { s | 1 }
}

#[inline]
fn sample_unit(rng: &mut u64) -> f64 {
    (xorshift64(rng) >> 11) as f64 / (1u64 << 53) as f64
}

fn pareto_sample_us(rng: &mut u64, median_us: u64, shape_x10: u64) -> u64 {
    let u = sample_unit(rng);
    let alpha = (shape_x10.max(1) as f64) / 10.0;
    let xm = (median_us as f64) / 2f64.powf(1.0 / alpha);
    let denom = (1.0 - u).max(1e-12);
    (xm * denom.powf(-1.0 / alpha)).min(1.0e9) as u64
}

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

#[inline]
fn sleep_us(us: u64) { if us > 0 { std::thread::sleep(Duration::from_micros(us)); } }

// rss_kb — peak resident-set-size since process start, in KB.
// macOS: ru_maxrss is bytes → divide by 1024. Linux: already KB.
// Returns 0 on getrusage failure (non-fatal).
fn rss_peak_kb() -> u64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut ru) != 0 { return 0; }
        #[cfg(target_os = "macos")]
        { (ru.ru_maxrss as u64) / 1024 }
        #[cfg(not(target_os = "macos"))]
        { ru.ru_maxrss as u64 }
    }
}

// ============================================================================
// AXIS 1 — EffectCost
// ============================================================================
//
// A dispatch consumes a batch of paths and reports (bytes, matches,
// dispatch_us). How that cost scales with batch size is the effect-layer
// knob: linear per-item, flat group-commit, quadratic coordination, or a
// stepped cost gated by a global lock. All impls use sleep models unless
// noted; swap in real rusqlite / git2 in follow-up commits.

trait EffectCost: Send + Sync {
    fn dispatch(
        &self,
        batch: &[PathBuf],
        pat: &Pattern<SupportLang>,
        rng: &mut u64,
    ) -> (u64, u64, u64);
    fn name(&self) -> String;
}

// --- Synthetic: ports v1 Linear/Flat/Quadratic + WorkDist ------------------

#[derive(Clone, Copy, Debug)]
enum SyntheticShape {
    Linear,
    Flat { flat_us: u64 },
    Quadratic { coeff_us: u64 },
}

#[derive(Clone, Copy, Debug)]
enum Work { Real, Fixed { per_item_us: u64 }, Pareto { median_us: u64, shape_x10: u64 } }

struct Synthetic {
    fixed_us: u64,          // per-dispatch setup cost
    shape: SyntheticShape,
    work: Work,             // per-item work (ignored for Flat with Real)
}

impl EffectCost for Synthetic {
    fn dispatch(
        &self, batch: &[PathBuf], pat: &Pattern<SupportLang>, rng: &mut u64,
    ) -> (u64, u64, u64) {
        let t0 = Instant::now();
        sleep_us(self.fixed_us);
        let (mut b, mut m) = (0u64, 0u64);
        let n = batch.len() as u64;

        if let SyntheticShape::Flat { flat_us } = self.shape {
            // Group-commit shape. Whole batch completes in a batch-size-
            // independent time. Honest scan still runs under Work::Real so
            // (bytes, matches) stays reproducible.
            if matches!(self.work, Work::Real) {
                for p in batch {
                    let (bb, mm) = scan_one(p, pat, scan_lang());
                    b += bb; m += mm;
                }
            }
            sleep_us(flat_us);
            return (b, m, t0.elapsed().as_micros() as u64);
        }

        match self.work {
            Work::Real => for p in batch {
                let (bb, mm) = scan_one(p, pat, scan_lang());
                b += bb; m += mm;
            },
            Work::Fixed { per_item_us } => for _ in batch { sleep_us(per_item_us); },
            Work::Pareto { median_us, shape_x10 } => for _ in batch {
                sleep_us(pareto_sample_us(rng, median_us, shape_x10));
            },
        }
        if let SyntheticShape::Quadratic { coeff_us } = self.shape {
            // Super-linear coordination / cache-pressure model.
            let overhead = coeff_us.saturating_mul(n).saturating_mul(n) / 1000;
            sleep_us(overhead);
        }
        (b, m, t0.elapsed().as_micros() as u64)
    }
    fn name(&self) -> String {
        let shape = match self.shape {
            SyntheticShape::Linear => "linear".into(),
            SyntheticShape::Flat { flat_us } => format!("flat:{}", flat_us),
            SyntheticShape::Quadratic { coeff_us } => format!("quad:{}", coeff_us),
        };
        let work = match self.work {
            Work::Real => "real".into(),
            Work::Fixed { per_item_us } => format!("fixed/{}", per_item_us),
            Work::Pareto { median_us, shape_x10 } => format!("pareto/{}/{}", median_us, shape_x10),
        };
        format!("synthetic:{},fixed={},work={}", shape, self.fixed_us, work)
    }
}

// --- GitOdbRead: Mutex-serialized fixed cost (mock) ------------------------
//
// Models a git ODB read path where the pack index lookup holds a global lock
// for ~10-100µs per item. Consumer parallelism on top buys nothing because
// the lock serializes every worker through the same critical section. Flat
// amortization curve: batching neither helps nor hurts.

struct GitOdbRead {
    fixed_us: u64,            // per-item lock-held cost
    lock: Arc<Mutex<()>>,
    do_real_scan: bool,       // if true, run ast-grep scan after lock release
}

impl EffectCost for GitOdbRead {
    fn dispatch(
        &self, batch: &[PathBuf], pat: &Pattern<SupportLang>, _rng: &mut u64,
    ) -> (u64, u64, u64) {
        let t0 = Instant::now();
        let (mut b, mut m) = (0u64, 0u64);
        for p in batch {
            {
                let _g = self.lock.lock().unwrap();
                sleep_us(self.fixed_us);
            }
            if self.do_real_scan {
                let (bb, mm) = scan_one(p, pat, scan_lang());
                b += bb; m += mm;
            }
        }
        (b, m, t0.elapsed().as_micros() as u64)
    }
    fn name(&self) -> String { format!("git-odb:fixed_us={}:scan={}", self.fixed_us, self.do_real_scan) }
}

// --- SqliteInsertTx: stepped fixed cost per batch (mock) --------------------
//
// Scaffold uses sleep model: `tx_fixed_us` once per dispatch (BEGIN +
// COMMIT fsync sim) + `per_row_us` per row. Real rusqlite `:memory:` +
// `BEGIN IMMEDIATE` lands in follow-up. The stepped-cost shape is what
// batching-jutsu's windowed-trailing policy specifically targets.

struct SqliteInsertTx {
    tx_fixed_us: u64,
    per_row_us: u64,
    do_real_scan: bool,
}

impl EffectCost for SqliteInsertTx {
    fn dispatch(
        &self, batch: &[PathBuf], pat: &Pattern<SupportLang>, _rng: &mut u64,
    ) -> (u64, u64, u64) {
        let t0 = Instant::now();
        let (mut b, mut m) = (0u64, 0u64);
        if self.do_real_scan {
            for p in batch {
                let (bb, mm) = scan_one(p, pat, scan_lang());
                b += bb; m += mm;
            }
        }
        sleep_us(self.tx_fixed_us);
        for _ in batch { sleep_us(self.per_row_us); }
        (b, m, t0.elapsed().as_micros() as u64)
    }
    fn name(&self) -> String {
        format!("sqlite:tx_fixed_us={}:per_row_us={}:scan={}",
            self.tx_fixed_us, self.per_row_us, self.do_real_scan)
    }
}

// --- RayonAstGrep: par_iter baseline ---------------------------------------
//
// Runs the full batch under rayon par_iter. The point is to demonstrate the
// 1/C throughput ceiling from batching-jutsu: wrapping already-parallel work
// in a batching layer serializes workers through the dispatch call, so more
// batching = less throughput. Worker count comes from rayon's global pool.

struct RayonAstGrep;

impl EffectCost for RayonAstGrep {
    fn dispatch(
        &self, batch: &[PathBuf], pat: &Pattern<SupportLang>, _rng: &mut u64,
    ) -> (u64, u64, u64) {
        let t0 = Instant::now();
        let (b, m) = batch.par_iter()
            .map(|p| scan_one(p, pat, scan_lang()))
            .reduce(|| (0u64, 0u64), |(a, c), (x, y)| (a + x, c + y));
        (b, m, t0.elapsed().as_micros() as u64)
    }
    fn name(&self) -> String { "rayon-ast-grep".into() }
}

// --- GitOdbReadReal: real git2 ODB reads -----------------------------------
//
// Construct-time: open `repo_path`, walk HEAD tree preorder, collect up to
// `oid_cap` blob OIDs. Dispatch: for each item, xorshift-sample an OID and
// call `odb.read(oid)` under the shared Mutex<Repository> lock. Touches
// `obj.data()` so the blob bytes actually flow through. Bytes/matches
// reported come from (a) summed blob sizes and (b) optional ast-grep scan
// run OUTSIDE the lock so the contention picture stays honest.
//
// Realistic model:
// - Mutex<Repository> matches the real write path (git2's Repository is
//   Send but not Sync; sharing requires serialization anyway)
// - ODB pack-index lookup + zlib inflate happens under the lock
// - Scanning the file from disk happens outside the lock
// - No workaround for !Sync — one connection per worker is a variation to
//   enable later via a `shared=false` knob

struct GitOdbReadReal {
    repo: Arc<Mutex<git2::Repository>>,
    oids: Vec<git2::Oid>,
    repo_label: String,
    do_real_scan: bool,
}

impl GitOdbReadReal {
    fn new(repo_path: &str, oid_cap: usize, do_real_scan: bool) -> Self {
        let repo = git2::Repository::open(repo_path)
            .unwrap_or_else(|e| panic!("git-odb-real: open {}: {}", repo_path, e));
        let mut oids = Vec::with_capacity(oid_cap.min(4096));
        {
            let head_tree = repo.head()
                .and_then(|h| h.peel_to_tree())
                .expect("git-odb-real: HEAD tree");
            head_tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
                if entry.kind() == Some(git2::ObjectType::Blob) {
                    oids.push(entry.id());
                    if oids.len() >= oid_cap { return git2::TreeWalkResult::Abort; }
                }
                git2::TreeWalkResult::Ok
            }).ok();
        }
        assert!(!oids.is_empty(), "git-odb-real: HEAD tree has no blobs");
        Self {
            repo: Arc::new(Mutex::new(repo)),
            oids,
            repo_label: repo_path.to_string(),
            do_real_scan,
        }
    }
}

impl EffectCost for GitOdbReadReal {
    fn dispatch(
        &self, batch: &[PathBuf], pat: &Pattern<SupportLang>, rng: &mut u64,
    ) -> (u64, u64, u64) {
        let t0 = Instant::now();
        let n_oids = self.oids.len();
        let (mut b, mut m) = (0u64, 0u64);
        // Accumulate inflated-blob bytes into `odb_bytes` with a byte-wise
        // folding hash so the compiler cannot elide the inflate call. The
        // fold forces each byte to be read after zlib decompresses the pack
        // object. Without this, an aggressive optimizer could notice that
        // `obj.data().len()` is unused and drop the FFI call. `git2::odb::read`
        // is FFI-opaque to LLVM today, so elision is unlikely in practice,
        // but folding makes it a hard guarantee regardless of inlining.
        let mut odb_bytes: u64 = 0;
        let mut odb_hash: u64 = 0;
        {
            let repo = self.repo.lock().unwrap();
            let odb = repo.odb().expect("git-odb-real: odb()");
            for _ in batch {
                let idx = (xorshift64(rng) as usize) % n_oids;
                let obj = odb.read(self.oids[idx]).expect("git-odb-real: odb.read");
                let data = obj.data();
                odb_bytes = odb_bytes.wrapping_add(data.len() as u64);
                // Fold every 64th byte — cheap but load-bearing. Prevents
                // any future optimizer from treating this as dead code.
                for byte in data.iter().step_by(64) {
                    odb_hash = odb_hash.wrapping_mul(1099511628211)
                        ^ (*byte as u64);
                }
            }
        }
        if self.do_real_scan {
            for p in batch {
                let (bb, mm) = scan_one(p, pat, scan_lang());
                b += bb; m += mm;
            }
        }
        // Fold hash into matches so it can't be dropped; fold inflated bytes
        // into bytes so the counter reflects real work.
        m = m.wrapping_add(odb_hash & 1); // 0 or 1 — negligible, unelidable
        b = b.wrapping_add(odb_bytes);
        (b, m, t0.elapsed().as_micros() as u64)
    }
    fn name(&self) -> String {
        format!("git-odb-real:repo={}:oids={}:scan={}",
            self.repo_label, self.oids.len(), self.do_real_scan)
    }
}

// --- SqliteInsertTxReal: real rusqlite :memory: INSERT transactions --------
//
// Single shared Mutex<Connection>. Per-dispatch:
//   lock → BEGIN → INSERT-each → COMMIT → unlock.
// Scan (if scan=true) runs BEFORE the lock so scan time doesn't contend
// with insert time; matches real sprefa shape where scan is pipeline work
// and sqlite is the tail effect.
//
// Journal/sync knobs are meaningful even on :memory: for CPU path
// differences. Default journal=memory, sync=off — the fastest write path
// and the best scaffold baseline. Flip to journal=wal,sync=normal to see
// realistic on-disk fsync cost (requires path=file).

struct SqliteInsertTxReal {
    conn: Arc<Mutex<rusqlite::Connection>>,
    journal: String,
    sync: String,
    do_real_scan: bool,
}

impl SqliteInsertTxReal {
    fn new(journal: &str, sync: &str, do_real_scan: bool) -> Self {
        let conn = rusqlite::Connection::open_in_memory()
            .expect("sqlite-real: open_in_memory");
        conn.execute_batch(&format!(
            "PRAGMA journal_mode={};\nPRAGMA synchronous={};\n\
             CREATE TABLE hits (\n\
                 id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
                 path TEXT NOT NULL,\n\
                 bytes INTEGER NOT NULL,\n\
                 matches INTEGER NOT NULL\n\
             );",
            journal, sync
        )).expect("sqlite-real: schema");
        Self {
            conn: Arc::new(Mutex::new(conn)),
            journal: journal.to_string(),
            sync: sync.to_string(),
            do_real_scan,
        }
    }
}

impl EffectCost for SqliteInsertTxReal {
    fn dispatch(
        &self, batch: &[PathBuf], pat: &Pattern<SupportLang>, _rng: &mut u64,
    ) -> (u64, u64, u64) {
        let t0 = Instant::now();
        // Scan outside the lock: accumulates (path, bytes, matches) rows.
        let rows: Vec<(String, i64, i64)> = batch.iter().map(|p| {
            let (bb, mm) = if self.do_real_scan {
                scan_one(p, pat, scan_lang())
            } else { (0, 0) };
            (p.to_string_lossy().into_owned(), bb as i64, mm as i64)
        }).collect();
        let (mut b, mut m) = (0u64, 0u64);
        for r in &rows { b += r.1 as u64; m += r.2 as u64; }
        // BEGIN/INSERT/COMMIT under the shared lock.
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().expect("sqlite-real: BEGIN");
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO hits (path, bytes, matches) VALUES (?1, ?2, ?3)"
            ).expect("sqlite-real: prepare");
            for r in &rows {
                stmt.execute(rusqlite::params![r.0, r.1, r.2])
                    .expect("sqlite-real: insert");
            }
        }
        tx.commit().expect("sqlite-real: COMMIT");
        (b, m, t0.elapsed().as_micros() as u64)
    }
    fn name(&self) -> String {
        format!("sqlite-real:journal={}:sync={}:scan={}",
            self.journal, self.sync, self.do_real_scan)
    }
}

// ============================================================================
// AXIS 2 — ArrivalGen
// ============================================================================
//
// Producer calls `next_gap_us(i)` before every send. Distinct impls expose
// distinct failure modes downstream:
//   Dirac        saturates the channel — exposes backpressure shape.
//   Dist(Fixed)  steady-state — baseline for amortization math.
//   Dist(Pareto) tail-heavy — exposes queueing wait blowup at ρ > 0.8.
//   Burst        edge-event model — LSP reparse arrival.
//   Sine         oscillatory — exposes buffer_count-stalls-in-trough
//                (the failure mode batching-jutsu's fixed-count policy names).
//   AR1          autocorrelated — realistic sustained arrival w/ memory.
//   StepFn       piecewise-constant — rate changes at known indices.

trait ArrivalGen: Send {
    fn next_gap_us(&mut self, idx: u64) -> u64;
    fn name(&self) -> String;
}

#[derive(Clone, Copy, Debug)]
enum Dist {
    Fixed { us: u64 },
    Pareto { median_us: u64, shape_x10: u64 },
    Normal { mean_us: u64, sd_us: u64 },
    Exp { mean_us: u64 },
}

struct DistArrival { dist: Dist, rng: u64 }
impl ArrivalGen for DistArrival {
    fn next_gap_us(&mut self, _idx: u64) -> u64 {
        match self.dist {
            Dist::Fixed { us } => us,
            Dist::Pareto { median_us, shape_x10 } =>
                pareto_sample_us(&mut self.rng, median_us, shape_x10),
            Dist::Normal { mean_us, sd_us } => {
                let u1 = sample_unit(&mut self.rng).max(1e-12);
                let u2 = sample_unit(&mut self.rng);
                let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                (mean_us as f64 + z * sd_us as f64).max(0.0).min(1e9) as u64
            }
            Dist::Exp { mean_us } => {
                let u = sample_unit(&mut self.rng).max(1e-12);
                (mean_us as f64 * -u.ln()).min(1e9) as u64
            }
        }
    }
    fn name(&self) -> String {
        match self.dist {
            Dist::Fixed { us } => format!("fixed:{}", us),
            Dist::Pareto { median_us, shape_x10 } => format!("pareto:{}:{}", median_us, shape_x10),
            Dist::Normal { mean_us, sd_us } => format!("normal:{}:{}", mean_us, sd_us),
            Dist::Exp { mean_us } => format!("exp:{}", mean_us),
        }
    }
}

struct BurstArrival { count: u64, gap_us: u64 }
impl ArrivalGen for BurstArrival {
    fn next_gap_us(&mut self, idx: u64) -> u64 {
        if self.count > 0 && idx > 0 && idx % self.count == 0 { self.gap_us } else { 0 }
    }
    fn name(&self) -> String { format!("burst:{}:{}", self.count, self.gap_us) }
}

struct DiracArrival;
impl ArrivalGen for DiracArrival {
    fn next_gap_us(&mut self, _idx: u64) -> u64 { 0 }
    fn name(&self) -> String { "dirac".into() }
}

// Sine: gap = base + amp * (1 + sin(2π·idx/period)) / 2. Clamped >= 0.
// Exposes buffer_count-held-indefinitely in trough where arrival rate drops
// below drain rate — the policy red flag in batching-jutsu.
struct SineArrival { period: u64, amp_us: u64, base_us: u64 }
impl ArrivalGen for SineArrival {
    fn next_gap_us(&mut self, idx: u64) -> u64 {
        let t = (idx % self.period.max(1)) as f64 / self.period.max(1) as f64;
        let s = (2.0 * std::f64::consts::PI * t).sin();
        let gap = self.base_us as f64 + (self.amp_us as f64) * (1.0 + s) / 2.0;
        gap.max(0.0) as u64
    }
    fn name(&self) -> String { format!("sine:period={}:amp={}:base={}", self.period, self.amp_us, self.base_us) }
}

// AR(1): gap_i = base + rho*(gap_{i-1} - base) + N(0, sd). rho_x100 in [0,100].
// Autocorrelated arrival — realistic LSP/editor arrival where bursts cluster.
struct AR1Arrival {
    base_us: f64,
    rho: f64,
    sd_us: f64,
    state_us: f64,
    rng: u64,
}
impl ArrivalGen for AR1Arrival {
    fn next_gap_us(&mut self, _idx: u64) -> u64 {
        let u1 = sample_unit(&mut self.rng).max(1e-12);
        let u2 = sample_unit(&mut self.rng);
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        let next = self.base_us + self.rho * (self.state_us - self.base_us) + self.sd_us * z;
        self.state_us = next;
        next.max(0.0).min(1e9) as u64
    }
    fn name(&self) -> String {
        format!("ar1:base={}:rho={:.2}:sd={}", self.base_us as u64, self.rho, self.sd_us as u64)
    }
}

// StepFn: segments of (until_idx, gap_us). First matching segment wins.
// Falls back to last segment's gap once idx passes every threshold.
struct StepFnArrival { segments: Vec<(u64, u64)> }
impl ArrivalGen for StepFnArrival {
    fn next_gap_us(&mut self, idx: u64) -> u64 {
        for &(until, gap) in &self.segments {
            if idx < until { return gap; }
        }
        self.segments.last().map(|&(_, g)| g).unwrap_or(0)
    }
    fn name(&self) -> String {
        let parts: Vec<String> = self.segments.iter()
            .map(|(u, g)| format!("{}:{}", u, g)).collect();
        format!("stepfn:{}", parts.join(","))
    }
}

// ============================================================================
// AXIS 3 — BatchPolicy
// ============================================================================
//
// Policy sits between the bounded channel and effect dispatch. Under paced
// arrival (Sine, AR1, fixed gap) opportunistic drain collapses to 1-item
// batches because consumers outrun the producer — exactly the failure
// mode batching-jutsu's fixed-count red flag names. WindowedTrailing fixes
// this by giving a pending batch up to `window_us` to fill before flushing.

trait BatchPolicy: Send + Sync {
    // Fills `buf` with up to `max_batch` items. Returns false when the
    // channel is closed and no more items will arrive (shutdown signal).
    fn next_batch(
        &self,
        rx: &crossbeam_channel::Receiver<PathBuf>,
        max_batch: usize,
        buf: &mut Vec<PathBuf>,
    ) -> bool;
    fn name(&self) -> String;
}

// Opportunistic: `recv` (block) then `try_recv` until max_batch or empty.
// Batches are as large as the channel happens to have queued up. Default.
struct OpportunisticPolicy;
impl BatchPolicy for OpportunisticPolicy {
    fn next_batch(
        &self,
        rx: &crossbeam_channel::Receiver<PathBuf>,
        max_batch: usize,
        buf: &mut Vec<PathBuf>,
    ) -> bool {
        buf.clear();
        let Ok(first) = rx.recv() else { return false };
        buf.push(first);
        while buf.len() < max_batch {
            match rx.try_recv() {
                Ok(p) => buf.push(p),
                Err(_) => break,
            }
        }
        true
    }
    fn name(&self) -> String { "opportunistic".into() }
}

// WindowedTrailing: after first recv, wait up to `window_us` for more
// items (blocking with timeout). Flush on max_batch reached or deadline.
// Trades latency for batch amortization — the throttle-time-buffer shape.
struct WindowedTrailingPolicy { window_us: u64 }
impl BatchPolicy for WindowedTrailingPolicy {
    fn next_batch(
        &self,
        rx: &crossbeam_channel::Receiver<PathBuf>,
        max_batch: usize,
        buf: &mut Vec<PathBuf>,
    ) -> bool {
        buf.clear();
        let Ok(first) = rx.recv() else { return false };
        buf.push(first);
        let deadline = Instant::now() + Duration::from_micros(self.window_us);
        while buf.len() < max_batch {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { break; }
            match rx.recv_timeout(remaining) {
                Ok(p) => buf.push(p),
                Err(_) => break, // timeout or disconnect; flush what we have
            }
        }
        true
    }
    fn name(&self) -> String { format!("windowed-trailing:{}us", self.window_us) }
}

fn parse_policy(spec: &str) -> Arc<dyn BatchPolicy> {
    if spec == "opportunistic" { return Arc::new(OpportunisticPolicy); }
    if let Some(rest) = spec.strip_prefix("windowed:") {
        let us: u64 = rest.parse().expect("windowed:US");
        return Arc::new(WindowedTrailingPolicy { window_us: us });
    }
    panic!("unknown --policy: {}", spec);
}

// ============================================================================
// Topology (single impl in the scaffold): Hopp-shape with pluggable axes.
// ============================================================================

const TAG_HOPP_V2: u64 = 0x8097_2222_8097_2222;

fn run_hopp(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    cap: usize,
    max_batch: usize,
    workers: usize,
    effect: Arc<dyn EffectCost>,
    mut arrival: Box<dyn ArrivalGen>,
    policy: Arc<dyn BatchPolicy>,
    seed_base: u64,
) -> (u64, u64, u64, Vec<u32>, Vec<u64>) {
    let (tx, rx) = crossbeam_channel::bounded::<PathBuf>(cap);
    let mut handles = Vec::with_capacity(workers);
    for w_idx in 0..workers {
        let rx = rx.clone();
        let pat = pat.clone();
        let effect = effect.clone();
        let policy = policy.clone();
        let worker_tag = TAG_HOPP_V2
            ^ ((w_idx as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut rng = mix_seed(seed_base, worker_tag);
        handles.push(std::thread::spawn(move || {
            let mut batch_sizes: Vec<u32> = Vec::with_capacity(512);
            let mut dispatch_us_vec: Vec<u64> = Vec::with_capacity(512);
            let (mut f, mut b, mut m) = (0u64, 0u64, 0u64);
            let mut batch: Vec<PathBuf> = Vec::with_capacity(max_batch);
            while policy.next_batch(&rx, max_batch, &mut batch) {
                let depth = batch.len() as u32;
                let (bb, mm, du) = effect.dispatch(&batch, &pat, &mut rng);
                f += depth as u64; b += bb; m += mm;
                batch_sizes.push(depth);
                dispatch_us_vec.push(du);
            }
            (f, b, m, batch_sizes, dispatch_us_vec)
        }));
    }
    drop(rx);
    for (i, p) in paths.iter().enumerate() {
        sleep_us(arrival.next_gap_us(i as u64));
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
    let _ = seed_base;
    (f, b, m, batch_sizes, dispatch_us_vec)
}

// ============================================================================
// walk-parallel topology: rayon work-stealing over pre-enumerated paths.
// Matches ast-grep CLI's shape (ignore::WalkParallel work-stealing deque).
// No channel, no batching, no arrival pacing, no EffectCost dispatch — just
// parallel scan_one across paths. Isolates the topology lever from the
// effect/arrival/policy axes so we can measure crossbeam-MPMC vs work-stealing.
// ============================================================================

fn run_walk_parallel(
    paths: &[PathBuf],
    pat: Arc<Pattern<SupportLang>>,
    workers: usize,
) -> (u64, u64, u64) {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .expect("rayon pool");
    let lang = scan_lang();
    pool.install(|| {
        paths
            .par_iter()
            .map(|p| {
                let (b, m) = scan_one(p, &pat, lang);
                (1u64, b, m)
            })
            .reduce(|| (0, 0, 0), |(f1, b1, m1), (f2, b2, m2)| (f1 + f2, b1 + b2, m1 + m2))
    })
}

// ============================================================================
// Spec parsing. Grammar mirrors v1: dead-simple split-on-colon-and-comma.
// ============================================================================

// Parse `k=v,k=v,...` into a Vec. Order-preserving (no HashMap).
fn parse_kvs(s: &str) -> Vec<(String, String)> {
    if s.is_empty() { return Vec::new(); }
    s.split(',')
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or_else(|| panic!("expected k=v, got {:?}", p));
            (k.to_string(), v.to_string())
        })
        .collect()
}

fn kv_u64(kvs: &[(String, String)], key: &str, default: u64) -> u64 {
    kvs.iter().find(|(k, _)| k == key).map(|(_, v)| v.parse().unwrap()).unwrap_or(default)
}

fn kv_bool(kvs: &[(String, String)], key: &str, default: bool) -> bool {
    kvs.iter().find(|(k, _)| k == key)
        .map(|(_, v)| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(default)
}

fn parse_effect(spec: &str) -> Arc<dyn EffectCost> {
    // synthetic:<shape>[,k=v,...]
    //   shape ∈ { linear | flat:US | quad:COEFF }
    //   kvs:  fixed=US | work=real|fixed/US|pareto/MED/SHAPEX10
    //   `/` separates components inside a kv value so `:` stays reserved
    //   for shape-argument separation.
    if let Some(rest) = spec.strip_prefix("synthetic:") {
        let (shape_part, kv_blob) = match rest.split_once(',') {
            Some((s, k)) => (s, k),
            None => (rest, ""),
        };
        let shape = if shape_part == "linear" {
            SyntheticShape::Linear
        } else if let Some(us_s) = shape_part.strip_prefix("flat:") {
            SyntheticShape::Flat { flat_us: us_s.parse().unwrap() }
        } else if let Some(c_s) = shape_part.strip_prefix("quad:") {
            SyntheticShape::Quadratic { coeff_us: c_s.parse().unwrap() }
        } else {
            panic!("unknown synthetic shape: {}", shape_part);
        };
        let kvs = parse_kvs(kv_blob);
        let fixed_us = kv_u64(&kvs, "fixed", 0);
        let work = match kvs.iter().find(|(k, _)| k == "work").map(|(_, v)| v.as_str()) {
            None | Some("real") => Work::Real,
            Some(s) if s.starts_with("fixed/") =>
                Work::Fixed { per_item_us: s.strip_prefix("fixed/").unwrap().parse().unwrap() },
            Some(s) if s.starts_with("pareto/") => {
                let rest = s.strip_prefix("pareto/").unwrap();
                let (med, sh) = rest.split_once('/').expect("pareto/MED/SHAPEX10");
                Work::Pareto { median_us: med.parse().unwrap(), shape_x10: sh.parse().unwrap() }
            }
            Some(other) => panic!("unknown work: {}", other),
        };
        return Arc::new(Synthetic { fixed_us, shape, work });
    }
    if let Some(rest) = spec.strip_prefix("git-odb:") {
        let kvs = parse_kvs(rest);
        return Arc::new(GitOdbRead {
            fixed_us: kv_u64(&kvs, "fixed_us", 50),
            lock: Arc::new(Mutex::new(())),
            do_real_scan: kv_bool(&kvs, "scan", true),
        });
    }
    if let Some(rest) = spec.strip_prefix("sqlite:") {
        let kvs = parse_kvs(rest);
        return Arc::new(SqliteInsertTx {
            tx_fixed_us: kv_u64(&kvs, "tx_fixed_us", 1000),
            per_row_us:  kv_u64(&kvs, "per_row_us", 10),
            do_real_scan: kv_bool(&kvs, "scan", true),
        });
    }
    if spec == "rayon-ast-grep" { return Arc::new(RayonAstGrep); }
    if let Some(rest) = spec.strip_prefix("git-odb-real:") {
        let kvs = parse_kvs(rest);
        let repo = kvs.iter().find(|(k, _)| k == "repo")
            .map(|(_, v)| v.clone())
            .expect("git-odb-real requires repo=PATH");
        let oid_cap = kv_u64(&kvs, "oid_cap", 8192) as usize;
        let scan = kv_bool(&kvs, "scan", true);
        return Arc::new(GitOdbReadReal::new(&repo, oid_cap, scan));
    }
    if spec == "sqlite-real" || spec.starts_with("sqlite-real:") {
        let kv_blob = spec.strip_prefix("sqlite-real:").unwrap_or("");
        let kvs = parse_kvs(kv_blob);
        let journal = kvs.iter().find(|(k, _)| k == "journal")
            .map(|(_, v)| v.clone()).unwrap_or_else(|| "MEMORY".into());
        let sync = kvs.iter().find(|(k, _)| k == "sync")
            .map(|(_, v)| v.clone()).unwrap_or_else(|| "OFF".into());
        let scan = kv_bool(&kvs, "scan", true);
        return Arc::new(SqliteInsertTxReal::new(&journal, &sync, scan));
    }
    panic!("unknown --effect: {}", spec);
}

fn parse_arrival(spec: &str, seed: u64) -> Box<dyn ArrivalGen> {
    if spec == "dirac" { return Box::new(DiracArrival); }
    if let Some(rest) = spec.strip_prefix("fixed:") {
        return Box::new(DistArrival { dist: Dist::Fixed { us: rest.parse().unwrap() }, rng: mix_seed(seed, 0xA1) });
    }
    if let Some(rest) = spec.strip_prefix("pareto:") {
        let (m, s) = rest.split_once(':').expect("pareto:MED:SHAPEX10");
        return Box::new(DistArrival {
            dist: Dist::Pareto { median_us: m.parse().unwrap(), shape_x10: s.parse().unwrap() },
            rng: mix_seed(seed, 0xA2),
        });
    }
    if let Some(rest) = spec.strip_prefix("normal:") {
        let (m, s) = rest.split_once(':').expect("normal:MEAN:SD");
        return Box::new(DistArrival {
            dist: Dist::Normal { mean_us: m.parse().unwrap(), sd_us: s.parse().unwrap() },
            rng: mix_seed(seed, 0xA3),
        });
    }
    if let Some(rest) = spec.strip_prefix("exp:") {
        return Box::new(DistArrival {
            dist: Dist::Exp { mean_us: rest.parse().unwrap() },
            rng: mix_seed(seed, 0xA4),
        });
    }
    if let Some(rest) = spec.strip_prefix("burst:") {
        let (c, g) = rest.split_once(':').expect("burst:COUNT:GAP_US");
        return Box::new(BurstArrival { count: c.parse().unwrap(), gap_us: g.parse().unwrap() });
    }
    if let Some(rest) = spec.strip_prefix("sine:") {
        let kvs = parse_kvs(rest);
        return Box::new(SineArrival {
            period: kv_u64(&kvs, "period", 1000),
            amp_us: kv_u64(&kvs, "amp", 500),
            base_us: kv_u64(&kvs, "base", 0),
        });
    }
    if let Some(rest) = spec.strip_prefix("ar1:") {
        let kvs = parse_kvs(rest);
        let base = kv_u64(&kvs, "base", 100) as f64;
        let rho = (kv_u64(&kvs, "rho", 80) as f64) / 100.0;
        let sd = kv_u64(&kvs, "sd", 50) as f64;
        return Box::new(AR1Arrival {
            base_us: base, rho, sd_us: sd, state_us: base, rng: mix_seed(seed, 0xA5),
        });
    }
    if let Some(rest) = spec.strip_prefix("stepfn:") {
        let segs: Vec<(u64, u64)> = rest.split(',')
            .map(|p| {
                let (u, g) = p.split_once(':').expect("stepfn:UNTIL:GAP,...");
                (u.parse().unwrap(), g.parse().unwrap())
            })
            .collect();
        return Box::new(StepFnArrival { segments: segs });
    }
    panic!("unknown --arrival: {}", spec);
}

// ============================================================================
// main — single topology, pluggable axes, same TSV/JSON output shape as v1.
// ============================================================================

fn main() {
    let mut pattern_tag: Option<String> = None;
    let mut root = PathBuf::from("tests/smoke/.fixtures/swc");
    let mut max_files: Option<usize> = None;
    let mut cap: usize = 2048;
    let mut max_batch: usize = 64;
    let mut workers: usize = 1;
    let mut effect_spec: String = "synthetic:linear".into();
    let mut arrival_spec: String = "dirac".into();
    let mut policy_spec: String = "opportunistic".into();
    let mut ext_spec: String = "js-ts".into();
    let mut lang_spec: String = "tsx".into();
    let mut seed: u64 = 0xDEAD_BEEF_CAFE_BABE;
    let mut trials: usize = 1;
    let mut tsv: bool = false;
    let mut rayon_threads: Option<usize> = None;
    let mut breakdown: bool = false;
    let mut max_bytes: Option<u64> = None;
    let mut topology: String = "hopp-v2".into();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--pattern"       => { pattern_tag = Some(args[i + 1].clone()); i += 2; }
            "--root"          => { root = PathBuf::from(&args[i + 1]); i += 2; }
            "--max"           => { max_files = args[i + 1].parse().ok(); i += 2; }
            "--cap"           => { cap = args[i + 1].parse().unwrap(); i += 2; }
            "--max-batch"     => { max_batch = args[i + 1].parse().unwrap(); i += 2; }
            "--workers"       => { workers = args[i + 1].parse().unwrap(); i += 2; }
            "--effect"        => { effect_spec = args[i + 1].clone(); i += 2; }
            "--arrival"       => { arrival_spec = args[i + 1].clone(); i += 2; }
            "--policy"        => { policy_spec = args[i + 1].clone(); i += 2; }
            "--ext"           => { ext_spec = args[i + 1].clone(); i += 2; }
            "--lang"          => { lang_spec = args[i + 1].clone(); i += 2; }
            "--seed"          => { seed = args[i + 1].parse().unwrap(); i += 2; }
            "--trials"        => { trials = args[i + 1].parse::<usize>().unwrap().max(1); i += 2; }
            "--tsv"           => { tsv = true; i += 1; }
            "--rayon-threads" => { rayon_threads = args[i + 1].parse().ok(); i += 2; }
            "--breakdown"     => { breakdown = true; i += 1; }
            "--max-bytes"     => { max_bytes = args[i + 1].parse().ok(); i += 2; }
            "--topology"      => { topology = args[i + 1].clone(); i += 2; }
            "--no-prefilter"  => { PREFILTER_ENABLED.store(false, std::sync::atomic::Ordering::Relaxed); i += 1; }
            other => panic!("unknown arg: {}", other),
        }
    }
    let pattern_tag = pattern_tag.expect("--pattern required");
    if let Some(n) = rayon_threads {
        rayon::ThreadPoolBuilder::new().num_threads(n).build_global().ok();
    }

    let lang = parse_lang(&lang_spec);
    SCAN_LANG.set(lang).expect("SCAN_LANG set once");
    let pat = Arc::new(Pattern::try_new(pattern_src(&pattern_tag), lang).expect("pattern compile"));
    let ext = parse_ext_filter(&ext_spec);
    let t_enum = Instant::now();
    let paths = enumerate_with_size(&root, max_files, &ext, max_bytes);
    let enumerate_ms = t_enum.elapsed().as_secs_f64() * 1000.0;
    assert!(!paths.is_empty(), "no files enumerated (root={:?} ext={})", root, ext.label());
    let rss_after_enum_kb = rss_peak_kb();

    // Page cache + ast-grep JIT warm, same as v1.
    let t_pc = Instant::now();
    for p in &paths { if let Ok(b) = std::fs::read(p) { let _ = b.len(); } }
    if let Some(p) = paths.first() { let _ = scan_one(p, &pat, lang); }
    let page_cache_ms = t_pc.elapsed().as_secs_f64() * 1000.0;
    let rss_after_warm_kb = rss_peak_kb();

    if breakdown {
        // Serial phase-breakdown: call scan_one_timed on every enumerated file.
        // Prints per-phase percentiles (read / parse / match) to stdout so we
        // can see where the scan_one cost actually lives. Bypasses pipeline.
        let mut read_v: Vec<u64> = Vec::with_capacity(paths.len());
        let mut parse_v: Vec<u64> = Vec::with_capacity(paths.len());
        let mut match_v: Vec<u64> = Vec::with_capacity(paths.len());
        let mut ts_v: Vec<u64> = Vec::with_capacity(paths.len());
        let mut total_bytes: u64 = 0;
        let mut total_matches: u64 = 0;
        let t0 = Instant::now();
        for p in &paths {
            let (bb, mm, rn, pn, mn, tn) = scan_one_timed(p, &pat, lang);
            total_bytes += bb;
            total_matches += mm;
            read_v.push(rn);
            parse_v.push(pn);
            match_v.push(mn);
            ts_v.push(tn);
        }
        let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let pct_ns = |v: &Vec<u64>, q: f64| pct_u64(v, q) as f64 / 1000.0;
        let sum_us = |v: &Vec<u64>| v.iter().sum::<u64>() as f64 / 1000.0;
        println!("# breakdown files={} wall_ms={:.1} bytes={} matches={}",
            paths.len(), wall_ms, total_bytes, total_matches);
        println!("# read  p50={:.1}us p95={:.1}us p99={:.1}us total={:.1}ms",
            pct_ns(&read_v, 0.50), pct_ns(&read_v, 0.95), pct_ns(&read_v, 0.99),
            sum_us(&read_v) / 1000.0);
        println!("# parse p50={:.1}us p95={:.1}us p99={:.1}us total={:.1}ms",
            pct_ns(&parse_v, 0.50), pct_ns(&parse_v, 0.95), pct_ns(&parse_v, 0.99),
            sum_us(&parse_v) / 1000.0);
        println!("# match p50={:.1}us p95={:.1}us p99={:.1}us total={:.1}ms",
            pct_ns(&match_v, 0.50), pct_ns(&match_v, 0.95), pct_ns(&match_v, 0.99),
            sum_us(&match_v) / 1000.0);
        println!("# ts-only (reused Parser, parse only, no ast-grep Pattern):");
        println!("# parse-ts-reuse p50={:.1}us p95={:.1}us p99={:.1}us total={:.1}ms",
            pct_ns(&ts_v, 0.50), pct_ns(&ts_v, 0.95), pct_ns(&ts_v, 0.99),
            sum_us(&ts_v) / 1000.0);
        let alloc_share = (sum_us(&parse_v) - sum_us(&ts_v)) / sum_us(&parse_v) * 100.0;
        println!("# ast-grep-parse vs ts-reused-parse: {:.1}% saved by Parser reuse",
            alloc_share);
        let read_pct = sum_us(&read_v) / (wall_ms * 1000.0) * 100.0;
        let parse_pct = sum_us(&parse_v) / (wall_ms * 1000.0) * 100.0;
        let match_pct = sum_us(&match_v) / (wall_ms * 1000.0) * 100.0;
        println!("# share  read={:.1}% parse={:.1}% match={:.1}% (remainder = loop/alloc)",
            read_pct, parse_pct, match_pct);
        let mb_s = (total_bytes as f64 / 1_048_576.0) / (wall_ms / 1000.0);
        let fps = (paths.len() as f64) / (wall_ms / 1000.0);
        println!("# throughput {:.0} files/s  {:.1} MB/s  (serial, 1 thread)", fps, mb_s);
        return;
    }

    // walk-parallel topology short-circuit: no effect/arrival/policy axes.
    // Matches ast-grep CLI shape for apples-to-apples comparison.
    if topology == "walk-parallel" {
        // Warmup pass.
        let warm_n = paths.len().min(8192);
        let t_warm = Instant::now();
        let _ = run_walk_parallel(&paths[..warm_n], pat.clone(), workers);
        let warm_ms = t_warm.elapsed().as_secs_f64() * 1000.0;
        let rss_after_probe_warm_kb = rss_peak_kb();
        eprintln!(
            "# harness: files={} enumerate_ms={:.1} page_cache_ms={:.1} warm_n={} warm_ms={:.1}",
            paths.len(), enumerate_ms, page_cache_ms, warm_n, warm_ms,
        );
        eprintln!(
            "# rss_kb: after_enum={} after_page_cache={} after_probe_warm={}",
            rss_after_enum_kb, rss_after_warm_kb, rss_after_probe_warm_kb,
        );
        eprintln!(
            "# config: topology=walk-parallel workers={} (no-effect no-arrival no-policy no-batch)",
            workers,
        );
        let mut trial_walls: Vec<f64> = Vec::with_capacity(trials);
        let mut trial_fps: Vec<f64> = Vec::with_capacity(trials);
        let mut trial_mbs: Vec<f64> = Vec::with_capacity(trials);
        let mut trial_rss: Vec<u64> = Vec::with_capacity(trials);
        let (mut files_out, mut bytes_out, mut matches_out) = (0u64, 0u64, 0u64);
        for t in 0..trials {
            let t0 = Instant::now();
            let (files, bytes, matches) = run_walk_parallel(&paths, pat.clone(), workers);
            let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let rss_kb = rss_peak_kb();
            let fps = (files as f64) / (wall_ms / 1000.0);
            let mbs = (bytes as f64 / 1_048_576.0) / (wall_ms / 1000.0);
            eprintln!(
                "# trial {}/{}: wall_ms={:.1} files/s={:.0} MB/s={:.1} rss_kb={} bytes={} matches={}",
                t + 1, trials, wall_ms, fps, mbs, rss_kb, bytes, matches,
            );
            trial_walls.push(wall_ms);
            trial_fps.push(fps);
            trial_mbs.push(mbs);
            trial_rss.push(rss_kb);
            files_out = files; bytes_out = bytes; matches_out = matches;
        }
        let wp10 = pct_f64(&trial_walls, 0.10);
        let wp50 = pct_f64(&trial_walls, 0.50);
        let wp90 = pct_f64(&trial_walls, 0.90);
        let wmin = trial_walls.iter().copied().fold(f64::MAX, f64::min);
        let wmax = trial_walls.iter().copied().fold(f64::MIN, f64::max);
        let fps_p50 = pct_f64(&trial_fps, 0.50);
        let mbs_p50 = pct_f64(&trial_mbs, 0.50);
        let rss_peak = trial_rss.iter().copied().max().unwrap_or(0);
        let variant = format!("WalkParallel:{}", workers);
        if tsv {
            println!(
                "{}\t{}\tnone\tnone\tnone\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t1\t1\t0\t0\t0\t{}\t{:.1}\t{:.2}\t{:.1}",
                variant, pattern_tag, seed, trials,
                files_out, bytes_out, matches_out,
                wp10, wp50, wp90, wmin, wmax,
                rss_peak, fps_p50, mbs_p50, enumerate_ms,
            );
        } else {
            println!(
                "{{\"variant\":\"{}\",\"pattern\":\"{}\",\"topology\":\"walk-parallel\",\"files\":{},\"bytes\":{},\"matches\":{},\"trials\":{},\"wall_p10\":{:.3},\"wall_p50\":{:.3},\"wall_p90\":{:.3},\"wall_min\":{:.3},\"wall_max\":{:.3},\"rss_peak_kb\":{},\"files_per_sec_p50\":{:.1},\"mb_per_sec_p50\":{:.2},\"enumerate_ms\":{:.1}}}",
                variant, pattern_tag, files_out, bytes_out, matches_out, trials,
                wp10, wp50, wp90, wmin, wmax, rss_peak, fps_p50, mbs_p50, enumerate_ms,
            );
        }
        return;
    }

    let effect = parse_effect(&effect_spec);
    let effect_name = effect.name();
    let policy = parse_policy(&policy_spec);
    let policy_name = policy.name();

    // Warmup pass: warm crossbeam channels, worker threads, OS page cache on
    // pack files (for git-odb-real) and worktree files (for scan=true). Scale
    // with input so large runs (linux kernel, 80k files) don't start trial 0
    // with a cold ODB pack cache. Cap at 8192 to bound warmup wall time.
    let warm_n = paths.len().min(8192);
    let t_warm = Instant::now();
    let _ = run_hopp(
        &paths[..warm_n], pat.clone(), cap, max_batch, workers,
        effect.clone(), parse_arrival(&arrival_spec, seed ^ 0xFFFF),
        policy.clone(), seed,
    );
    let warm_ms = t_warm.elapsed().as_secs_f64() * 1000.0;
    let rss_after_probe_warm_kb = rss_peak_kb();
    eprintln!(
        "# harness: files={} enumerate_ms={:.1} page_cache_ms={:.1} warm_n={} warm_ms={:.1}",
        paths.len(), enumerate_ms, page_cache_ms, warm_n, warm_ms,
    );
    eprintln!(
        "# rss_kb: after_enum={} after_page_cache={} after_probe_warm={}",
        rss_after_enum_kb, rss_after_warm_kb, rss_after_probe_warm_kb,
    );
    eprintln!(
        "# config: effect={} arrival={} policy={} workers={} max_batch={} cap={}",
        effect_name, arrival_spec, policy_name, workers, max_batch, cap,
    );

    #[derive(Clone)]
    struct Trial { wall_ms: f64, files: u64, bytes: u64, matches: u64,
        b_p50: u32, b_p95: u32, d_p50: u64, d_p95: u64, d_p99: u64,
        rss_kb: u64, files_per_sec: f64, mb_per_sec: f64 }
    let mut results: Vec<Trial> = Vec::with_capacity(trials);
    let mut arrival_name_last = String::new();
    for t in 0..trials {
        let trial_seed = seed.wrapping_add(t as u64);
        let arrival = parse_arrival(&arrival_spec, trial_seed);
        arrival_name_last = arrival.name();
        let t0 = Instant::now();
        let (files, bytes, matches, batch_sizes, dispatch_us_vec) = run_hopp(
            &paths, pat.clone(), cap, max_batch, workers,
            effect.clone(), arrival, policy.clone(), trial_seed,
        );
        let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let rss_kb = rss_peak_kb();
        let files_per_sec = if wall_ms > 0.0 { (files as f64) / (wall_ms / 1000.0) } else { 0.0 };
        let mb_per_sec = if wall_ms > 0.0 { (bytes as f64 / 1_048_576.0) / (wall_ms / 1000.0) } else { 0.0 };
        eprintln!(
            "# trial {}/{}: wall_ms={:.1} files/s={:.0} MB/s={:.1} rss_kb={} b_p50={} d_p50_us={} bytes={} matches={}",
            t + 1, trials, wall_ms, files_per_sec, mb_per_sec, rss_kb,
            pct_u32(&batch_sizes, 0.50),
            pct_u64(&dispatch_us_vec, 0.50),
            bytes, matches,
        );
        results.push(Trial {
            wall_ms, files, bytes, matches,
            b_p50: pct_u32(&batch_sizes, 0.50),
            b_p95: pct_u32(&batch_sizes, 0.95),
            d_p50: pct_u64(&dispatch_us_vec, 0.50),
            d_p95: pct_u64(&dispatch_us_vec, 0.95),
            d_p99: pct_u64(&dispatch_us_vec, 0.99),
            rss_kb,
            files_per_sec,
            mb_per_sec,
        });
    }

    let walls: Vec<f64> = results.iter().map(|t| t.wall_ms).collect();
    let wp10 = pct_f64(&walls, 0.10);
    let wp50 = pct_f64(&walls, 0.50);
    let wp90 = pct_f64(&walls, 0.90);
    let wmin = walls.iter().copied().fold(f64::MAX, f64::min);
    let wmax = walls.iter().copied().fold(f64::MIN, f64::max);
    let d_p50 = pct_u64(&results.iter().map(|t| t.d_p50).collect::<Vec<_>>(), 0.50);
    let d_p95 = pct_u64(&results.iter().map(|t| t.d_p95).collect::<Vec<_>>(), 0.50);
    let d_p99 = pct_u64(&results.iter().map(|t| t.d_p99).collect::<Vec<_>>(), 0.50);
    let files = results[0].files;
    let bytes = results[0].bytes;
    let matches = results[0].matches;
    let b_p50 = results.iter().map(|t| t.b_p50).sum::<u32>() / results.len() as u32;
    let b_p95 = results.iter().map(|t| t.b_p95).sum::<u32>() / results.len() as u32;

    let variant = format!("HoppV2:{}:{}:{}", cap, max_batch, workers);
    let rss_peak = results.iter().map(|t| t.rss_kb).max().unwrap_or(0);
    let fps_p50 = pct_f64(&results.iter().map(|t| t.files_per_sec).collect::<Vec<_>>(), 0.50);
    let mbs_p50 = pct_f64(&results.iter().map(|t| t.mb_per_sec).collect::<Vec<_>>(), 0.50);
    if tsv {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.1}\t{:.2}\t{:.1}",
            variant, pattern_tag, effect_name, arrival_name_last, policy_name, seed, trials,
            files, bytes, matches,
            wp10, wp50, wp90, wmin, wmax,
            b_p50, b_p95, d_p50, d_p95, d_p99,
            rss_peak, fps_p50, mbs_p50, enumerate_ms,
        );
    } else {
        println!(
            "{{\"variant\":\"{}\",\"pattern\":\"{}\",\"effect\":\"{}\",\"arrival\":\"{}\",\"policy\":\"{}\",\"files\":{},\"bytes\":{},\"matches\":{},\"trials\":{},\"wall_p10\":{:.3},\"wall_p50\":{:.3},\"wall_p90\":{:.3},\"wall_min\":{:.3},\"wall_max\":{:.3},\"seed\":{},\"batch_p50\":{},\"batch_p95\":{},\"disp_p50_us\":{},\"disp_p95_us\":{},\"disp_p99_us\":{},\"rss_peak_kb\":{},\"files_per_sec_p50\":{:.1},\"mb_per_sec_p50\":{:.2},\"enumerate_ms\":{:.1}}}",
            variant, pattern_tag, effect_name, arrival_name_last, policy_name,
            files, bytes, matches, trials,
            wp10, wp50, wp90, wmin, wmax, seed,
            b_p50, b_p95, d_p50, d_p95, d_p99,
            rss_peak, fps_p50, mbs_p50, enumerate_ms,
        );
    }
}
