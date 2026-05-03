// sprefa v4 — runtime lib. shared by v4-proto (demo) and v4-bench (perf).
//
//   Layers:
//     §1 Action / Gen          §6 Op trait + ident_of
//     §2 Cursor                §7 concrete ops (Fs, AstNm, Fact, Select, Print, SinglePath)
//     §3 Store + MemStore      §8 Reducer + drive
//     §4 Hooks
//     §5 Effect

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ast_grep_core::{source::StrDoc, AstGrep, Language, Pattern};
use ast_grep_language::SupportLang;
use futures::stream::{BoxStream, StreamExt};
use ignore::WalkBuilder;
use rayon::prelude::*;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;

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

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Cursor { pub terms: Vec<(Arc<str>, Arc<str>)> }

impl Cursor {
    pub fn set(&mut self, name: &str, value: impl Into<Arc<str>>) {
        let v = value.into();
        match self.terms.binary_search_by(|(n, _)| (**n).cmp(name)) {
            Ok(i)  => self.terms[i].1 = v,
            Err(i) => self.terms.insert(i, (Arc::<str>::from(name), v)),
        }
    }
    pub fn get(&self, name: &str) -> Option<&str> {
        self.terms.binary_search_by(|(n, _)| (**n).cmp(name))
            .ok().map(|i| &*self.terms[i].1)
    }
}

// ▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░
// ░  § 3   Store — relational state, redux-style dispatch             ░
// ▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░

#[async_trait::async_trait]
pub trait Store: Send + Sync + 'static {
    async fn insert(&self, fact: &str, row: Cursor, gen: Gen);
    async fn insert_many(&self, fact: &str, rows: Vec<Cursor>, gen: Gen);
    async fn remove(&self, fact: &str, row: Cursor, gen: Gen);
    async fn forget_by(&self, fact: &str, key_term: &str, key_value: &str, gen: Gen);
    async fn commit(&self, gen: Gen);
    fn define_rule(&self, name: &str, body: RuleBody);
    fn select(&self, name: &str) -> BoxStream<'static, Diff>;
    async fn snapshot(&self, name: &str) -> Vec<Cursor>;
}

#[derive(Clone, Debug)]
pub struct Diff { pub row: Cursor, pub gen: Gen, pub sign: i8 }

#[derive(Clone)]
pub enum RuleBody {
    Filter     { src: String, pred: Arc<dyn Fn(&Cursor) -> bool + Send + Sync> },
    Antijoin   { left: String, right: String, key: String },
    GroupCount { src: String, key: String, min: usize, count_term: String },
}

pub struct MemStore {
    pub facts:    RwLock<HashMap<String, HashMap<Cursor, Gen>>>,
    pub rules:    std::sync::RwLock<HashMap<String, RuleBody>>,
    pub channels: std::sync::RwLock<HashMap<String, broadcast::Sender<Diff>>>,
    pub dirty:    std::sync::Mutex<HashSet<String>>,
}

impl MemStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            facts: Default::default(), rules: Default::default(),
            channels: Default::default(), dirty: Default::default(),
        })
    }
    fn mark_dirty(&self, fact: &str) { self.dirty.lock().unwrap().insert(fact.to_string()); }
    fn channel(&self, name: &str) -> broadcast::Sender<Diff> {
        if let Some(tx) = self.channels.read().unwrap().get(name) { return tx.clone(); }
        let mut w = self.channels.write().unwrap();
        w.entry(name.to_string()).or_insert_with(|| broadcast::channel(1024).0).clone()
    }
    async fn rederive(&self, changed_fact: &str) {
        let rules = self.rules.read().unwrap().clone();
        for (name, body) in rules {
            if !body_depends_on(&body, changed_fact) { continue; }
            let derived = self.materialize(&body).await;
            let tx = self.channel(&name);
            let g = GEN.load(Ordering::SeqCst);
            for row in derived { let _ = tx.send(Diff { row, gen: g, sign: 1 }); }
        }
    }
    pub async fn materialize(&self, body: &RuleBody) -> Vec<Cursor> {
        let facts = self.facts.read().await;
        match body {
            RuleBody::Filter { src, pred } => facts.get(src)
                .map(|set| set.keys().filter(|c| pred(c)).cloned().collect())
                .unwrap_or_default(),
            RuleBody::Antijoin { left, right, key } => {
                let l: Vec<Cursor> = facts.get(left ).map(|s| s.keys().cloned().collect()).unwrap_or_default();
                let r: Vec<Cursor> = facts.get(right).map(|s| s.keys().cloned().collect()).unwrap_or_default();
                let r_keys: HashSet<String> = r.iter().filter_map(|c| c.get(key).map(str::to_owned)).collect();
                l.into_iter().filter(|c| c.get(key).map_or(true, |v| !r_keys.contains(v))).collect()
            }
            RuleBody::GroupCount { src, key, min, count_term } => {
                let s: Vec<Cursor> = facts.get(src).map(|s| s.keys().cloned().collect()).unwrap_or_default();
                let mut counts: HashMap<String, usize> = HashMap::new();
                for c in &s { if let Some(k) = c.get(key) { *counts.entry(k.into()).or_default() += 1; } }
                counts.into_iter().filter(|(_, n)| n >= min)
                    .map(|(k, n)| {
                        let mut c = Cursor::default();
                        c.set(key, k); c.set(count_term, n.to_string()); c
                    }).collect()
            }
        }
    }
}

fn body_depends_on(body: &RuleBody, fact: &str) -> bool {
    match body {
        RuleBody::Filter   { src, .. }       => src == fact,
        RuleBody::Antijoin { left, right, .. } => left == fact || right == fact,
        RuleBody::GroupCount { src, .. }     => src == fact,
    }
}

#[async_trait::async_trait]
impl Store for MemStore {
    async fn insert(&self, fact: &str, row: Cursor, gen: Gen) {
        self.insert_many(fact, vec![row], gen).await
    }
    async fn insert_many(&self, fact: &str, rows: Vec<Cursor>, gen: Gen) {
        if rows.is_empty() { return; }
        {
            let mut w = self.facts.write().await;
            let set = w.entry(fact.to_string()).or_default();
            for r in &rows { set.insert(r.clone(), gen); }
        }
        let tx = self.channel(fact);
        for r in rows { let _ = tx.send(Diff { row: r, gen, sign: 1 }); }
        self.mark_dirty(fact);
    }
    async fn commit(&self, _gen: Gen) {
        let drained: Vec<String> = { let mut d = self.dirty.lock().unwrap(); d.drain().collect() };
        for fact in drained { self.rederive(&fact).await; }
    }
    async fn remove(&self, fact: &str, row: Cursor, gen: Gen) {
        let removed = {
            let mut w = self.facts.write().await;
            w.entry(fact.to_string()).or_default().remove(&row).is_some()
        };
        if removed {
            let tx = self.channel(fact);
            let _ = tx.send(Diff { row, gen, sign: -1 });
            self.mark_dirty(fact);
        }
    }
    async fn forget_by(&self, fact: &str, key_term: &str, key_value: &str, gen: Gen) {
        let removed: Vec<_> = {
            let mut w = self.facts.write().await;
            let set = w.entry(fact.to_string()).or_default();
            let kill: Vec<Cursor> = set.keys()
                .filter(|c| c.get(key_term) == Some(key_value))
                .cloned().collect();
            for k in &kill { set.remove(k); }
            kill
        };
        if !removed.is_empty() {
            let tx = self.channel(fact);
            for r in removed { let _ = tx.send(Diff { row: r, gen, sign: -1 }); }
            self.mark_dirty(fact);
        }
    }
    fn define_rule(&self, name: &str, body: RuleBody) {
        self.rules.write().unwrap().insert(name.to_string(), body);
    }
    fn select(&self, name: &str) -> BoxStream<'static, Diff> {
        let tx = self.channels.write().unwrap()
            .entry(name.to_string())
            .or_insert_with(|| broadcast::channel(1024).0).clone();
        let rx = tx.subscribe();
        Box::pin(async_stream::stream! {
            let mut rx = rx;
            while let Ok(diff) = rx.recv().await { yield diff; }
        })
    }
    async fn snapshot(&self, name: &str) -> Vec<Cursor> {
        let r = self.rules.read().unwrap().get(name).cloned();
        if let Some(body) = r { return self.materialize(&body).await; }
        self.facts.read().await.get(name).map(|s| s.keys().cloned().collect()).unwrap_or_default()
    }
}

// ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐
// ╳    § 4   Hooks                                                  ╳
// └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘

#[derive(Clone)]
pub struct Hooks {
    pub store:   Arc<dyn Store>,
    pub effects: mpsc::UnboundedSender<Effect>,
    pub gen:     Gen,
    pub lineage: LineageId,
    pub tele:    Telemetry,
}

impl Hooks {
    pub fn use_store(&self) -> &Arc<dyn Store> { &self.store }
    pub fn use_dispatch_effect(&self, e: Effect) { let _ = self.effects.send(e); }
    pub fn use_gen(&self) -> Gen { self.gen }
    pub fn use_tele(&self) -> &Telemetry { &self.tele }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 4b   Telemetry — v3-shape spans, per-op-batch granularity
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
        let mut s = String::with_capacity(1024);
        s.push_str(&format!(
            "{:<28} {:>8} {:>10} {:>10} {:>10} {:>10} {:>12} {:>12} {:>14}\n",
            "op", "batches", "p50", "p95", "p99", "mean",
            "rows_in", "wall", "rows/s_wall",
        ));
        s.push_str(&format!(
            "{:<28} {:>8} {:>10} {:>10} {:>10} {:>10} {:>12} {:>12} {:>14}\n",
            "-".repeat(28), "-------", "---", "---", "---", "----",
            "-------", "----", "-----------",
        ));
        for r in &reports {
            let wall_s = r.wall_window_ns as f64 / 1e9;
            let rps = if wall_s > 0.0 { r.total_in.map(|n| n as f64 / wall_s) } else { None };
            s.push_str(&format!(
                "{:<28} {:>8} {:>10} {:>10} {:>10} {:>10} {:>12} {:>12} {:>14}\n",
                short_name(r.name),
                r.count,
                fmt_ns(r.p50_ns),
                fmt_ns(r.p95_ns),
                fmt_ns(r.p99_ns),
                fmt_ns(r.mean_ns),
                r.total_in.map(|n| n.to_string()).unwrap_or_else(|| "—".into()),
                fmt_ns(r.wall_window_ns),
                rps.map(|r| format!("{:.0}", r)).unwrap_or_else(|| "—".into()),
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
    sink: Arc<Mutex<Vec<Span>>>,
}

impl SpanOpen {
    pub fn close(self, n_out: Option<u64>) {
        let wall_ns = self.started.elapsed().as_nanos() as u64;
        let span = Span { name: self.name, start_ns: self.start_ns, wall_ns, n_in: self.n_in, n_out };
        self.sink.lock().unwrap().push(span);
        std::mem::forget(self);
    }
}

impl Drop for SpanOpen {
    fn drop(&mut self) {
        let wall_ns = self.started.elapsed().as_nanos() as u64;
        let span = Span { name: self.name, start_ns: self.start_ns, wall_ns, n_in: self.n_in, n_out: None };
        if let Ok(mut v) = self.sink.lock() { v.push(span); }
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
        for s in spans {
            if let Some(n) = s.n_in  { total_in  = Some(total_in.unwrap_or(0)  + n); }
            if let Some(n) = s.n_out { total_out = Some(total_out.unwrap_or(0) + n); }
        }
        Self {
            name, count,
            p50_ns: p(0.50), p95_ns: p(0.95), p99_ns: p(0.99), mean_ns,
            total_wall_ns: sum_wall,
            wall_window_ns: latest.saturating_sub(earliest),
            total_in, total_out,
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

// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
// ▒  § 5   Effect                                                  ▒
// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

#[derive(Debug)]
pub enum Effect { Print(String) }

// ◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤
// ◣◢ § 6   Op trait                                                  ◣
// ◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤

pub trait Op: Send + Sync + 'static {
    fn ident(&self) -> [u8; 32];
    fn run(self: Arc<Self>, hooks: Hooks, input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>;
}

pub fn ident_of(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    for p in parts { h.update(p); h.update(b"|"); }
    *h.finalize().as_bytes()
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//          § 7   concrete ops
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░

pub const BATCH: usize = 256;

/// Walks `root` via `ignore::WalkBuilder` (skips .git etc.), emits batches
/// of cursors with FS=path. Same enumerator v3's bench uses.
/// `batch` is the per-yield row count; `cap` is the mpsc channel cap
/// between Fs's blocking task and the rest of the pipeline.
pub struct Fs { pub root: PathBuf, pub exts: Vec<String>, pub batch: usize, pub cap: usize }
impl Fs {
    pub fn new(root: PathBuf, exts: Vec<String>) -> Self {
        Self { root, exts, batch: BATCH, cap: 8 }
    }
}
impl Op for Fs {
    fn ident(&self) -> [u8; 32] {
        let root = self.root.to_string_lossy();
        let mut bits: Vec<&[u8]> = vec![b"fs", root.as_bytes()];
        for e in &self.exts { bits.push(e.as_bytes()); }
        ident_of(&bits)
    }
    fn run(self: Arc<Self>, h: Hooks, _in: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        let batch = me.batch;
        let (tx, rx) = mpsc::channel::<Vec<Cursor>>(me.cap);
        tokio::task::spawn_blocking(move || {
            let mut buf: Vec<Cursor> = Vec::with_capacity(batch);
            for entry in WalkBuilder::new(&me.root).hidden(true).git_ignore(false).build() {
                let Ok(e) = entry else { continue };
                if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
                let p = e.into_path();
                let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
                if !me.exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }
                let mut c = Cursor::default();
                c.set("FS", p.display().to_string());
                buf.push(c);
                if buf.len() >= batch {
                    let span = tele.start("v4::Fs", None);
                    let n = buf.len() as u64;
                    if tx.blocking_send(std::mem::take(&mut buf)).is_err() { span.close(Some(n)); return; }
                    span.close(Some(n));
                    buf = Vec::with_capacity(batch);
                }
            }
            if !buf.is_empty() {
                let span = tele.start("v4::Fs", None);
                let n = buf.len() as u64;
                let _ = tx.blocking_send(buf);
                span.close(Some(n));
            }
        });
        Box::pin(ReceiverStream::new(rx))
    }
}

/// ast-grep matcher op. Runs `pattern` over each upstream cursor's FS file.
/// Per batch: hands the whole batch to spawn_blocking, runs rayon par_iter
/// on cursors. Pattern::fixed_string prefilter short-circuits non-matching
/// files at memchr speed before tree-sitter touches them.
pub struct AstNm {
    pub pattern_src:   String,
    pub lang:          SupportLang,
    pub capture_names: Vec<String>,
    pub want_match:    bool,        // store the matched span text under MATCH=
}

impl AstNm {
    pub fn new(pat: &str, lang: SupportLang, captures: &[&str]) -> Self {
        Self {
            pattern_src:   pat.to_string(),
            lang,
            capture_names: captures.iter().map(|s| s.to_string()).collect(),
            want_match:    false,
        }
    }
    pub fn with_match_text(mut self, on: bool) -> Self { self.want_match = on; self }
}

impl Op for AstNm {
    fn ident(&self) -> [u8; 32] {
        ident_of(&[b"ast", format!("{:?}", self.lang).as_bytes(), self.pattern_src.as_bytes()])
    }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me  = self.clone();
        let tele = h.tele.clone();
        let pat = Arc::new(Pattern::new(&me.pattern_src, me.lang));
        let fixed: Arc<str> = Arc::from(pat.fixed_string().to_string().as_str());
        // [perf-probe] flip these on to attribute prefilter cutoff vs ast-grep recall.
        // eprintln!("[AstNm] pattern={:?} fixed_string={:?}", me.pattern_src, fixed);
        let dbg_seen   = Arc::new(AtomicU64::new(0));
        let dbg_passed = Arc::new(AtomicU64::new(0));
        let dbg_hit    = Arc::new(AtomicU64::new(0));
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n_in = batch.len() as u64;
                let span = tele.start("v4::AstNm", Some(n_in));
                let pat   = pat.clone();
                let fixed = fixed.clone();
                let cap_names = me.capture_names.clone();
                let lang  = me.lang;
                let want_match = me.want_match;
                let dbg_seen   = dbg_seen.clone();
                let dbg_passed = dbg_passed.clone();
                let dbg_hit    = dbg_hit.clone();
                let out: Vec<Cursor> = tokio::task::spawn_blocking(move || {
                    batch.par_iter().flat_map(|c| {
                        // [perf-probe] dbg_seen.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = c.get("FS") else { return vec![] };
                        let Ok(src) = std::fs::read_to_string(path) else { return vec![] };
                        if !fixed.is_empty() && !src.contains(&*fixed) { return vec![]; }
                        // [perf-probe] dbg_passed.fetch_add(1, Ordering::Relaxed);
                        let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(&src);
                        let hits = grep.root().find_all(&*pat).map(|nm| {
                            let env = nm.get_env();
                            let r = nm.range();
                            let mut child = c.clone();
                            child.set("LO", (r.start as u64).to_string());
                            child.set("HI", (r.end   as u64).to_string());
                            if want_match { child.set("MATCH", &src[r.start..r.end]); }
                            for nm_name in &cap_names {
                                if let Some(node) = env.get_match(nm_name) {
                                    child.set(nm_name, node.text().to_string());
                                }
                            }
                            child
                        }).collect::<Vec<_>>();
                        // [perf-probe] if !hits.is_empty() { dbg_hit.fetch_add(hits.len() as u64, Ordering::Relaxed); }
                        hits
                    }).collect()
                }).await.unwrap_or_default();
                let n_out = out.len() as u64;
                span.close(Some(n_out));
                if !out.is_empty() { yield out; }
            }
            // [perf-probe] re-enable alongside the pattern eprintln above.
            // eprintln!("[AstNm] seen={} passed_prefilter={} total_hits={}",
            //     dbg_seen.load(Ordering::Relaxed),
            //     dbg_passed.load(Ordering::Relaxed),
            //     dbg_hit.load(Ordering::Relaxed));
            let _ = (&dbg_seen, &dbg_passed, &dbg_hit);
        })
    }
}

/// Insert each upstream batch into the named fact. Pass-through.
pub struct Fact { pub name: String }
impl Op for Fact {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"fact", self.name.as_bytes()]) }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n = batch.len() as u64;
                let span = tele.start("v4::Fact", Some(n));
                h.use_store().insert_many(&me.name, batch.clone(), h.use_gen()).await;
                span.close(Some(n));
                yield batch;
            }
        })
    }
}

/// Pass-through op that counts cursors for benchmarking.
pub struct Count { pub matches: Arc<AtomicU64>, pub bytes_seen: Arc<AtomicU64> }
impl Op for Count {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"count"]) }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n = batch.len() as u64;
                let span = tele.start("v4::Count", Some(n));
                me.matches.fetch_add(n, Ordering::Relaxed);
                span.close(Some(n));
                yield batch;
            }
        })
    }
}

pub struct Select { pub name: String }
impl Op for Select {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"select", self.name.as_bytes()]) }
    fn run(self: Arc<Self>, h: Hooks, _in: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let mut sub = h.use_store().select(&self.name);
        Box::pin(async_stream::stream! {
            while let Some(d) = sub.next().await {
                let mut batch = Vec::with_capacity(BATCH);
                let mut push = |d: Diff, b: &mut Vec<Cursor>| {
                    let mut c = d.row.clone();
                    c.set("GEN",  d.gen.to_string());
                    c.set("SIGN", if d.sign > 0 { "+" } else { "-" });
                    b.push(c);
                };
                push(d, &mut batch);
                while let Some(Some(more)) = futures::future::poll_immediate(sub.next()).await {
                    push(more, &mut batch);
                    if batch.len() >= BATCH { break; }
                }
                yield batch;
            }
        })
    }
}

pub struct Print { pub template: String }
impl Op for Print {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"print", self.template.as_bytes()]) }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                for c in &batch {
                    let mut s = me.template.clone();
                    for (n, v) in &c.terms {
                        s = s.replace(&format!("{{{}}}", n), v);
                    }
                    h.use_dispatch_effect(Effect::Print(s));
                }
                if false { yield batch; }
            }
        })
    }
}

pub struct SinglePath { pub path: PathBuf }
impl Op for SinglePath {
    fn ident(&self) -> [u8;32] { ident_of(&[b"single", self.path.to_string_lossy().as_bytes()]) }
    fn run(self: Arc<Self>, _h: Hooks, _in: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let p = self.path.clone();
        Box::pin(async_stream::stream! {
            let mut c = Cursor::default();
            c.set("FS", p.display().to_string());
            yield vec![c];
        })
    }
}

// ▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟
// ▜▛  § 8   Reducer + drive                                          ▜▛
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

pub async fn drive(chain: Vec<Arc<dyn Op>>, h: Hooks) { drive_with(chain, h, 8).await }

/// Run a chain with explicit channel cap between ops. Producer can be
/// up to `cap` batches ahead of consumer; raise to absorb burstier
/// upstreams, lower to surface back-pressure as Fs span wall.
pub async fn drive_with(chain: Vec<Arc<dyn Op>>, h: Hooks, cap: usize) {
    if chain.is_empty() { return; }
    let mut prev_rx: Option<mpsc::Receiver<Vec<Cursor>>> = None;
    let mut handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let n = chain.len();
    for (i, op) in chain.into_iter().enumerate() {
        let is_last = i + 1 == n;
        let in_stream: BoxStream<'static, Vec<Cursor>> = match prev_rx.take() {
            Some(rx) => Box::pin(ReceiverStream::new(rx)),
            None     => Box::pin(futures::stream::empty()),
        };
        let hooks = h.clone();
        if is_last {
            handles.push(tokio::spawn(async move {
                let mut s = op.run(hooks, in_stream);
                while s.next().await.is_some() {}
            }));
        } else {
            let (tx, rx) = mpsc::channel::<Vec<Cursor>>(cap);
            prev_rx = Some(rx);
            handles.push(tokio::spawn(async move {
                let mut s = op.run(hooks, in_stream);
                while let Some(batch) = s.next().await {
                    if tx.send(batch).await.is_err() { return; }
                }
            }));
        }
    }
    for h in handles { let _ = h.await; }
}
