pub mod chan;
pub mod cst;
pub mod cursor_codec;
pub mod fact;
pub mod lower;
pub mod rule;
pub mod runtime_bridge;
pub mod v2_ops;
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

use futures::stream::BoxStream;
use tokio::sync::{broadcast, RwLock};

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

/// `fact name(cols)` — declare a fact's schema once. Idempotent.
pub fn declare_fact(store: &Arc<dyn Store>, name: &str, cols: &[&str]) {
    store.ensure_schema(name, cols);
}
