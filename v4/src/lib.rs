pub mod chan;
pub mod cst;
pub mod cursor_codec;
pub mod fact;
pub mod rule;
pub mod runtime_bridge;
pub mod substrate_ops;
pub mod term;

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
    /// On-disk constructor. Sets WAL journal mode for concurrent readers.
    pub fn open_path(p: impl AsRef<std::path::Path>) -> Arc<Self> {
        if let Some(parent) = p.as_ref().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = rusqlite::Connection::open(p.as_ref()).expect("open sqlite");
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        Arc::new(Self {
            conn:    Mutex::new(conn),
            schemas: Default::default(),
            pending: Mutex::new(HashMap::new()),
            tele:    OnceLock::new(),
        })
    }
    /// On-disk constructor tuned for CACHE durability — no fsync, journal
    /// in memory, exclusive lock, 64MB page cache. Loses pending writes
    /// on crash; correct when sqlite is a derived cache (LSP / scanner)
    /// that can be rebuilt from the source corpus + Layer-2 hashes.
    pub fn open_path_fast(p: impl AsRef<std::path::Path>) -> Arc<Self> {
        if let Some(parent) = p.as_ref().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = rusqlite::Connection::open(p.as_ref()).expect("open sqlite");
        // Apply BEFORE any DDL. page_size must be set before first write.
        let _ = conn.pragma_update(None, "page_size", 8192_i64);
        let _ = conn.pragma_update(None, "journal_mode", "MEMORY");
        let _ = conn.pragma_update(None, "synchronous", "OFF");
        let _ = conn.pragma_update(None, "locking_mode", "EXCLUSIVE");
        let _ = conn.pragma_update(None, "temp_store", "MEMORY");
        // Negative = -KB; 65536 = 64 MB.
        let _ = conn.pragma_update(None, "cache_size", -65536_i64);
        Arc::new(Self {
            conn:    Mutex::new(conn),
            schemas: Default::default(),
            pending: Mutex::new(HashMap::new()),
            tele:    OnceLock::new(),
        })
    }
    /// Default on-disk path: $HOME/.cache/sprefa/v4.db.
    pub fn open_default() -> Arc<Self> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let path = std::path::PathBuf::from(home).join(".cache/sprefa/v4.db");
        Self::open_path(path)
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
        // Spans by phase. Lets us see if commit cost is lock contention on
        // the connection, txn-begin overhead, the chunked-execute loop,
        // or txn-commit (where the WAL fsync lives).
        let sp_lock = self.span("v4::Sqlite::commit::lock", None);
        let mut conn = self.conn.lock().unwrap();
        if let Some(s) = sp_lock { s.close(None); }

        let sp_begin = self.span("v4::Sqlite::commit::begin", None);
        let txn = conn.transaction().expect("begin txn");
        if let Some(s) = sp_begin { s.close(None); }

        let mut total: u64 = 0;
        // Chunk size knob. sqlite default SQLITE_MAX_VARIABLE_NUMBER = 32766;
        // chunk * ncols must stay under it. SPREFA_SQLITE_CHUNK env var lets
        // perf labs sweep the knob without rebuilding. 500 was the win on
        // 4-col TEXT writes at 366k rows in baseline lab.
        let chunk_size: usize = std::env::var("SPREFA_SQLITE_CHUNK")
            .ok().and_then(|v| v.parse().ok()).unwrap_or(500);
        for (fact, rows) in &to_flush {
            let Some(cols) = schemas.get(fact) else { continue };
            let n_rows = rows.len() as u64;
            let mut sp_fact = self.span("v4::Sqlite::commit::write", Some(n_rows));
            let mut bytes_total: u64 = 0;
            let ncols = cols.len();
            let max_chunk = if ncols == 0 { chunk_size }
                            else { (32000 / ncols).max(1).min(chunk_size) };
            let col_list = cols.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(", ");
            let single = "(".to_string() + &cols.iter().map(|_| "?").collect::<Vec<_>>().join(",") + ")";

            for chunk in rows.chunks(max_chunk) {
                let values_clause = vec![single.as_str(); chunk.len()].join(",");
                let sql = format!("INSERT INTO \"{}\" ({}) VALUES {}", fact, col_list, values_clause);
                let mut stmt = txn.prepare_cached(&sql).expect("prepare insert");
                // Bind Option<&str> directly. Cursor::get returns &str into
                // an Arc<str> backing — no String allocation per cell. At
                // 366k rows × 4 cols that's 1.5M allocations avoided.
                let mut vals: Vec<Option<&str>> = Vec::with_capacity(chunk.len() * ncols);
                for row in chunk {
                    for c in cols {
                        let v = row.get(c);
                        if let Some(s) = v { bytes_total += s.len() as u64; }
                        vals.push(v);
                    }
                }
                let params: Vec<&dyn rusqlite::ToSql> = vals.iter()
                    .map(|v| v as &dyn rusqlite::ToSql).collect();
                stmt.execute(params.as_slice()).expect("exec insert");
                total += chunk.len() as u64;
            }
            if let Some(s) = sp_fact.as_mut() { s.set_bytes(bytes_total); }
            if let Some(s) = sp_fact { s.close(Some(n_rows)); }
        }
        let sp_commit = self.span("v4::Sqlite::commit::txn", None);
        txn.commit().expect("commit txn");
        if let Some(s) = sp_commit { s.close(None); }
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
                // NULL → term not set on cursor (preserves "missing").
                let v: Option<String> = row.get(i).ok().flatten();
                if let Some(v) = v { c.set(name, v); }
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
                let v: Option<String> = row.get(i).ok().flatten();
                if let Some(v) = v { c.set(name, v); }
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
    /// Layer-2 probe: source ops describe the (path, content_hash) set
    /// they would emit, without running the rest of the pipeline.
    /// Returned vector is sorted by path before hashing in
    /// Rule::input_set_hash. Default None = "not a probeable source".
    fn probe(&self) -> Option<Vec<(String, [u8; 32])>> { None }
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
    fn probe(&self) -> Option<Vec<(String, [u8; 32])>> {
        let mut out: Vec<(String, [u8; 32])> = Vec::new();
        for entry in WalkBuilder::new(&self.root).hidden(true).git_ignore(false).build() {
            let Ok(e) = entry else { continue };
            if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
            let p = e.into_path();
            let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
            if !self.exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }
            let Ok(bytes) = std::fs::read(&p) else { continue };
            let h = *blake3::hash(&bytes).as_bytes();
            out.push((p.display().to_string(), h));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Some(out)
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

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 7b  Repo — multi-root cross-repo source
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// One cursor per file across N labelled roots. Each cursor carries:
//   REPO = slug (caller-supplied)
//   FS   = absolute file path (downstream ops read bytes from this)
// Wraps the same WalkBuilder shape as Fs; just multiplexed across roots
// and tagged with REPO. Walking happens sequentially per root inside one
// spawn_blocking; per-root parallelism is left to upstream batching of
// the chained matcher (AstNm/Re par_iter on each batch).
//
// Probe: walks every root, blake3-hashes every matching file, returns
// sorted ("{slug}::{path}", hash) entries. Slug-prefix prevents same-name
// files in different repos from colliding in the Layer-2 input-set hash.
pub struct Repo {
    pub roots: Vec<(String, PathBuf)>,
    pub exts:  Vec<String>,
    pub batch: usize,
    pub cap:   usize,
}
impl Repo {
    pub fn new(roots: Vec<(String, PathBuf)>, exts: Vec<String>) -> Self {
        Self { roots, exts, batch: BATCH, cap: 8 }
    }
    /// Auto-slug each root by its basename. Convenience for symmetric
    /// multi-root walks where the caller doesn't care about labels.
    pub fn auto(roots: &[PathBuf], exts: Vec<String>) -> Self {
        let labelled: Vec<(String, PathBuf)> = roots.iter().map(|p| {
            let slug = p.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p.display().to_string());
            (slug, p.clone())
        }).collect();
        Self::new(labelled, exts)
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 7b0  Seed — emit a single fixed batch of cursors, ignore input
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Useful as a chain head when the first real op (e.g., Sh::emit_lines)
// is per-cursor but you want it to run once. `Seed::one()` ships a
// single empty cursor; arbitrary seeds via `Seed::new(rows)`.
pub struct Seed { pub rows: Vec<Cursor> }
impl Seed {
    pub fn one() -> Self { Self { rows: vec![Cursor::default()] } }
    pub fn new(rows: Vec<Cursor>) -> Self { Self { rows } }
}
impl Op for Seed {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"seed"]) }
    fn run(self: Arc<Self>, _h: Hooks, _i: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let rows = self.rows.clone();
        Box::pin(async_stream::stream! { yield rows; })
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 7b'  RepoFromTerm — value-driven cross-repo source (transformer)
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Per input cursor, treats cursor.get(term) as a repo root. Walks it,
// emits REPO+FS cursors for each matching file, preserving all other
// terms from the input cursor (so upstream context flows through).
//
// REPO = basename of the root path. FS = absolute file path.
//
// is_pure: false (source-shaped; reads the filesystem).
pub struct RepoFromTerm {
    pub term:  String,
    pub exts:  Vec<String>,
    pub batch: usize,
}
impl RepoFromTerm {
    pub fn new(term: impl Into<String>, exts: Vec<String>) -> Self {
        Self { term: term.into(), exts, batch: BATCH }
    }
}
impl Op for RepoFromTerm {
    fn ident(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"repo_from_term|");
        h.update(self.term.as_bytes()); h.update(b"|");
        for e in &self.exts { h.update(e.as_bytes()); h.update(b","); }
        *h.finalize().as_bytes()
    }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        let interner = h.interner.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n_in = batch.len() as u64;
                let span = tele.start("v4::RepoFromTerm", Some(n_in));
                let me_ = me.clone();
                let interner_ = interner.clone();
                let out: Vec<Cursor> = tokio::task::spawn_blocking(move || {
                    let mut all: Vec<Cursor> = Vec::new();
                    for parent in batch {
                        let Some(root_str) = parent.get(&me_.term) else { continue };
                        let root = PathBuf::from(root_str);
                        let slug = root.file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| root.display().to_string());
                        let slug_arc: Arc<str> = interner_.intern(&slug);
                        for entry in WalkBuilder::new(&root).hidden(true).git_ignore(false).build() {
                            let Ok(e) = entry else { continue };
                            if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
                            let p = e.into_path();
                            let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
                            if !me_.exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }
                            let mut c = parent.clone();
                            c.set_arc("REPO", slug_arc.clone());
                            c.set_arc("FS",   interner_.intern(&p.display().to_string()));
                            all.push(c);
                        }
                    }
                    all
                }).await.unwrap_or_default();
                let n_out = out.len() as u64;
                span.close(Some(n_out));
                if !out.is_empty() { yield out; }
            }
        })
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 7b''  Line — explode a term's value into one cursor per line
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Reads cursor.get(source). Splits on '\n'. Empty lines dropped.
// Each non-empty line emitted as a fresh cursor with all original terms
// preserved + target=line. Default target = source (line replaces value).
// is_pure: true.
pub struct Line {
    pub source: String,
    pub target: String,
}
impl Line {
    pub fn on(term: impl Into<String>) -> Self {
        let s = term.into();
        Self { target: s.clone(), source: s }
    }
    pub fn into_term(mut self, target: impl Into<String>) -> Self {
        self.target = target.into(); self
    }
}
impl Op for Line {
    fn ident(&self) -> [u8; 32] {
        ident_of(&[b"line", self.source.as_bytes(), b"->", self.target.as_bytes()])
    }
    fn is_pure(&self) -> bool { true }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n_in = batch.len() as u64;
                let span = tele.start("v4::Line", Some(n_in));
                let mut out = Vec::with_capacity(batch.len());
                for c in batch {
                    let Some(text) = c.get(&me.source).map(|s| s.to_string()) else { continue };
                    for line in text.split('\n') {
                        if line.is_empty() { continue; }
                        let mut child = c.clone();
                        child.set(&me.target, line);
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
//   § 7c'  ChannelHub + Next / NextQ — ephemeral pub/sub between rules
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Process-wide channel hub. One `tokio::sync::broadcast::Sender` per
// channel name, lazily created. Channels survive across drives so a
// producer rule and a consumer rule (separate drive_with calls) can
// share state.
//
//   next(:chan)        — Next op:  send each input cursor on `chan`,
//                        forward downstream unchanged.
//   next?(:chan, take) — NextQ op: source-shaped, subscribes and emits
//                        cursors as they arrive. `take` is the count
//                        cap (None = unbounded; will block at end of
//                        the producer's life unless `close(chan)` is
//                        called).
//
// Lock detection: not implemented. Workarounds —
//   - send is buffer-bounded (broadcast buf=1024). Lagging recv sees
//     RecvError::Lagged; we skip and continue (telemetry counter).
//   - recv ends when all senders drop OR the hub explicitly closes the
//     channel. Provide `Close { chan }` op and `ChannelHub::close(name)`
//     for explicit termination.
pub struct ChannelHub {
    senders: std::sync::RwLock<HashMap<String, tokio::sync::broadcast::Sender<Cursor>>>,
    buf:     usize,
}
impl ChannelHub {
    pub fn new() -> Self { Self { senders: Default::default(), buf: 1024 } }
    fn ensure(&self, name: &str) -> tokio::sync::broadcast::Sender<Cursor> {
        {
            let r = self.senders.read().unwrap();
            if let Some(s) = r.get(name) { return s.clone(); }
        }
        let mut w = self.senders.write().unwrap();
        if let Some(s) = w.get(name) { return s.clone(); }
        let (tx, _rx) = tokio::sync::broadcast::channel::<Cursor>(self.buf);
        w.insert(name.to_string(), tx.clone());
        tx
    }
    pub fn send(&self, name: &str, cur: Cursor) {
        // Returns Err if no subscribers; broadcast still buffers, so
        // late subscribers get historical events up to buf depth.
        let _ = self.ensure(name).send(cur);
    }
    pub fn subscribe(&self, name: &str) -> tokio::sync::broadcast::Receiver<Cursor> {
        self.ensure(name).subscribe()
    }
    pub fn close(&self, name: &str) {
        self.senders.write().unwrap().remove(name);
    }
    pub fn subscribers(&self, name: &str) -> usize {
        self.senders.read().unwrap().get(name).map(|s| s.receiver_count()).unwrap_or(0)
    }
}
static CHANNELS: OnceLock<ChannelHub> = OnceLock::new();
pub fn channels() -> &'static ChannelHub {
    CHANNELS.get_or_init(ChannelHub::new)
}

pub struct Next { pub chan: String }
impl Next {
    pub fn new(chan: impl Into<String>) -> Self { Self { chan: chan.into() } }
}
impl Op for Next {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"next", self.chan.as_bytes()]) }
    fn is_pure(&self) -> bool { false }  // observable side-effect
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n = batch.len() as u64;
                let span = tele.start("v4::Next", Some(n));
                for c in &batch { channels().send(&me.chan, c.clone()); }
                span.close(Some(n));
                yield batch;
            }
        })
    }
}

pub struct NextQ {
    pub chan: String,
    pub take: Option<usize>,  // None = unbounded
}
impl NextQ {
    pub fn new(chan: impl Into<String>) -> Self { Self { chan: chan.into(), take: None } }
    pub fn take(mut self, n: usize) -> Self { self.take = Some(n); self }
}
impl Op for NextQ {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"next?", self.chan.as_bytes()]) }
    fn is_pure(&self) -> bool { false }  // depends on external state
    fn run(self: Arc<Self>, h: Hooks, _i: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        let mut rx = channels().subscribe(&me.chan);
        Box::pin(async_stream::stream! {
            let mut emitted: usize = 0;
            loop {
                match rx.recv().await {
                    Ok(c) => {
                        let span = tele.start("v4::NextQ", Some(1));
                        emitted += 1;
                        span.close(Some(1));
                        yield vec![c];
                        if let Some(n) = me.take { if emitted >= n { break; } }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    }
}

pub struct Close { pub chan: String }
impl Close {
    pub fn new(chan: impl Into<String>) -> Self { Self { chan: chan.into() } }
}
impl Op for Close {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"close", self.chan.as_bytes()]) }
    fn is_pure(&self) -> bool { false }
    fn run(self: Arc<Self>, _h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await { yield batch; }
            channels().close(&me.chan);
        })
    }
}

impl Op for Repo {
    fn ident(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"repo|");
        for (slug, root) in &self.roots {
            h.update(slug.as_bytes()); h.update(b"=");
            h.update(root.to_string_lossy().as_bytes()); h.update(b"\n");
        }
        for e in &self.exts { h.update(e.as_bytes()); h.update(b","); }
        *h.finalize().as_bytes()
    }
    fn probe(&self) -> Option<Vec<(String, [u8; 32])>> {
        let mut out: Vec<(String, [u8; 32])> = Vec::new();
        for (slug, root) in &self.roots {
            for entry in WalkBuilder::new(root).hidden(true).git_ignore(false).build() {
                let Ok(e) = entry else { continue };
                if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
                let p = e.into_path();
                let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
                if !self.exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let h = *blake3::hash(&bytes).as_bytes();
                out.push((format!("{}::{}", slug, p.display()), h));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Some(out)
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
            for (slug, root) in &me.roots {
                let slug_arc: Arc<str> = interner.intern(slug);
                for entry in WalkBuilder::new(root).hidden(true).git_ignore(false).build() {
                    let Ok(e) = entry else { continue };
                    if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
                    let p = e.into_path();
                    let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
                    if !me.exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }
                    let mut c = Cursor::default();
                    c.set_arc("REPO", slug_arc.clone());
                    c.set_arc("FS",   interner.intern(&p.display().to_string()));
                    buf.push(c);
                    if buf.len() >= batch {
                        let span = tele.start("v4::Repo", None);
                        let n = buf.len() as u64;
                        if tx.blocking_send(std::mem::take(&mut buf)).is_err() {
                            span.close(Some(n)); return;
                        }
                        span.close(Some(n));
                        buf = Vec::with_capacity(batch);
                    }
                }
            }
            if !buf.is_empty() {
                let span = tele.start("v4::Repo", None);
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

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 7c  Re — regex matcher (grep-like)
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// One match → one output cursor. Groups bound positionally:
//   group(0) → MATCH (when want_match)
//   group(1..N) → captures[0..N-1]
// Source: when on=="FS", read file bytes at cursor.get("FS"); else use
// cursor.get(on) as the haystack text.
pub struct Re {
    pub pattern_src:   String,
    pub on:            String,
    pub capture_names: Vec<String>,
    pub want_match:    bool,
}
impl Re {
    pub fn new(pat: &str, captures: &[&str]) -> Self {
        Self {
            pattern_src:   pat.to_string(),
            on:            "FS".to_string(),
            capture_names: captures.iter().map(|s| s.to_string()).collect(),
            want_match:    true,
        }
    }
    pub fn on_term(mut self, term: &str) -> Self { self.on = term.to_string(); self }
    pub fn with_match_text(mut self, on: bool) -> Self { self.want_match = on; self }
}
impl Op for Re {
    fn ident(&self) -> [u8; 32] {
        ident_of(&[b"re", self.on.as_bytes(), self.pattern_src.as_bytes()])
    }
    fn is_pure(&self) -> bool { true }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        let interner = h.interner.clone();
        let re = Arc::new(regex::Regex::new(&me.pattern_src).expect("compile regex"));
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n_in = batch.len() as u64;
                let span = tele.start("v4::Re", Some(n_in));
                let re = re.clone();
                let me = me.clone();
                let interner = interner.clone();
                let bytes_acc = Arc::new(AtomicU64::new(0));
                let bytes_a   = bytes_acc.clone();
                let out: Vec<Cursor> = tokio::task::spawn_blocking(move || {
                    batch.par_iter().flat_map(|c| {
                        let (haystack, content_hash_arc): (String, Option<Arc<str>>) =
                            if me.on == "FS" {
                                let Some(path) = c.get("FS") else { return vec![] };
                                let Ok(bytes) = std::fs::read(path) else { return vec![] };
                                bytes_a.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                                let h_hex = blake3::hash(&bytes).to_hex().to_string();
                                let h_arc: Arc<str> = interner.intern(&h_hex);
                                let s = match String::from_utf8(bytes) {
                                    Ok(s) => s,
                                    Err(e) => unsafe { String::from_utf8_unchecked(e.into_bytes()) },
                                };
                                (s, Some(h_arc))
                            } else {
                                match c.get(&me.on) {
                                    Some(s) => (s.to_string(), None),
                                    None => return vec![],
                                }
                            };
                        let mut hits = Vec::new();
                        for caps in re.captures_iter(&haystack) {
                            let m0 = caps.get(0).unwrap();
                            let mut child = c.clone();
                            if let Some(arc) = &content_hash_arc {
                                child.set_arc("CONTENT_HASH", arc.clone());
                            }
                            child.set("LO", (m0.start() as u64).to_string());
                            child.set("HI", (m0.end()   as u64).to_string());
                            if me.want_match { child.set("MATCH", m0.as_str()); }
                            for (i, name) in me.capture_names.iter().enumerate() {
                                if let Some(g) = caps.get(i + 1) {
                                    child.set(name, g.as_str());
                                }
                            }
                            hits.push(child);
                        }
                        hits
                    }).collect()
                }).await.unwrap_or_default();
                let n_out = out.len() as u64;
                span.close(Some(n_out));
                if !out.is_empty() { yield out; }
            }
        })
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 7d  Sh / ShBang — shell ops
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Sh   = query/filter shell, AUTHOR-DECLARED PURE (cacheable). Modes:
//   EmitLines     — one cursor per stdout line, line bound to LINE term
//   FilterByExit  — exit 0 → pass cursor, nonzero → drop
//   CaptureStdout — bind trimmed stdout to `bind`, pass cursor
// ShBang (sh!) = side-effect shell, IMPURE. Runs cmd, discards stdout,
//   always passes cursor through. Never cached.
//
// `${TERM}` interpolation from the input cursor (POSIX single-quoted).
// Run via `sh -c`.
#[derive(Clone, Copy, Debug)]
pub enum ShMode { EmitLines, FilterByExit, CaptureStdout }
pub struct Sh {
    pub cmd_template: String,
    pub mode:         ShMode,
    pub bind:         Option<String>,
}
impl Sh {
    pub fn emit_lines(cmd: &str) -> Self {
        Self { cmd_template: cmd.to_string(), mode: ShMode::EmitLines, bind: None }
    }
    pub fn filter(cmd: &str) -> Self {
        Self { cmd_template: cmd.to_string(), mode: ShMode::FilterByExit, bind: None }
    }
    pub fn capture(cmd: &str, bind: &str) -> Self {
        Self { cmd_template: cmd.to_string(), mode: ShMode::CaptureStdout, bind: Some(bind.to_string()) }
    }
}
fn shell_quote(s: &str) -> String {
    // POSIX: wrap in single quotes, escape embedded single quotes as '\''.
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' { out.push_str("'\\''"); } else { out.push(ch); }
    }
    out.push('\'');
    out
}
fn render_template(template: &str, cur: &Cursor) -> String {
    // Replace ${TERM} with shell-quoted cursor.get(TERM). Missing → empty quoted.
    let re = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap();
    re.replace_all(template, |caps: &regex::Captures| {
        let name = &caps[1];
        let v = cur.get(name).unwrap_or("");
        shell_quote(v)
    }).into_owned()
}
impl Op for Sh {
    fn ident(&self) -> [u8; 32] {
        ident_of(&[b"sh", format!("{:?}", self.mode).as_bytes(), self.cmd_template.as_bytes()])
    }
    fn is_pure(&self) -> bool { true }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n_in = batch.len() as u64;
                let span = tele.start("v4::Sh", Some(n_in));
                let mut out: Vec<Cursor> = Vec::new();
                for c in batch {
                    let cmd = render_template(&me.cmd_template, &c);
                    let res = tokio::process::Command::new("sh")
                        .arg("-c").arg(&cmd)
                        .output().await;
                    let Ok(o) = res else { continue };
                    match me.mode {
                        ShMode::EmitLines => {
                            let stdout = String::from_utf8_lossy(&o.stdout);
                            for line in stdout.split('\n') {
                                if line.is_empty() { continue; }
                                let mut child = c.clone();
                                child.set("LINE", line);
                                out.push(child);
                            }
                        }
                        ShMode::FilterByExit => {
                            if o.status.success() { out.push(c); }
                        }
                        ShMode::CaptureStdout => {
                            let stdout = String::from_utf8_lossy(&o.stdout).trim_end().to_string();
                            let mut child = c;
                            if let Some(b) = &me.bind { child.set(b, stdout.as_str()); }
                            out.push(child);
                        }
                    }
                }
                let n_out = out.len() as u64;
                span.close(Some(n_out));
                if !out.is_empty() { yield out; }
            }
        })
    }
}

/// `sh!` — author-declared impure shell. Runs cmd per cursor, drops
/// stdout, always passes cursor through. Never cacheable.
pub struct ShBang { pub cmd_template: String }
impl ShBang {
    pub fn new(cmd: &str) -> Self { Self { cmd_template: cmd.to_string() } }
}
impl Op for ShBang {
    fn ident(&self) -> [u8; 32] {
        ident_of(&[b"sh!", self.cmd_template.as_bytes()])
    }
    fn is_pure(&self) -> bool { false }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n_in = batch.len() as u64;
                let span = tele.start("v4::Sh!", Some(n_in));
                let mut out: Vec<Cursor> = Vec::with_capacity(batch.len());
                for c in batch {
                    let cmd = render_template(&me.cmd_template, &c);
                    let _ = tokio::process::Command::new("sh")
                        .arg("-c").arg(&cmd)
                        .status().await;
                    out.push(c);
                }
                let n_out = out.len() as u64;
                span.close(Some(n_out));
                if !out.is_empty() { yield out; }
            }
        })
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 7s  SimpleOp — cheap op authoring
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Implementing `Op` directly costs ~30 LOC of async-stream + Hooks +
// tele plumbing. SimpleOp lets a transformation op be written as a
// 1-line `step(&Cursor) -> Vec<Cursor>`. The blanket `impl<T: SimpleOp>
// Op for T` does the rest. Use Op (not SimpleOp) when the op needs to
// own the stream shape (parallelism via spawn_blocking + rayon, batch
// reshaping, async I/O like Sh, fan-out across batches).
//
// Authoring contract:
//   kind()      — &'static str, used as ident bytes AND tele span name
//   instance()  — Optional fingerprint distinguishing two ops of same kind
//   step(&c)    — fn-of-cursor → 0..N cursors. Pure by default.
//
// Telemetry: spans use `kind()` as the span name. Counts batches as one
// span each, n_in / n_out as element counts. Same surface as the manual
// Op impls so the bench summary stays consistent.
pub trait SimpleOp: Send + Sync + 'static {
    fn kind(&self) -> &'static str;
    fn instance(&self) -> String { String::new() }
    fn step(&self, c: &Cursor) -> Vec<Cursor>;
    fn is_pure(&self) -> bool { true }
}
// Blanket: every SimpleOp is an Op. Do NOT impl Op manually for the
// same type — that would conflict with this blanket.
impl<T: SimpleOp> Op for T {
    fn ident(&self) -> [u8; 32] {
        let inst = self.instance();
        let kind = self.kind().as_bytes();
        ident_of(&[kind, b"|", inst.as_bytes()])
    }
    fn is_pure(&self) -> bool { SimpleOp::is_pure(self) }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        let span_name: &'static str = me.kind();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n_in = batch.len() as u64;
                let span = tele.start(span_name, Some(n_in));
                let mut out: Vec<Cursor> = Vec::with_capacity(batch.len());
                for c in batch {
                    out.extend(me.step(&c));
                }
                let n_out = out.len() as u64;
                span.close(Some(n_out));
                if !out.is_empty() { yield out; }
            }
        })
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 7t  String / path std-lib (SimpleOp instances)
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Five starter ops over the SimpleOp surface. Each is a single
// transformation cursor → cursor(s); none allocate streams or batches.
// Patterns to follow when adding more (json_path, toml_get, replace, ...).

/// `Split` — split `from` term's value by `sep`, emit one child cursor
/// per non-empty piece, bound to `into` term.
pub struct Split { pub from: String, pub sep: String, pub into: String }
impl Split {
    pub fn new(from: &str, sep: &str, into: &str) -> Self {
        Self { from: from.into(), sep: sep.into(), into: into.into() }
    }
}
impl SimpleOp for Split {
    fn kind(&self) -> &'static str { "v4::Split" }
    fn instance(&self) -> String { format!("{}|{}|{}", self.from, self.sep, self.into) }
    fn step(&self, c: &Cursor) -> Vec<Cursor> {
        let Some(s) = c.get(&self.from) else { return vec![] };
        s.split(self.sep.as_str())
            .filter(|p| !p.is_empty())
            .map(|p| { let mut k = c.clone(); k.set(&self.into, p); k })
            .collect()
    }
}

/// `Format` — render a template with ${TERM} substitutions, bind the
/// rendered string to `into`. Uses `render_template` (Sh's renderer)
/// without shell-quoting; if you need quoting use Sh.
pub struct Format { pub template: String, pub into: String }
impl Format {
    pub fn new(template: &str, into: &str) -> Self {
        Self { template: template.into(), into: into.into() }
    }
}
impl SimpleOp for Format {
    fn kind(&self) -> &'static str { "v4::Format" }
    fn instance(&self) -> String { format!("{}|{}", self.template, self.into) }
    fn step(&self, c: &Cursor) -> Vec<Cursor> {
        let re = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap();
        let rendered = re.replace_all(&self.template, |caps: &regex::Captures| {
            c.get(&caps[1]).unwrap_or("").to_string()
        }).into_owned();
        let mut out = c.clone();
        out.set(&self.into, rendered);
        vec![out]
    }
}

/// `Trim` — trim whitespace on `from` term, write back to `into`
/// (or `from` itself if `into` matches).
pub struct Trim { pub from: String, pub into: String }
impl Trim {
    pub fn new(from: &str, into: &str) -> Self {
        Self { from: from.into(), into: into.into() }
    }
    pub fn in_place(name: &str) -> Self { Self::new(name, name) }
}
impl SimpleOp for Trim {
    fn kind(&self) -> &'static str { "v4::Trim" }
    fn instance(&self) -> String { format!("{}|{}", self.from, self.into) }
    fn step(&self, c: &Cursor) -> Vec<Cursor> {
        let Some(s) = c.get(&self.from) else { return vec![c.clone()] };
        let trimmed = s.trim().to_string();
        let mut out = c.clone();
        out.set(&self.into, trimmed);
        vec![out]
    }
}

/// `Basename` — extract `Path::file_name` from `from` term, bind to `into`.
pub struct Basename { pub from: String, pub into: String }
impl Basename {
    pub fn new(from: &str, into: &str) -> Self {
        Self { from: from.into(), into: into.into() }
    }
}
impl SimpleOp for Basename {
    fn kind(&self) -> &'static str { "v4::Basename" }
    fn instance(&self) -> String { format!("{}|{}", self.from, self.into) }
    fn step(&self, c: &Cursor) -> Vec<Cursor> {
        let Some(s) = c.get(&self.from) else { return vec![c.clone()] };
        let bn = std::path::Path::new(s).file_name()
            .and_then(|b| b.to_str()).unwrap_or("").to_string();
        let mut out = c.clone();
        out.set(&self.into, bn);
        vec![out]
    }
}

/// `Dirname` — extract `Path::parent` from `from` term, bind to `into`.
/// Empty string when source has no parent.
pub struct Dirname { pub from: String, pub into: String }
impl Dirname {
    pub fn new(from: &str, into: &str) -> Self {
        Self { from: from.into(), into: into.into() }
    }
}
impl SimpleOp for Dirname {
    fn kind(&self) -> &'static str { "v4::Dirname" }
    fn instance(&self) -> String { format!("{}|{}", self.from, self.into) }
    fn step(&self, c: &Cursor) -> Vec<Cursor> {
        let Some(s) = c.get(&self.from) else { return vec![c.clone()] };
        let dn = std::path::Path::new(s).parent()
            .and_then(|p| p.to_str()).unwrap_or("").to_string();
        let mut out = c.clone();
        out.set(&self.into, dn);
        vec![out]
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 7e  OpCache — Layer 3 skip-on-hit gate
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// Wraps a pure inner Op. Per input cursor, hashes (op_ident, lineage(c)).
// HIT  = drop the input. Inner is not called. Nothing flows downstream.
// MISS = forward to inner; on output, mark all miss keys as seen.
//
// Memory: HashSet<[u8; 32]>. 32 bytes per unique input ever processed.
// We do NOT hold output cursors — the determinism contract is "if we
// see the same lineage again, the inner op would produce the same rows
// it already produced last time, which the sink already absorbed."
//
// Position contract: OpCache must sit upstream of a sink (FactWrite,
// or another cache). On warm runs nothing reaches a downstream Count or
// other in-pipeline aggregator, by design. Read aggregates via the store.
//
// Lineage = blake3 of the cursor's sorted (term=value) pairs. AstNm/Re
// stamp CONTENT_HASH on outputs, which becomes part of every downstream
// cursor's lineage automatically.
//
// Refusal contract: if `inner.is_pure() == false`, we panic at construction.
pub struct OpCache {
    pub inner:  Arc<dyn Op>,
    pub seen:   Arc<Mutex<HashSet<[u8; 32]>>>,
    pub hits:   Arc<AtomicU64>,
    pub misses: Arc<AtomicU64>,
}
impl OpCache {
    pub fn wrap(inner: Arc<dyn Op>) -> Arc<Self> {
        assert!(inner.is_pure(),
            "OpCache::wrap requires a pure inner op (got is_pure=false)");
        Arc::new(Self {
            inner,
            seen:   Arc::new(Mutex::new(HashSet::new())),
            hits:   Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        })
    }
    pub fn hits(&self)   -> u64 { self.hits.load(Ordering::Relaxed) }
    pub fn misses(&self) -> u64 { self.misses.load(Ordering::Relaxed) }
}
fn cursor_lineage_hash(c: &Cursor) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    for (k, v) in &c.terms {
        h.update(k.as_bytes()); h.update(b"=");
        h.update(v.as_bytes()); h.update(b"\n");
    }
    *h.finalize().as_bytes()
}
fn cache_key(op_ident: &[u8; 32], lineage: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(op_ident);
    h.update(b"|");
    h.update(lineage);
    *h.finalize().as_bytes()
}
fn hex32(b: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for byte in b {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}
const OPCACHE_KEY_TERM: &str = "_OPCACHE_KEY";
impl Op for OpCache {
    fn ident(&self) -> [u8; 32] { self.inner.ident() }
    fn is_pure(&self) -> bool { self.inner.is_pure() }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let tele = h.tele.clone();
        let op_id = me.inner.ident();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let n_in = batch.len() as u64;
                let span = tele.start("v4::OpCache", Some(n_in));
                // Partition by seen-vs-new without holding the lock for the
                // run. Hits are dropped (no replay).
                let mut to_run:    Vec<Cursor>   = Vec::with_capacity(batch.len());
                let mut miss_keys: Vec<[u8; 32]> = Vec::with_capacity(batch.len());
                {
                    let seen = me.seen.lock().unwrap();
                    for c in batch {
                        let key = cache_key(&op_id, &cursor_lineage_hash(&c));
                        if seen.contains(&key) {
                            me.hits.fetch_add(1, Ordering::Relaxed);
                        } else {
                            me.misses.fetch_add(1, Ordering::Relaxed);
                            to_run.push(c);
                            miss_keys.push(key);
                        }
                    }
                }
                let mut total_out: u64 = 0;
                if !to_run.is_empty() {
                    let single = futures::stream::iter(vec![to_run]).boxed();
                    let mut s = me.inner.clone().run(h.clone(), single);
                    while let Some(out) = s.next().await {
                        total_out += out.len() as u64;
                        yield out;
                    }
                    let mut seen = me.seen.lock().unwrap();
                    for key in miss_keys { seen.insert(key); }
                }
                span.close(Some(total_out));
            }
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
    /// Caller-driven chain. Prepends a `Seed` of the supplied rows so the
    /// rule body runs against those rows as its input stream. With
    /// `body.is_empty()` this collapses to `[Seed > FactWrite]` — the
    /// empty-body rule echoes its caller's inputs into its sink. This is
    /// the passthrough semantics for `fact = rule with empty body`.
    pub fn into_chain_with_seed(self, seed: Vec<Cursor>) -> Vec<Arc<dyn Op>> {
        let cols_ref: Vec<&str> = self.sink.cols.iter().map(|s| s.as_str()).collect();
        let mut chain: Vec<Arc<dyn Op>> = vec![Arc::new(Seed::new(seed))];
        chain.extend(self.body);
        chain.push(Arc::new(FactWrite::new(self.sink.fact, &cols_ref)));
        chain
    }
    pub fn is_passthrough(&self) -> bool { self.body.is_empty() }
    /// Layer-2 input set hash. Probes body[0] (the rule's source op) for
    /// (path, content_hash) pairs, hashes the sorted list. None when the
    /// source op doesn't implement probe (e.g. FactRead — store-driven).
    /// POC convention: body[0] is the source.
    pub fn input_set_hash(&self) -> Option<[u8; 32]> {
        let src = self.body.first()?;
        let entries = src.probe()?;
        let mut h = blake3::Hasher::new();
        for (path, hash) in &entries {
            h.update(path.as_bytes());
            h.update(b"|");
            h.update(hash);
            h.update(b"\n");
        }
        Some(*h.finalize().as_bytes())
    }
    /// Declare the rule's sink schema in the store. Idempotent. The
    /// "rule with empty body == fact declaration" path: ensures the
    /// table exists upfront, before any FactWrite has flushed a batch.
    pub fn declare(&self, store: &Arc<dyn Store>) {
        let cols_ref: Vec<&str> = self.sink.cols.iter().map(|s| s.as_str()).collect();
        store.ensure_schema(&self.sink.fact, &cols_ref);
    }
}

/// `fact name(cols)` — declare a fact's schema once. Idempotent. Sugar
/// for `Rule::new(name, vec![], name, cols).declare(store)`. After
/// declaration, FactWrite inserts may omit any column; missing terms
/// become NULL in the row.
pub fn declare_fact(store: &Arc<dyn Store>, name: &str, cols: &[&str]) {
    Rule::new(name, vec![], name, cols).declare(store);
}

/// `rule?` — read the sink table of a previously-driven rule by partial
/// bindings. Structurally identical to FactRead aimed at `rule.sink.fact`.
pub struct RuleQuery;
impl RuleQuery {
    pub fn from_rule(r: &Rule, key_term: &str, project: &[&str]) -> FactRead {
        FactRead::new(r.sink.fact.clone(), key_term, project)
    }
    pub fn by_sink(rule_sink_fact: &str, key_term: &str, project: &[&str]) -> FactRead {
        FactRead::new(rule_sink_fact, key_term, project)
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 7r  RuleStateCache + drive_rule_cached — Layer 2 first pass
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
// In-memory rule_name → input_set_hash. Process-lifetime; not persisted.
// drive_rule_cached probes the rule, compares to the last hash, skips
// the drain on hit; on miss it drives + commits + records the new hash.

#[derive(Clone, Default)]
pub struct RuleStateCache {
    inner: Arc<Mutex<HashMap<String, [u8; 32]>>>,
}
impl RuleStateCache {
    pub fn new() -> Self { Self::default() }
    pub fn get(&self, name: &str) -> Option<[u8; 32]> {
        self.inner.lock().unwrap().get(name).copied()
    }
    pub fn set(&self, name: &str, hash: [u8; 32]) {
        self.inner.lock().unwrap().insert(name.to_string(), hash);
    }
    pub fn forget(&self, name: &str) {
        self.inner.lock().unwrap().remove(name);
    }
}

pub enum RuleRunOutcome { Skipped, Ran }

/// Layer-2 cached driver. Probe rule, hash its input set, compare to
/// last seen. Match → Skipped. Miss/no-probe → drive_two_tick + record.
pub async fn drive_rule_cached(
    rule: Rule,
    h: Hooks,
    cap: usize,
    cache: &RuleStateCache,
) -> RuleRunOutcome {
    // Empty body without a caller-supplied seed means declaration-only.
    // No source op upstream; nothing to drive. Skipped, not Ran.
    if rule.is_passthrough() {
        rule.declare(h.use_store());
        return RuleRunOutcome::Skipped;
    }
    let name = rule.name.clone();
    let new_hash = rule.input_set_hash();
    if let Some(nh) = new_hash {
        if let Some(prev) = cache.get(&name) {
            if prev == nh { return RuleRunOutcome::Skipped; }
        }
    }
    drive_two_tick(rule.into_chain(), h, cap).await;
    if let Some(nh) = new_hash { cache.set(&name, nh); }
    RuleRunOutcome::Ran
}

/// Caller-driven rule run. Pipes `seed` rows through the rule body (or
/// directly to FactWrite when body is empty) and commits at end.
/// Bypasses RuleStateCache since input set is the caller's, not the
/// rule's source op.
pub async fn drive_rule_with_input(
    rule: Rule,
    seed: Vec<Cursor>,
    h: Hooks,
    cap: usize,
) {
    drive_two_tick(rule.into_chain_with_seed(seed), h, cap).await;
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
