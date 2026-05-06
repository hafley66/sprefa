pub mod app;
pub mod chan;
pub mod compile;
pub mod cst;
pub mod cursor_codec;
pub mod fact;
pub mod pipeline;
pub mod rule;
pub mod runtime_bridge;
pub mod v2_ops;
pub mod term;

pub use compile::lower;

// sprefa v4 — runtime lib. shared by v4-proto (demo) and v4-bench (perf).
//
//   Layers:
//     §1 Action / Gen
//     §2 Cursor (+ Row impl, Interner)
//     §4 Telemetry / Span
//     §5 Action / lineage counters
//
//   Fact storage is `effect_runtime::v2::FactStore<Cursor>`; impls
//   are `MemFactStore` / `SqliteFactStore`. Re-exported via
//   `crate::fact`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ░░░▒▒▒▓▓▓██████████████████████████████████████████████████████▓▓▓▒▒▒░░░
// ░░░▒                  § 1   actions / dispatch                    ▒░░░
// ░░░▒▒▒▓▓▓██████████████████████████████████████████████████████▓▓▓▒▒▒░░░

pub type Gen        = u64;
pub type LineageId  = u64;

#[derive(Clone, Debug)]
pub struct Action {
    pub gen:    Gen,
    pub parent: Option<(Gen, LineageId)>,
    pub kind:   ActionKind,
}

#[derive(Clone, Debug)]
pub enum ActionKind {
    Run         { root: PathBuf },
    FileChanged { path: PathBuf },
    Quit,
}

// ╔═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╗
// ║         § 2   cursor — dynamic-scope term-capture bag         ║
// ╚═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╝

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Cursor {
    /// Focal value `&.value`. Default for Term::Bind. Source ops set
    /// this to the per-row payload (path, hit text, etc.); cursor
    /// mutators rewrite it; Term::Read pulls from `terms` into here.
    pub value: Arc<str>,
    /// Sorted bag of (name, value) captures. ALL-CAPS keys = user
    /// captures (`X`), colon-prefixed keys = internal terms (`:fan_idx`).
    pub terms: Vec<(Arc<str>, Arc<str>)>,
}

impl Cursor {
    pub fn set(&mut self, name: &str, value: impl Into<Arc<str>>) {
        let v = value.into();
        match self.terms.binary_search_by(|(n, _)| (**n).cmp(name)) {
            Ok(i)  => self.terms[i].1 = v,
            Err(i) => self.terms.insert(i, (Arc::<str>::from(name), v)),
        }
    }
    /// Set with a pre-built Arc<str> value. Use this with Interner so
    /// repeated values (e.g. file paths) share heap.
    pub fn set_arc(&mut self, name: &str, value: Arc<str>) {
        match self.terms.binary_search_by(|(n, _)| (**n).cmp(name)) {
            Ok(i)  => self.terms[i].1 = value,
            Err(i) => self.terms.insert(i, (Arc::<str>::from(name), value)),
        }
    }
    pub fn get(&self, name: &str) -> Option<&str> {
        self.terms.binary_search_by(|(n, _)| (**n).cmp(name))
            .ok().map(|i| &*self.terms[i].1)
    }
    pub fn unset(&mut self, name: &str) {
        if let Ok(i) = self.terms.binary_search_by(|(n, _)| (**n).cmp(name)) {
            self.terms.remove(i);
        }
    }
    /// `&.value`. The focal value of the current cursor.
    pub fn value(&self) -> &str { &self.value }
}

impl effect_runtime::v2::Row for Cursor {
    fn get(&self, col: &str) -> Option<&str> { Cursor::get(self, col) }
    fn set(&mut self, col: &str, value: &str) { Cursor::set(self, col, value); }
    fn fields(&self) -> Vec<(&str, &str)> {
        self.terms.iter().map(|(n, v)| (n.as_ref(), v.as_ref())).collect()
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 2b  Interner — share Arc<str> heap for repeated values
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//
// The matcher emits a Cursor per match. File path strings repeat
// thousands of times across matches (avg ~80 matches per file in the
// $F($$$) workload). Without interning, each cursor allocates a fresh
// Arc<str> for the path. With interning, every cursor for the same
// file holds the SAME Arc<str> — an 8-byte pointer + atomic refcount
// bump — so the per-row heap cost drops from ~50 bytes (path content)
// to 8 bytes (pointer).
//
// Cost: HashMap lookup per intern call (one per emitted cursor for
// the path column). Amortized fast because hits dominate; misses
// happen only on first sighting.

#[derive(Default)]
pub struct Interner {
    /// Forward: content -> id.
    fwd: std::sync::RwLock<HashMap<Arc<str>, u32>>,
    /// Reverse: id -> content. id is index into this Vec.
    rev: std::sync::RwLock<Vec<Arc<str>>>,
}

impl Interner {
    pub fn new() -> Arc<Self> { Arc::new(Self::default()) }
    /// Return the canonical Arc<str> for `s`. Repeated calls with
    /// equal content return the same Arc clone.
    pub fn intern(&self, s: &str) -> Arc<str> {
        let id = self.intern_id(s);
        self.rev.read().unwrap()[id as usize].clone()
    }
    /// Return a stable u32 id for `s`. Cheaper than intern() when
    /// downstream storage is integer-keyed (DdStore arrangements).
    pub fn intern_id(&self, s: &str) -> u32 {
        if let Some(&id) = self.fwd.read().unwrap().get(s) { return id; }
        let mut fwd = self.fwd.write().unwrap();
        if let Some(&id) = fwd.get(s) { return id; }
        let mut rev = self.rev.write().unwrap();
        let id = rev.len() as u32;
        let arc: Arc<str> = Arc::from(s);
        fwd.insert(arc.clone(), id);
        rev.push(arc);
        id
    }
    /// Resolve id -> Arc<str>. Caller must use ids issued by this
    /// interner.
    pub fn lookup(&self, id: u32) -> Arc<str> {
        self.rev.read().unwrap()[id as usize].clone()
    }
    pub fn len(&self) -> usize { self.fwd.read().unwrap().len() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}


// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 4   Telemetry — v3-shape spans, per-op-batch granularity
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//
// One Telemetry per drive() call. Each op opens a span per batch via
// `tele.start("v4::Fs", Some(in_rows))` and closes it with `out_rows`.
// Summary groups by name and reports count / p50 / p95 / p99 / mean
// plus wall_window (latest_close − earliest_open) so concurrent batches
// across ops report aggregate throughput, not summed wall.

#[derive(Clone, Debug)]
pub struct Span {
    pub name:    &'static str,
    pub start_ns: u64,
    pub wall_ns:  u64,
    pub n_in:    Option<u64>,
    pub n_out:   Option<u64>,
    /// Bytes consumed by this span (sum of file sizes parsed, etc.).
    pub bytes_in: Option<u64>,
    /// Time spent in tree-sitter parse (sum across files inside batch).
    pub parse_ns: Option<u64>,
    /// Time spent in find_all + match collection.
    pub match_ns: Option<u64>,
    /// Process RSS at span close, KB. Sampled via getrusage.
    pub rss_kb_end: Option<u64>,
}

#[derive(Clone)]
pub struct Telemetry {
    inner: Arc<Mutex<Vec<Span>>>,
    epoch: Arc<Mutex<Instant>>,
}

impl Default for Telemetry { fn default() -> Self { Self::new() } }

impl Telemetry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::with_capacity(1 << 14))),
            epoch: Arc::new(Mutex::new(Instant::now())),
        }
    }
    pub fn start(&self, name: &'static str, n_in: Option<u64>) -> SpanOpen {
        let epoch = *self.epoch.lock().unwrap();
        let now   = Instant::now();
        SpanOpen {
            name,
            started:  now,
            start_ns: now.saturating_duration_since(epoch).as_nanos() as u64,
            n_in,
            bytes_in: None,
            parse_ns: None,
            match_ns: None,
            sink: self.inner.clone(),
        }
    }
    pub fn snapshot(&self) -> Vec<Span> { self.inner.lock().unwrap().clone() }
    pub fn drain(&self) -> Vec<Span> { std::mem::take(&mut *self.inner.lock().unwrap()) }
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
        *self.epoch.lock().unwrap() = Instant::now();
    }
    pub fn report(&self) -> Vec<OpReport> {
        let spans = self.inner.lock().unwrap();
        let mut by: HashMap<&'static str, Vec<&Span>> = HashMap::new();
        for s in spans.iter() { by.entry(s.name).or_default().push(s); }
        let mut out: Vec<OpReport> = by.into_iter()
            .map(|(n, v)| OpReport::from_spans(n, &v)).collect();
        out.sort_by(|a, b| b.total_wall_ns.cmp(&a.total_wall_ns));
        out
    }
    pub fn summary(&self) -> String {
        let reports = self.report();
        let mut s = String::with_capacity(2048);
        // Row 1: timing + throughput
        s.push_str(&format!(
            "{:<20} {:>7} {:>9} {:>9} {:>9} {:>9} {:>10} {:>9} {:>10}\n",
            "op", "batches", "p50", "p95", "p99", "mean", "wall", "MB", "MB/s",
        ));
        s.push_str(&format!(
            "{:<20} {:>7} {:>9} {:>9} {:>9} {:>9} {:>10} {:>9} {:>10}\n",
            "-".repeat(20), "-------", "---", "---", "---", "----",
            "----", "--", "----",
        ));
        for r in &reports {
            let wall_s = r.wall_window_ns as f64 / 1e9;
            let mb = r.total_bytes_in.map(|b| b as f64 / 1_048_576.0);
            let mbs = match (mb, wall_s) {
                (Some(m), w) if w > 0.0 => Some(m / w),
                _ => None,
            };
            s.push_str(&format!(
                "{:<20} {:>7} {:>9} {:>9} {:>9} {:>9} {:>10} {:>9} {:>10}\n",
                short_name(r.name),
                r.count,
                fmt_ns(r.p50_ns), fmt_ns(r.p95_ns), fmt_ns(r.p99_ns), fmt_ns(r.mean_ns),
                fmt_ns(r.wall_window_ns),
                mb.map(|m| format!("{:.1}", m)).unwrap_or_else(|| "—".into()),
                mbs.map(|m| format!("{:.1}", m)).unwrap_or_else(|| "—".into()),
            ));
        }
        // Row 2: parse vs match split + RSS
        s.push('\n');
        s.push_str(&format!(
            "{:<20} {:>10} {:>10} {:>7} {:>10} {:>10} {:>10}\n",
            "op", "parse_sum", "match_sum", "p/m", "rss_min", "rss_max", "rss_last",
        ));
        s.push_str(&format!(
            "{:<20} {:>10} {:>10} {:>7} {:>10} {:>10} {:>10}\n",
            "-".repeat(20), "---------", "---------", "---", "-------", "-------", "--------",
        ));
        for r in &reports {
            let pm_ratio = match (r.total_parse_ns, r.total_match_ns) {
                (Some(p), Some(m)) if m > 0 => Some(p as f64 / m as f64),
                _ => None,
            };
            let mb_str = |kb: Option<u64>| kb.map(|k| format!("{} MB", k / 1024)).unwrap_or_else(|| "—".into());
            s.push_str(&format!(
                "{:<20} {:>10} {:>10} {:>7} {:>10} {:>10} {:>10}\n",
                short_name(r.name),
                r.total_parse_ns.map(fmt_ns).unwrap_or_else(|| "—".into()),
                r.total_match_ns.map(fmt_ns).unwrap_or_else(|| "—".into()),
                pm_ratio.map(|x| format!("{:.1}×", x)).unwrap_or_else(|| "—".into()),
                mb_str(r.rss_kb_min),
                mb_str(r.rss_kb_max),
                mb_str(r.rss_kb_last),
            ));
        }
        s
    }
}

pub struct SpanOpen {
    name:    &'static str,
    started: Instant,
    start_ns: u64,
    n_in:    Option<u64>,
    bytes_in: Option<u64>,
    parse_ns: Option<u64>,
    match_ns: Option<u64>,
    sink: Arc<Mutex<Vec<Span>>>,
}

impl SpanOpen {
    pub fn set_bytes(&mut self, bytes: u64) { self.bytes_in = Some(bytes); }
    pub fn set_parse_ns(&mut self, ns: u64) { self.parse_ns = Some(ns); }
    pub fn set_match_ns(&mut self, ns: u64) { self.match_ns = Some(ns); }
    pub fn close(self, n_out: Option<u64>) {
        let wall_ns = self.started.elapsed().as_nanos() as u64;
        let rss_kb_end = Some(rss_peak_kb_now());
        let span = Span {
            name: self.name, start_ns: self.start_ns, wall_ns,
            n_in: self.n_in, n_out,
            bytes_in: self.bytes_in,
            parse_ns: self.parse_ns,
            match_ns: self.match_ns,
            rss_kb_end,
        };
        self.sink.lock().unwrap().push(span);
        std::mem::forget(self);
    }
}

impl Drop for SpanOpen {
    fn drop(&mut self) {
        let wall_ns = self.started.elapsed().as_nanos() as u64;
        let span = Span {
            name: self.name, start_ns: self.start_ns, wall_ns,
            n_in: self.n_in, n_out: None,
            bytes_in: self.bytes_in,
            parse_ns: self.parse_ns,
            match_ns: self.match_ns,
            rss_kb_end: Some(rss_peak_kb_now()),
        };
        if let Ok(mut v) = self.sink.lock() { v.push(span); }
    }
}

/// Push a synthetic span with a precomputed wall_ns. Useful when the
/// timed work happens on a non-async thread (e.g. timely worker) and
/// we forward accumulated nanoseconds across a channel boundary.
pub fn push_synthetic_span(t: &Telemetry, name: &'static str, wall_ns: u64, n_out: Option<u64>) {
    let epoch = *t.epoch.lock().unwrap();
    let now   = Instant::now();
    let start_ns = now.saturating_duration_since(epoch).as_nanos() as u64;
    let span = Span {
        name, start_ns, wall_ns,
        n_in: None, n_out,
        bytes_in: None, parse_ns: None, match_ns: None,
        rss_kb_end: Some(rss_peak_kb_now()),
    };
    t.inner.lock().unwrap().push(span);
}

fn rss_peak_kb_now() -> u64 {
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 { return 0; }
        #[cfg(target_os = "macos")] { (u.ru_maxrss as u64) / 1024 }
        #[cfg(not(target_os = "macos"))] { u.ru_maxrss as u64 }
    }
}

#[derive(Debug, Clone)]
pub struct OpReport {
    pub name: &'static str,
    pub count: usize,
    pub p50_ns: u64, pub p95_ns: u64, pub p99_ns: u64, pub mean_ns: u64,
    pub total_wall_ns: u64,
    pub wall_window_ns: u64,
    pub total_in: Option<u64>,
    pub total_out: Option<u64>,
    pub total_bytes_in: Option<u64>,
    pub total_parse_ns: Option<u64>,
    pub total_match_ns: Option<u64>,
    pub rss_kb_min: Option<u64>,
    pub rss_kb_max: Option<u64>,
    pub rss_kb_last: Option<u64>,
}

impl OpReport {
    fn from_spans(name: &'static str, spans: &[&Span]) -> Self {
        let count = spans.len();
        let mut walls: Vec<u64> = spans.iter().map(|s| s.wall_ns).collect();
        walls.sort_unstable();
        let p = |q: f64| -> u64 {
            if walls.is_empty() { return 0; }
            walls[((walls.len() - 1) as f64 * q).round() as usize]
        };
        let sum_wall: u64 = walls.iter().sum();
        let mean_ns = if count > 0 { sum_wall / count as u64 } else { 0 };
        let earliest = spans.iter().map(|s| s.start_ns).min().unwrap_or(0);
        let latest   = spans.iter().map(|s| s.start_ns.saturating_add(s.wall_ns)).max().unwrap_or(0);
        let mut total_in:  Option<u64> = None;
        let mut total_out: Option<u64> = None;
        let mut total_bytes_in: Option<u64> = None;
        let mut total_parse_ns: Option<u64> = None;
        let mut total_match_ns: Option<u64> = None;
        let mut rss_kb_min: Option<u64> = None;
        let mut rss_kb_max: Option<u64> = None;
        let mut rss_kb_last: Option<u64> = None;
        for s in spans {
            if let Some(n) = s.n_in  { total_in  = Some(total_in.unwrap_or(0)  + n); }
            if let Some(n) = s.n_out { total_out = Some(total_out.unwrap_or(0) + n); }
            if let Some(b) = s.bytes_in { total_bytes_in = Some(total_bytes_in.unwrap_or(0) + b); }
            if let Some(p) = s.parse_ns { total_parse_ns = Some(total_parse_ns.unwrap_or(0) + p); }
            if let Some(m) = s.match_ns { total_match_ns = Some(total_match_ns.unwrap_or(0) + m); }
            if let Some(r) = s.rss_kb_end {
                rss_kb_min = Some(rss_kb_min.map(|x| x.min(r)).unwrap_or(r));
                rss_kb_max = Some(rss_kb_max.map(|x| x.max(r)).unwrap_or(r));
                rss_kb_last = Some(r);
            }
        }
        Self {
            name, count,
            p50_ns: p(0.50), p95_ns: p(0.95), p99_ns: p(0.99), mean_ns,
            total_wall_ns: sum_wall,
            wall_window_ns: latest.saturating_sub(earliest),
            total_in, total_out,
            total_bytes_in, total_parse_ns, total_match_ns,
            rss_kb_min, rss_kb_max, rss_kb_last,
        }
    }
}

fn fmt_ns(ns: u64) -> String {
    if ns >= 1_000_000_000 { format!("{:.2}s",  ns as f64 / 1e9) }
    else if ns >= 1_000_000 { format!("{:.1}ms", ns as f64 / 1e6) }
    else if ns >= 1_000     { format!("{:.1}µs", ns as f64 / 1e3) }
    else                    { format!("{}ns", ns) }
}
fn short_name(full: &'static str) -> String {
    full.rsplit("::").next().unwrap_or(full).to_string()
}


// ▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟
// ▜▛  § 5   Action / lineage counters (process-globals)              ▜▛
// ▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟

pub static GEN: AtomicU64 = AtomicU64::new(0);
pub static LIN: AtomicU64 = AtomicU64::new(0);

pub fn new_action(kind: ActionKind, parent: Option<(Gen, LineageId)>) -> Action {
    Action {
        gen: GEN.fetch_add(1, Ordering::SeqCst) + 1,
        parent, kind,
    }
}

pub fn new_lineage() -> LineageId { LIN.fetch_add(1, Ordering::SeqCst) + 1 }
