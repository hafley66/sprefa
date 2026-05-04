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
use std::sync::{Arc, Mutex, OnceLock};
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Cursor { pub terms: Vec<(Arc<str>, Arc<str>)> }

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
    /// Declare the fact's column set. Backends that need typed storage
    /// (sqlite) materialize the table here. Mem/Dd ignore.
    fn ensure_schema(&self, _fact: &str, _cols: &[&str]) {}
    /// Brute-force per-batch IN-clause read: rows where `key_col`
    /// matches any of `key_values`, projected to `project` cols.
    /// Returned as Cursors carrying the projected terms.
    async fn read_in(
        &self,
        fact: &str,
        key_col: &str,
        key_values: Vec<String>,
        project: Vec<String>,
    ) -> Vec<Cursor>;
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
    /// Optional telemetry sink. Attach via `attach_tele` to record store
    /// spans (ins / commit / rederive) into the same Telemetry the ops use.
    pub tele:     OnceLock<Telemetry>,
}

impl MemStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            facts: Default::default(), rules: Default::default(),
            channels: Default::default(), dirty: Default::default(),
            tele: OnceLock::new(),
        })
    }
    pub fn attach_tele(&self, t: Telemetry) { let _ = self.tele.set(t); }
    fn span(&self, name: &'static str, n_in: Option<u64>) -> Option<SpanOpen> {
        self.tele.get().map(|t| t.start(name, n_in))
    }
    /// Stats footer: per-fact (rows, bytes), per-rule cardinality.
    /// Bytes are an estimate: sum of (term name + value + 16 bytes/entry).
    pub async fn stats(&self) -> StoreStats {
        let f = self.facts.read().await;
        let mut facts: Vec<FactStats> = f.iter().map(|(name, rows)| {
            let bytes: u64 = rows.keys()
                .map(|c| c.terms.iter()
                     .map(|(n, v)| (n.len() + v.len() + 16) as u64).sum::<u64>())
                .sum();
            FactStats { name: name.clone(), rows: rows.len() as u64, bytes }
        }).collect();
        facts.sort_by(|a, b| b.rows.cmp(&a.rows));
        let rules = self.rules.read().unwrap().len() as u64;
        let channels = self.channels.read().unwrap().len() as u64;
        StoreStats { facts, rules, channels }
    }
    fn mark_dirty(&self, fact: &str) { self.dirty.lock().unwrap().insert(fact.to_string()); }
    fn channel(&self, name: &str) -> broadcast::Sender<Diff> {
        if let Some(tx) = self.channels.read().unwrap().get(name) { return tx.clone(); }
        let mut w = self.channels.write().unwrap();
        w.entry(name.to_string()).or_insert_with(|| broadcast::channel(1024).0).clone()
    }
    async fn rederive(&self, changed_fact: &str) -> u64 {
        let rules = self.rules.read().unwrap().clone();
        let mut total: u64 = 0;
        for (name, body) in rules {
            if !body_depends_on(&body, changed_fact) { continue; }
            let sp = self.span("v4::Mem::rederive", None);
            let derived = self.materialize(&body).await;
            let n = derived.len() as u64;
            let tx = self.channel(&name);
            let g = GEN.load(Ordering::SeqCst);
            for row in derived { let _ = tx.send(Diff { row, gen: g, sign: 1 }); }
            if let Some(s) = sp { s.close(Some(n)); }
            total += n;
        }
        total
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

#[derive(Debug, Clone)]
pub struct FactStats { pub name: String, pub rows: u64, pub bytes: u64 }
#[derive(Debug, Clone)]
pub struct StoreStats { pub facts: Vec<FactStats>, pub rules: u64, pub channels: u64 }

impl StoreStats {
    pub fn summary(&self) -> String {
        let mut s = String::with_capacity(512);
        s.push_str(&format!("{:<24} {:>12} {:>12}\n", "fact", "rows", "bytes"));
        s.push_str(&format!("{:<24} {:>12} {:>12}\n",
            "-".repeat(24), "-".repeat(12), "-".repeat(12)));
        let mut total_rows = 0u64;
        let mut total_bytes = 0u64;
        for f in &self.facts {
            s.push_str(&format!("{:<24} {:>12} {:>11} K\n",
                &f.name, f.rows, f.bytes / 1024));
            total_rows  += f.rows;
            total_bytes += f.bytes;
        }
        s.push_str(&format!("{:<24} {:>12} {:>11} K\n",
            "TOTAL", total_rows, total_bytes / 1024));
        s.push_str(&format!("rules={} channels={}\n", self.rules, self.channels));
        s
    }
}

fn cursor_bytes(c: &Cursor) -> u64 {
    c.terms.iter().map(|(n, v)| (n.len() + v.len() + 16) as u64).sum()
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
        let n = rows.len() as u64;
        let mut sp = self.span("v4::Mem::ins", Some(n));
        if let Some(s) = sp.as_mut() {
            let bytes: u64 = rows.iter().map(cursor_bytes).sum();
            s.set_bytes(bytes);
        }
        {
            let mut w = self.facts.write().await;
            let set = w.entry(fact.to_string()).or_default();
            for r in &rows { set.insert(r.clone(), gen); }
        }
        let tx = self.channel(fact);
        for r in rows { let _ = tx.send(Diff { row: r, gen, sign: 1 }); }
        self.mark_dirty(fact);
        if let Some(s) = sp { s.close(Some(n)); }
    }
    async fn commit(&self, _gen: Gen) {
        let sp = self.span("v4::Mem::commit", None);
        let drained: Vec<String> = { let mut d = self.dirty.lock().unwrap(); d.drain().collect() };
        let mut total_out: u64 = 0;
        for fact in &drained { total_out += self.rederive(fact).await; }
        if let Some(s) = sp { s.close(Some(total_out)); }
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
    async fn read_in(
        &self,
        fact: &str,
        key_col: &str,
        key_values: Vec<String>,
        project: Vec<String>,
    ) -> Vec<Cursor> {
        let keys: HashSet<String> = key_values.into_iter().collect();
        let f = self.facts.read().await;
        let Some(set) = f.get(fact) else { return vec![] };
        set.keys()
            .filter(|c| c.get(key_col).map_or(false, |v| keys.contains(v)))
            .map(|c| {
                let mut out = Cursor::default();
                if let Some(v) = c.get(key_col) { out.set(key_col, v); }
                for col in &project {
                    if let Some(v) = c.get(col) { out.set(col, v); }
                }
                out
            })
            .collect()
    }
}

// ▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░
// ░  § 3b  DdStore — differential-dataflow Store impl                  ░
// ▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░
//
// Single-worker timely runtime, single fact "matches", N copies of the
// GroupCount rule pre-built at construction. Async store API marshals
// commands across a std::sync::mpsc channel into the worker thread.
// Lab scope: prove DD's incremental delta cost vs MemStore's full
// rederive. Generalize fact names / rule kinds after the proof point.

pub struct DdStore {
    cmd_tx:        std::sync::mpsc::SyncSender<DdCmd>,
    handle:        Mutex<Option<std::thread::JoinHandle<()>>>,
    tele:          OnceLock<Telemetry>,
    rule_outputs:  std::sync::RwLock<HashMap<String, broadcast::Sender<Diff>>>,
    /// Per-rule (row -> signed multiplicity) accumulator, updated from
    /// inspect_batch on the worker thread. Read from `snapshot()` to
    /// verify output parity against MemStore.
    rule_state:    Arc<Mutex<HashMap<String, HashMap<DdRow, isize>>>>,
    /// Shared with caller. Used to convert Cursor ↔ DdRow (id-keyed).
    interner:      Arc<Interner>,
}

/// DD-side row representation: 4-byte ids on each side, 8 bytes per
/// term vs ~32 bytes for (Arc<str>, Arc<str>). Repeated values
/// (paths, term names, pattern labels) collapse to one entry in the
/// interner regardless of how many rows reference them.
type DdRow = Vec<(u32, u32)>;

fn cursor_to_dd(c: &Cursor, interner: &Interner) -> DdRow {
    c.terms.iter()
        .map(|(n, v)| (interner.intern_id(n), interner.intern_id(v)))
        .collect()
}
fn dd_to_cursor(r: &DdRow, interner: &Interner) -> Cursor {
    let mut c = Cursor::default();
    for (n_id, v_id) in r {
        let n = interner.lookup(*n_id);
        let v = interner.lookup(*v_id);
        c.set_arc(&n, v);
    }
    c
}

enum DdCmd {
    Insert { fact: String, rows: Vec<DdRow>, gen: Gen },
    Commit { gen: Gen, ack: tokio::sync::oneshot::Sender<DdAck> },
    Stop,
}

#[derive(Default, Debug)]
pub struct DdAck { pub derived: u64, pub advance_ns: u64, pub step_ns: u64 }

impl DdStore {
    /// Build a DdStore with `rules` pre-attached over fact `fact_name`.
    /// Each rule is GroupCount(src=fact_name, key, min, count_term).
    pub fn new(fact_name: String, rules: Vec<(String, RuleBody)>, interner: Arc<Interner>) -> Arc<Self> {
        use differential_dataflow::input::Input;
        use differential_dataflow::operators::Reduce;

        let (cmd_tx, cmd_rx) = std::sync::mpsc::sync_channel::<DdCmd>(1024);
        let rule_outputs: HashMap<String, broadcast::Sender<Diff>> = rules.iter()
            .map(|(n, _)| (n.clone(), broadcast::channel(1024).0))
            .collect();
        let outputs_for_worker = rule_outputs.clone();
        let fact_for_worker = fact_name.clone();
        let rules_for_worker = rules.clone();
        let interner_for_worker = interner.clone();
        let rule_state: Arc<Mutex<HashMap<String, HashMap<DdRow, isize>>>> =
            Arc::new(Mutex::new(rules.iter().map(|(n, _)| (n.clone(), HashMap::new())).collect()));
        let rule_state_for_worker = rule_state.clone();

        // timely::execute_directly requires its worker closure to be
        // Send + Sync. Receiver isn't Sync, so wrap it. Single-threaded
        // worker contention is impossible (one worker, one thread) so
        // the Mutex is uncontended.
        let cmd_rx = Arc::new(Mutex::new(cmd_rx));
        let handle = std::thread::spawn(move || {
            let cmd_rx = cmd_rx.clone();
            timely::execute_directly(move |worker| {
                let cmd_rx = cmd_rx.lock().unwrap();
                use timely::dataflow::operators::probe::Handle as ProbeHandle;
                let derived_counter = Arc::new(AtomicU64::new(0));
                let mut probes: Vec<ProbeHandle<Gen>> = Vec::new();
                // Internal monotonic time. The Hooks.gen value is per-
                // trial and may not change between commits in a single
                // trial, which would let advance_to silently no-op.
                // We bump on every Commit, and route each Insert to the
                // current time. The caller's gen is preserved on Diff
                // for downstream subscribers.
                let mut dd_time: Gen = 0;
                let mut input = worker.dataflow::<Gen, _, _>(|scope| {
                    let (input, facts) = scope.new_collection::<DdRow, isize>();
                    for (rule_name, body) in &rules_for_worker {
                        let RuleBody::GroupCount { key, min, count_term, .. } = body else { continue };
                        let key_name_id    = interner_for_worker.intern_id(key);
                        let count_term_id  = interner_for_worker.intern_id(count_term);
                        let min = *min;
                        let out_tx = outputs_for_worker.get(rule_name).unwrap().clone();
                        let derived = derived_counter.clone();
                        let rule_state_inner = rule_state_for_worker.clone();
                        let rule_name_owned = rule_name.clone();
                        let interner_for_reduce  = interner_for_worker.clone();
                        let interner_for_inspect = interner_for_worker.clone();
                        let stream = facts
                            .map(move |r: DdRow| {
                                let k_id = r.iter().find(|(n, _)| *n == key_name_id)
                                    .map(|(_, v)| *v).unwrap_or(u32::MAX);
                                (k_id, r)
                            })
                            .reduce(move |k_id, vs, out| {
                                let n = vs.iter().map(|(_, m)| *m).sum::<isize>();
                                if n >= min as isize {
                                    let n_str_id = interner_for_reduce.intern_id(&n.to_string());
                                    let row: DdRow = vec![
                                        (key_name_id, *k_id),
                                        (count_term_id, n_str_id),
                                    ];
                                    out.push((row, 1));
                                }
                            })
                            .inspect_batch(move |t, batch| {
                                let mut state = rule_state_inner.lock().unwrap();
                                let entry = state.entry(rule_name_owned.clone()).or_default();
                                for ((_k, row), _t, sign) in batch {
                                    derived.fetch_add(1, Ordering::Relaxed);
                                    *entry.entry(row.clone()).or_insert(0) += *sign as isize;
                                    let _ = out_tx.send(Diff {
                                        row: dd_to_cursor(row, &interner_for_inspect),
                                        gen: *t,
                                        sign: *sign as i8,
                                    });
                                }
                            });
                        probes.push(stream.probe());
                    }
                    input
                });

                loop {
                    match cmd_rx.recv() {
                        Ok(DdCmd::Insert { fact, rows, gen: _ }) => {
                            if fact != fact_for_worker { continue; }
                            input.advance_to(dd_time);
                            for r in rows { input.insert(r); }
                        }
                        Ok(DdCmd::Commit { gen: _, ack }) => {
                            let t_adv = Instant::now();
                            dd_time += 1;
                            input.advance_to(dd_time);
                            input.flush();
                            let advance_ns = t_adv.elapsed().as_nanos() as u64;
                            let t_step = Instant::now();
                            let prev = derived_counter.load(Ordering::Relaxed);
                            while probes.iter().any(|p| p.less_than(input.time())) {
                                worker.step();
                            }
                            let step_ns = t_step.elapsed().as_nanos() as u64;
                            let derived = derived_counter.load(Ordering::Relaxed) - prev;
                            let _ = ack.send(DdAck { derived, advance_ns, step_ns });
                        }
                        Ok(DdCmd::Stop) => break,
                        Err(_) => break,
                    }
                }
            });
        });

        Arc::new(Self {
            cmd_tx,
            handle: Mutex::new(Some(handle)),
            tele: OnceLock::new(),
            rule_outputs: std::sync::RwLock::new(rule_outputs),
            rule_state,
            interner,
        })
    }
    pub fn attach_tele(&self, t: Telemetry) { let _ = self.tele.set(t); }
    fn span(&self, name: &'static str, n_in: Option<u64>) -> Option<SpanOpen> {
        self.tele.get().map(|t| t.start(name, n_in))
    }
}

#[async_trait::async_trait]
impl Store for DdStore {
    async fn insert(&self, fact: &str, row: Cursor, gen: Gen) {
        self.insert_many(fact, vec![row], gen).await
    }
    async fn insert_many(&self, fact: &str, rows: Vec<Cursor>, gen: Gen) {
        if rows.is_empty() { return; }
        let n = rows.len() as u64;
        let mut sp = self.span("v4::Dd::ins", Some(n));
        if let Some(s) = sp.as_mut() {
            let bytes: u64 = rows.iter().map(cursor_bytes).sum();
            s.set_bytes(bytes);
        }
        let dd_rows: Vec<DdRow> = rows.iter().map(|c| cursor_to_dd(c, &self.interner)).collect();
        let _ = self.cmd_tx.send(DdCmd::Insert { fact: fact.to_string(), rows: dd_rows, gen });
        if let Some(s) = sp { s.close(Some(n)); }
    }
    async fn commit(&self, gen: Gen) {
        let sp = self.span("v4::Dd::commit", None);
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        let _ = self.cmd_tx.send(DdCmd::Commit { gen, ack: ack_tx });
        let ack = ack_rx.await.unwrap_or_default();
        // Synthesize child spans from worker-side accumulators so the
        // summary table breaks DD::commit into advance + step phases.
        if let Some(t) = self.tele.get() {
            push_synthetic_span(t, "v4::Dd::advance", ack.advance_ns, None);
            push_synthetic_span(t, "v4::Dd::step",    ack.step_ns,    Some(ack.derived));
        }
        if let Some(s) = sp { s.close(Some(ack.derived)); }
    }
    async fn remove(&self, _fact: &str, _row: Cursor, _gen: Gen) {}
    async fn forget_by(&self, _fact: &str, _key: &str, _v: &str, _gen: Gen) {}
    fn define_rule(&self, _name: &str, _body: RuleBody) {
        // No-op: DdStore takes rules at construction.
    }
    fn select(&self, name: &str) -> BoxStream<'static, Diff> {
        let tx = self.rule_outputs.write().unwrap()
            .entry(name.to_string())
            .or_insert_with(|| broadcast::channel(1024).0).clone();
        let rx = tx.subscribe();
        Box::pin(async_stream::stream! {
            let mut rx = rx;
            while let Ok(diff) = rx.recv().await { yield diff; }
        })
    }
    async fn snapshot(&self, name: &str) -> Vec<Cursor> {
        let state = self.rule_state.lock().unwrap();
        let Some(rule) = state.get(name) else { return vec![] };
        rule.iter()
            .filter(|(_, &mult)| mult > 0)
            .map(|(row, _)| dd_to_cursor(row, &self.interner))
            .collect()
    }
    async fn read_in(
        &self,
        _fact: &str,
        _key_col: &str,
        _key_values: Vec<String>,
        _project: Vec<String>,
    ) -> Vec<Cursor> {
        // Parked. DdStore is no longer on the active read path.
        vec![]
    }
}

impl Drop for DdStore {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(DdCmd::Stop);
        if let Some(h) = self.handle.lock().unwrap().take() { let _ = h.join(); }
    }
}

// ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒
// ▒  § 3c  SqliteStore — write-through fact store, brute-force IN()    ▒
// ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒
//
// `:memory:` first to lock the trait shape without filesystem ceremony.
// One Mutex<Connection> for the POC; multi-conn pool comes when
// benchmarks force it. Schema is per-fact, declared via ensure_schema.
// Writes accumulate in an in-memory pending Vec per fact, flushed via
// commit() inside one transaction with a prepared INSERT. Reads are
// brute-force per-batch SELECT WHERE key_col IN (?, ?, …) with the
// projection columns chosen by the caller.

pub struct SqliteStore {
    conn:    Mutex<rusqlite::Connection>,
    schemas: std::sync::RwLock<HashMap<String, Vec<String>>>,
    pending: Mutex<HashMap<String, Vec<Cursor>>>,
    tele:    OnceLock<Telemetry>,
}

impl SqliteStore {
    pub fn open_memory() -> Arc<Self> {
        let conn = rusqlite::Connection::open_in_memory()
            .expect("open_in_memory");
        Arc::new(Self {
            conn:    Mutex::new(conn),
            schemas: Default::default(),
            pending: Mutex::new(HashMap::new()),
            tele:    OnceLock::new(),
        })
    }
    pub fn attach_tele(&self, t: Telemetry) { let _ = self.tele.set(t); }
    fn span(&self, name: &'static str, n_in: Option<u64>) -> Option<SpanOpen> {
        self.tele.get().map(|t| t.start(name, n_in))
    }
}

#[async_trait::async_trait]
impl Store for SqliteStore {
    async fn insert(&self, fact: &str, row: Cursor, gen: Gen) {
        self.insert_many(fact, vec![row], gen).await
    }
    async fn insert_many(&self, fact: &str, rows: Vec<Cursor>, _gen: Gen) {
        if rows.is_empty() { return; }
        let n = rows.len() as u64;
        let mut sp = self.span("v4::Sqlite::ins", Some(n));
        if let Some(s) = sp.as_mut() {
            let bytes: u64 = rows.iter().map(cursor_bytes).sum();
            s.set_bytes(bytes);
        }
        let mut pending = self.pending.lock().unwrap();
        pending.entry(fact.to_string()).or_default().extend(rows);
        if let Some(s) = sp { s.close(Some(n)); }
    }
    async fn commit(&self, _gen: Gen) {
        let sp = self.span("v4::Sqlite::commit", None);
        let to_flush: HashMap<String, Vec<Cursor>> = {
            let mut p = self.pending.lock().unwrap();
            std::mem::take(&mut *p)
        };
        let schemas = self.schemas.read().unwrap().clone();
        let mut conn = self.conn.lock().unwrap();
        let txn = conn.transaction().expect("begin txn");
        let mut total: u64 = 0;
        for (fact, rows) in &to_flush {
            let Some(cols) = schemas.get(fact) else { continue };
            // Prepared INSERT: INSERT INTO fact (c1, c2, ...) VALUES (?, ?, ...)
            let placeholders = cols.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            let col_list = cols.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(", ");
            let sql = format!("INSERT INTO \"{}\" ({}) VALUES ({})", fact, col_list, placeholders);
            let mut stmt = txn.prepare_cached(&sql).expect("prepare insert");
            for row in rows {
                let vals: Vec<String> = cols.iter()
                    .map(|c| row.get(c).unwrap_or("").to_string()).collect();
                let params: Vec<&dyn rusqlite::ToSql> = vals.iter()
                    .map(|v| v as &dyn rusqlite::ToSql).collect();
                stmt.execute(params.as_slice()).expect("exec insert");
                total += 1;
            }
        }
        txn.commit().expect("commit txn");
        if let Some(s) = sp { s.close(Some(total)); }
    }
    async fn remove(&self, _fact: &str, _row: Cursor, _gen: Gen) {}
    async fn forget_by(&self, _fact: &str, _k: &str, _v: &str, _gen: Gen) {}
    fn define_rule(&self, _name: &str, _body: RuleBody) {
        // POC: rules are run by the driver, not stored here.
    }
    fn select(&self, _name: &str) -> BoxStream<'static, Diff> {
        Box::pin(async_stream::stream! {
            // POC: SqliteStore doesn't push diffs; reads are pull-only via read_in / snapshot.
            if false { yield Diff { row: Cursor::default(), gen: 0, sign: 1 }; }
        })
    }
    async fn snapshot(&self, fact: &str) -> Vec<Cursor> {
        let cols = self.schemas.read().unwrap().get(fact).cloned();
        let Some(cols) = cols else { return vec![] };
        let conn = self.conn.lock().unwrap();
        let col_list = cols.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(", ");
        let sql = format!("SELECT {} FROM \"{}\"", col_list, fact);
        let mut stmt = match conn.prepare(&sql) { Ok(s) => s, Err(_) => return vec![] };
        let cols_owned = cols.clone();
        let rows = stmt.query_map([], move |row| {
            let mut c = Cursor::default();
            for (i, name) in cols_owned.iter().enumerate() {
                let v: String = row.get(i).unwrap_or_default();
                c.set(name, v);
            }
            Ok(c)
        });
        match rows {
            Ok(it) => it.filter_map(|r| r.ok()).collect(),
            Err(_) => vec![],
        }
    }
    fn ensure_schema(&self, fact: &str, cols: &[&str]) {
        {
            let r = self.schemas.read().unwrap();
            if let Some(existing) = r.get(fact) {
                debug_assert_eq!(existing.len(), cols.len(),
                    "ensure_schema: {} reschema mismatch", fact);
                return;
            }
        }
        let cols_owned: Vec<String> = cols.iter().map(|s| s.to_string()).collect();
        let col_decls = cols_owned.iter()
            .map(|c| format!("\"{}\" TEXT", c))
            .collect::<Vec<_>>().join(", ");
        let sql = format!("CREATE TABLE IF NOT EXISTS \"{}\" ({})", fact, col_decls);
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(&sql, []).expect("create table");
        }
        self.schemas.write().unwrap().insert(fact.to_string(), cols_owned);
    }
    async fn read_in(
        &self,
        fact: &str,
        key_col: &str,
        key_values: Vec<String>,
        project: Vec<String>,
    ) -> Vec<Cursor> {
        if key_values.is_empty() { return vec![] }
        let mut sp = self.span("v4::Sqlite::read_in", Some(key_values.len() as u64));
        let placeholders = (0..key_values.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        // Always include key_col in the projection so caller can re-key.
        let mut full_proj: Vec<String> = vec![key_col.to_string()];
        for c in &project { if c != key_col { full_proj.push(c.clone()); } }
        let col_list = full_proj.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT {} FROM \"{}\" WHERE \"{}\" IN ({})",
            col_list, fact, key_col, placeholders
        );
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare_cached(&sql) {
            Ok(s) => s,
            Err(_) => { if let Some(s) = sp.take() { s.close(Some(0)); } return vec![] }
        };
        let params: Vec<&dyn rusqlite::ToSql> = key_values.iter()
            .map(|v| v as &dyn rusqlite::ToSql).collect();
        let proj_for_map = full_proj.clone();
        let rows = stmt.query_map(params.as_slice(), move |row| {
            let mut c = Cursor::default();
            for (i, name) in proj_for_map.iter().enumerate() {
                let v: String = row.get(i).unwrap_or_default();
                c.set(name, v);
            }
            Ok(c)
        });
        let out: Vec<Cursor> = match rows {
            Ok(it) => it.filter_map(|r| r.ok()).collect(),
            Err(_) => vec![],
        };
        if let Some(s) = sp { s.close(Some(out.len() as u64)); }
        out
    }
}

// ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐
// ╳    § 4   Hooks                                                  ╳
// └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘

#[derive(Clone)]
pub struct Hooks {
    pub store:    Arc<dyn Store>,
    pub effects:  mpsc::UnboundedSender<Effect>,
    pub gen:      Gen,
    pub lineage:  LineageId,
    pub tele:     Telemetry,
    pub interner: Arc<Interner>,
}

impl Hooks {
    pub fn use_store(&self) -> &Arc<dyn Store> { &self.store }
    pub fn use_dispatch_effect(&self, e: Effect) { let _ = self.effects.send(e); }
    pub fn use_gen(&self) -> Gen { self.gen }
    pub fn use_tele(&self) -> &Telemetry { &self.tele }
    pub fn use_interner(&self) -> &Arc<Interner> { &self.interner }
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
    /// Pure ops: same input lineage → same output. Cacheable.
    /// Sources (Fs, FactRead) and sinks (FactWrite) and effectful ops
    /// (sh, http) override to false. Default false (assume impure).
    fn is_pure(&self) -> bool { false }
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
        let interner = h.interner.clone();
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
                c.set_arc("FS", interner.intern(&p.display().to_string()));
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
    /// Pure: output cursors are determined by (input cursor's FS) +
    /// (file content) + (pattern). The CONTENT_HASH stamp on every
    /// emitted cursor lets downstream caches detect "same input, skip."
    fn is_pure(&self) -> bool { true }
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
                let mut span = tele.start("v4::AstNm", Some(n_in));
                let pat   = pat.clone();
                let fixed = fixed.clone();
                let cap_names = me.capture_names.clone();
                let lang  = me.lang;
                let want_match = me.want_match;
                let dbg_seen   = dbg_seen.clone();
                let dbg_passed = dbg_passed.clone();
                let dbg_hit    = dbg_hit.clone();
                // Per-batch atomic accumulators populated inside par_iter.
                let bytes_acc = Arc::new(AtomicU64::new(0));
                let parse_acc = Arc::new(AtomicU64::new(0));
                let match_acc = Arc::new(AtomicU64::new(0));
                let bytes_a = bytes_acc.clone();
                let parse_a = parse_acc.clone();
                let interner_for_par = h.interner.clone();
                let match_a = match_acc.clone();
                let out: Vec<Cursor> = tokio::task::spawn_blocking(move || {
                    batch.par_iter().flat_map(|c| {
                        // [perf-probe] dbg_seen.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = c.get("FS") else { return vec![] };
                        // Skip UTF-8 validation. tree-sitter parsers accept any byte
                        // sequence; mis-encoded bytes already turn into ERROR nodes.
                        // Validating once with `read_to_string` would walk the whole
                        // 1.4 GB of source for nothing.
                        let Ok(bytes) = std::fs::read(path) else { return vec![] };
                        bytes_a.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                        // CONTENT_HASH: blake3 of file bytes, hex-encoded,
                        // interned so all cursors emitted from this file
                        // share one Arc<str>. Anchor for downstream caches
                        // (same content → same lineage → skippable).
                        let content_hash = blake3::hash(&bytes).to_hex().to_string();
                        let content_hash_arc: Arc<str> = interner_for_par.intern(&content_hash);
                        let src: String = unsafe { String::from_utf8_unchecked(bytes) };
                        if !fixed.is_empty() && !src.contains(&*fixed) { return vec![]; }
                        // [perf-probe] dbg_passed.fetch_add(1, Ordering::Relaxed);
                        let t_parse = Instant::now();
                        let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(&src);
                        parse_a.fetch_add(t_parse.elapsed().as_nanos() as u64, Ordering::Relaxed);
                        let t_match = Instant::now();
                        let hits = grep.root().find_all(&*pat).map(|nm| {
                            let env = nm.get_env();
                            let r = nm.range();
                            let mut child = c.clone();
                            child.set_arc("CONTENT_HASH", content_hash_arc.clone());
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
                        match_a.fetch_add(t_match.elapsed().as_nanos() as u64, Ordering::Relaxed);
                        // [perf-probe] if !hits.is_empty() { dbg_hit.fetch_add(hits.len() as u64, Ordering::Relaxed); }
                        hits
                    }).collect()
                }).await.unwrap_or_default();
                let n_out = out.len() as u64;
                span.set_bytes(bytes_acc.load(Ordering::Relaxed));
                span.set_parse_ns(parse_acc.load(Ordering::Relaxed));
                span.set_match_ns(match_acc.load(Ordering::Relaxed));
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

/// Multi-pattern AstNm: parses each file once, then runs all `patterns`
/// against the same `AstGrep<StrDoc>`. The "share-the-parse" lever.
/// Each match cursor gets a `PAT` term identifying which pattern fired.
/// Prefilter: file is read+parsed if ANY pattern's fixed_string is present
/// (or any pattern has empty fixed_string).
///
/// `out_chunk_files` controls back-pressure: rather than accumulate every
/// match for the whole input batch into one Vec (RSS blowup on patterns
/// that match millions of nodes), the op processes the batch in chunks
/// of this many files at a time and ships each chunk's matches out
/// immediately via mpsc. Default 256.
pub struct MultiAstNm {
    pub patterns: Vec<(String, String)>,  // (label, pattern_src)
    pub lang:     SupportLang,
    pub out_chunk_files: usize,
}

impl MultiAstNm {
    pub fn new(patterns: Vec<(String, String)>, lang: SupportLang) -> Self {
        Self { patterns, lang, out_chunk_files: 256 }
    }
    pub fn with_out_chunk(mut self, n: usize) -> Self { self.out_chunk_files = n; self }
}

impl Op for MultiAstNm {
    fn ident(&self) -> [u8; 32] {
        let lang = format!("{:?}", self.lang);
        let mut bits: Vec<&[u8]> = vec![b"multi_ast", lang.as_bytes()];
        for (l, p) in &self.patterns { bits.push(l.as_bytes()); bits.push(p.as_bytes()); }
        ident_of(&bits)
    }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        // Compile once per run; share via Arc.
        let pats: Arc<Vec<(Arc<str>, Arc<Pattern<SupportLang>>, Arc<str>)>> = Arc::new(
            me.patterns.iter().map(|(label, src)| {
                let p = Pattern::new(src, me.lang);
                let fx: Arc<str> = Arc::from(p.fixed_string().to_string().as_str());
                (Arc::<str>::from(label.as_str()), Arc::new(p), fx)
            }).collect()
        );
        let chunk_files = me.out_chunk_files.max(1);
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n_in = batch.len() as u64;
                let mut span = tele.start("v4::MultiAstNm", Some(n_in));
                let pats = pats.clone();
                let lang = me.lang;
                let (tx, mut rx) = mpsc::channel::<Vec<Cursor>>(4);
                // Per-batch instrumentation. Atomics are written from rayon
                // workers (Relaxed is fine — read once at end-of-batch from
                // the same thread that joined spawn_blocking).
                let bytes_acc = Arc::new(AtomicU64::new(0));
                let parse_acc = Arc::new(AtomicU64::new(0));
                let match_acc = Arc::new(AtomicU64::new(0));
                let bytes_a = bytes_acc.clone();
                let parse_a = parse_acc.clone();
                let match_a = match_acc.clone();
                let chunk_join = tokio::task::spawn_blocking(move || {
                    for chunk in batch.chunks(chunk_files) {
                        let sub: Vec<Cursor> = chunk.par_iter().flat_map(|c| {
                            let Some(path) = c.get("FS") else { return vec![] };
                            let Ok(bytes) = std::fs::read(path) else { return vec![] };
                            bytes_a.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                            let src: String = unsafe { String::from_utf8_unchecked(bytes) };
                            let any_fires = pats.iter().any(|(_, _, fx)| fx.is_empty() || src.contains(&**fx));
                            if !any_fires { return vec![]; }
                            let t_parse = Instant::now();
                            let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(&src);
                            parse_a.fetch_add(t_parse.elapsed().as_nanos() as u64, Ordering::Relaxed);
                            let t_match = Instant::now();
                            let mut out: Vec<Cursor> = Vec::new();
                            for (label, pat, fx) in pats.iter() {
                                if !fx.is_empty() && !src.contains(&**fx) { continue; }
                                for nm in grep.root().find_all(&**pat) {
                                    let r = nm.range();
                                    let mut child = c.clone();
                                    child.set_arc("PAT", label.clone());
                                    child.set("LO",  (r.start as u64).to_string());
                                    child.set("HI",  (r.end   as u64).to_string());
                                    out.push(child);
                                }
                            }
                            match_a.fetch_add(t_match.elapsed().as_nanos() as u64, Ordering::Relaxed);
                            out
                        }).collect();
                        if !sub.is_empty() {
                            if tx.blocking_send(sub).is_err() { return; }
                        }
                    }
                });
                let mut total_out: u64 = 0;
                while let Some(sub) = rx.recv().await {
                    total_out += sub.len() as u64;
                    yield sub;
                }
                let _ = chunk_join.await;
                span.set_bytes(bytes_acc.load(Ordering::Relaxed));
                span.set_parse_ns(parse_acc.load(Ordering::Relaxed));
                span.set_match_ns(match_acc.load(Ordering::Relaxed));
                span.close(Some(total_out));
            }
        })
    }
}

/// Insert each upstream batch into the named fact. Pass-through.
/// Legacy: writes whatever terms the cursor carries — works for MemStore /
/// DdStore which accept arbitrary Cursor shapes; SqliteStore needs a
/// declared column set, use `FactWrite` for that.
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

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   FactWrite — schema-aware sink. Calls ensure_schema on first batch.
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
pub struct FactWrite {
    pub name: String,
    pub cols: Vec<String>,
}
impl FactWrite {
    pub fn new(name: impl Into<String>, cols: &[&str]) -> Self {
        Self { name: name.into(), cols: cols.iter().map(|s| s.to_string()).collect() }
    }
}
impl Op for FactWrite {
    fn ident(&self) -> [u8; 32] {
        let mut bits: Vec<&[u8]> = vec![b"fact_write", self.name.as_bytes()];
        for c in &self.cols { bits.push(c.as_bytes()); }
        ident_of(&bits)
    }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        Box::pin(async_stream::stream! {
            // Schema decl is idempotent + sync; safe to do at op start.
            let cols_ref: Vec<&str> = me.cols.iter().map(|s| s.as_str()).collect();
            h.use_store().ensure_schema(&me.name, &cols_ref);
            while let Some(batch) = input.next().await {
                let n = batch.len() as u64;
                let span = tele.start("v4::FactWrite", Some(n));
                h.use_store().insert_many(&me.name, batch.clone(), h.use_gen()).await;
                span.close(Some(n));
                yield batch;
            }
        })
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   FactRead — brute-force IN-batch fact-read with bound/unbound terms.
//
//   key_term : term name that MUST be present on every input cursor
//              (the bound side, drives WHERE col IN (?, ?, ...))
//   project  : term names to fetch from the fact's row and set on each
//              output cursor (the unbound side)
//
//   Per input batch:
//     1. collect distinct key_term values
//     2. ONE read_in() call → rows
//     3. group rows by key_term value
//     4. for each input cursor, emit one output per matching row
//        (cross-product cursor × matching rows)
//
//   N input cursors with M avg matches each → 1 query, N × M outputs.
//   The N+1 query failure mode (one query per input cursor) does not
//   exist by construction.
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
pub struct FactRead {
    pub fact:     String,
    pub key_term: String,
    pub project:  Vec<String>,
}
impl FactRead {
    pub fn new(fact: impl Into<String>, key_term: impl Into<String>, project: &[&str]) -> Self {
        Self {
            fact: fact.into(),
            key_term: key_term.into(),
            project: project.iter().map(|s| s.to_string()).collect(),
        }
    }
}
impl Op for FactRead {
    fn ident(&self) -> [u8; 32] {
        let mut bits: Vec<&[u8]> = vec![b"fact_read", self.fact.as_bytes(), self.key_term.as_bytes()];
        for c in &self.project { bits.push(c.as_bytes()); }
        ident_of(&bits)
    }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n_in = batch.len() as u64;
                let span = tele.start("v4::FactRead", Some(n_in));
                // 1. collect distinct keys from the batch
                let mut keys: Vec<String> = Vec::with_capacity(batch.len());
                let mut seen: HashSet<String> = HashSet::new();
                for c in &batch {
                    if let Some(v) = c.get(&me.key_term) {
                        if seen.insert(v.to_string()) { keys.push(v.to_string()); }
                    }
                }
                // 2. one query
                let rows = h.use_store().read_in(
                    &me.fact, &me.key_term, keys, me.project.clone(),
                ).await;
                // 3. group by key
                let mut by_key: HashMap<String, Vec<Cursor>> = HashMap::new();
                for r in rows {
                    if let Some(k) = r.get(&me.key_term) {
                        by_key.entry(k.to_string()).or_default().push(r);
                    }
                }
                // 4. cross-product into output
                let mut out: Vec<Cursor> = Vec::with_capacity(batch.len());
                for cursor in &batch {
                    let Some(k) = cursor.get(&me.key_term) else { continue };
                    let Some(matches) = by_key.get(k) else { continue };
                    for m in matches {
                        let mut child = cursor.clone();
                        for col in &me.project {
                            if let Some(v) = m.get(col) { child.set(col, v); }
                        }
                        out.push(child);
                    }
                }
                let n_out = out.len() as u64;
                span.close(Some(n_out));
                if !out.is_empty() { yield out; }
            }
        })
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   Rule — a callable, parametric pipeline whose output sinks to a fact.
//
//   Replaces the RuleBody enum's role for sqlite-shaped workloads.
//   `body` is the streaming op chain that produces cursors. `sink` is
//   the FactWrite step at the end (named so the runner can ensure_schema
//   before drain). RuleBody enum stays for MemStore/DdStore until the
//   unification refactor lands.
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
#[derive(Clone)]
pub struct Rule {
    pub name: String,
    pub body: Vec<Arc<dyn Op>>,
    pub sink: RuleSink,
}
#[derive(Clone)]
pub struct RuleSink {
    pub fact: String,
    pub cols: Vec<String>,
}
impl Rule {
    pub fn new(name: impl Into<String>, body: Vec<Arc<dyn Op>>, sink_fact: impl Into<String>, sink_cols: &[&str]) -> Self {
        Self {
            name: name.into(),
            body,
            sink: RuleSink {
                fact: sink_fact.into(),
                cols: sink_cols.iter().map(|s| s.to_string()).collect(),
            },
        }
    }
    /// Append a FactWrite step targeting the rule's sink. Returns the
    /// full chain ready for drive_with.
    pub fn into_chain(self) -> Vec<Arc<dyn Op>> {
        let cols_ref: Vec<&str> = self.sink.cols.iter().map(|s| s.as_str()).collect();
        let mut chain = self.body;
        chain.push(Arc::new(FactWrite::new(self.sink.fact, &cols_ref)));
        chain
    }
}

/// Pass-through op that calls `store.commit(gen)` every `every` batches.
/// Used to stress-test rule rederive cost under reactive (LSP-shaped)
/// workloads where commits happen many times per run instead of once.
pub struct CommitEvery { pub every: usize, pub counter: AtomicU64 }
impl CommitEvery {
    pub fn new(every: usize) -> Self {
        Self { every, counter: AtomicU64::new(0) }
    }
}
impl Op for CommitEvery {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"commit_every"]) }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n = me.counter.fetch_add(1, Ordering::Relaxed) + 1;
                if me.every > 0 && (n as usize) % me.every == 0 {
                    h.use_store().commit(h.use_gen()).await;
                }
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

/// Two-tick driver: render (drive_with drains) → commit (store flushes).
/// React mental model: tick 1 is render, tick 2 is commit. CI mode runs
/// this once and exits. Reactive mode wraps it in a switchMap event loop
/// that aborts in-flight tick 1 work when fresh invalidations arrive.
///
/// Tick 1: pipeline drains end-to-end. FactWrite buffers writes into the
/// store's pending area. FactRead sees the store as it stood entering
/// the tick (no half-applied writes from this run).
///
/// Tick 2: store.commit(gen) flushes pending writes inside one txn,
/// fsyncs, broadcasts diffs to any subscribers.
pub async fn drive_two_tick(chain: Vec<Arc<dyn Op>>, h: Hooks, cap: usize) {
    drive_with(chain, h.clone(), cap).await;
    h.use_store().commit(h.gen).await;
}
