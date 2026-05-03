// sprefa v4 react-named POC.
//
//   server-shaped frontend — a stateful reducer hub. CLI today, HTTP
//   later. Actions in, Effects out, Store in the middle holding
//   relational facts/rules. Ops are async stream selectors that talk
//   to the Store via Hooks (the OpCtx). Cursor is a dynamic-scope bag
//   of term captures — any op may set any term; latest write wins.
//
//   Names borrowed wholesale from the redux/react world even where
//   the runtime shape is its own thing. Action / Reducer / dispatch /
//   Store / select / Hooks / Effect / Provider — all here, no shame.
//
//   ast-grep used directly (re-using v3's call shape, not its crate).
//   Workload: per .rs file extract `fn $NAME` and `use $PATH`,
//   insert into facts. Rules derive `unused_fns`, `import_count`.
//   Two actions: Run (cold-start indexer) + FileChanged (re-index one
//   file, retracts old rows so rule diffs surface).
//
//   Cold DD is parked. Store is `MemStore` (HashMap + broadcast).
//   Suspense / yield-next is parked. Lanes / priority is parked.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use ast_grep_core::{source::StrDoc, AstGrep, Pattern};
use ast_grep_language::SupportLang;
use futures::stream::{BoxStream, StreamExt};
use tokio::sync::{broadcast, mpsc, RwLock};

// ░░░▒▒▒▓▓▓██████████████████████████████████████████████████████▓▓▓▒▒▒░░░
// ░░░▒                  § 1   actions / dispatch                    ▒░░░
// ░░░▒▒▒▓▓▓██████████████████████████████████████████████████████▓▓▓▒▒▒░░░
//
// every external nudge to the system is an Action. CLI parses argv into
// one. LSP didChange would parse into another. fs-watcher events too.
// the Reducer is the one place that turns Actions into store mutations
// and pipeline kickoffs.

type Gen        = u64;
type LineageId  = u64;

#[derive(Clone, Debug)]
struct Action {
    gen:     Gen,
    parent:  Option<(Gen, LineageId)>,
    kind:    ActionKind,
}

#[derive(Clone, Debug)]
enum ActionKind {
    /// Cold-start indexer over a root.
    Run         { root: PathBuf },
    /// One file's contents changed; retract its rows + re-index.
    FileChanged { path: PathBuf },
    /// Wind down.
    Quit,
}

// ╔═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╗
// ║         § 2   cursor — dynamic-scope term-capture bag         ║
// ╚═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╝
//
// no fixed cursor schema. each op adds whatever terms make sense.
// fs sets FS=path. rev sets REV=sha. ast sets NAME, BODY, ... whatever
// the pattern captured. latest set wins. cursor is value-equality so
// the Store can dedup rows.

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
struct Cursor {
    /// sorted Vec keeps Hash/Eq stable across set order.
    terms: Vec<(Arc<str>, Arc<str>)>,
}

impl Cursor {
    fn set(&mut self, name: &str, value: impl Into<Arc<str>>) {
        let v = value.into();
        match self.terms.binary_search_by(|(n, _)| (**n).cmp(name)) {
            Ok(i)  => self.terms[i].1 = v,
            Err(i) => self.terms.insert(i, (Arc::<str>::from(name), v)),
        }
    }
    fn get(&self, name: &str) -> Option<&str> {
        self.terms.binary_search_by(|(n, _)| (**n).cmp(name))
            .ok().map(|i| &*self.terms[i].1)
    }
    fn fork_set(&self, name: &str, value: impl Into<Arc<str>>) -> Self {
        let mut c = self.clone();
        c.set(name, value);
        c
    }
}

// ▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░
// ░  § 3   Store — relational state, redux-style dispatch             ░
// ▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░
//
// Store trait abstracts over the relational backend. impls today: MemStore
// (HashMap + tokio::broadcast). impls tomorrow: DdStore (DD on its own
// thread), LspBufferStore (overlays disk), GitStore (git2-backed).
//
// rules are declared once at startup. derived collections live forever;
// any subscriber sees diffs streaming as facts mutate. select() is the
// useSyncExternalStore moral equivalent.

#[async_trait::async_trait]
trait Store: Send + Sync + 'static {
    async fn insert(&self, fact: &str, row: Cursor, gen: Gen);
    async fn insert_many(&self, fact: &str, rows: Vec<Cursor>, gen: Gen);
    async fn remove(&self, fact: &str, row: Cursor, gen: Gen);
    /// Drop every row in `fact` whose cursor's `key_term` equals `key_value`.
    async fn forget_by(&self, fact: &str, key_term: &str, key_value: &str, gen: Gen);
    fn define_rule(&self, name: &str, body: RuleBody);
    fn select(&self, name: &str) -> BoxStream<'static, Diff>;
    async fn snapshot(&self, name: &str) -> Vec<Cursor>;
}

#[derive(Clone, Debug)]
struct Diff { row: Cursor, gen: Gen, sign: i8 }

#[derive(Clone)]
enum RuleBody {
    /// Pass through rows of `src` matching `pred`.
    Filter   { src: String, pred: Arc<dyn Fn(&Cursor) -> bool + Send + Sync> },
    /// Rows of `left` whose `key` term value is NOT a value of `right`'s `key`.
    Antijoin { left: String, right: String, key: String },
    /// For each distinct value of `key` in `src`, emit a row with `key`=value
    /// and `count`=count if count >= `min`.
    GroupCount { src: String, key: String, min: usize, count_term: String },
}

struct MemStore {
    /// fact name → set of (cursor → most-recent gen). HashMap-as-set keyed by Cursor.
    facts:    RwLock<HashMap<String, HashMap<Cursor, Gen>>>,
    /// rules + channels are touched in sync paths (define_rule, select);
    /// no `.await` held. std::sync::RwLock is the right primitive.
    rules:    std::sync::RwLock<HashMap<String, RuleBody>>,
    channels: std::sync::RwLock<HashMap<String, broadcast::Sender<Diff>>>,
}

impl MemStore {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            facts: Default::default(), rules: Default::default(),
            channels: Default::default(),
        })
    }
    fn channel(&self, name: &str) -> broadcast::Sender<Diff> {
        if let Some(tx) = self.channels.read().unwrap().get(name) { return tx.clone(); }
        let mut w = self.channels.write().unwrap();
        w.entry(name.to_string()).or_insert_with(|| broadcast::channel(1024).0).clone()
    }
    /// Re-derive every rule that depends on `changed_fact` against the latest
    /// snapshot, push diffs onto the rule's channel. Naive; fine for POC.
    async fn rederive(&self, changed_fact: &str) {
        let rules = self.rules.read().unwrap().clone();
        for (name, body) in rules {
            if !body_depends_on(&body, changed_fact) { continue; }
            let derived = self.materialize(&body).await;
            let tx = self.channel(&name);
            // emit derived rows as +1 with current gen; this POC doesn't
            // diff against last derivation (a real impl maintains
            // `previous` and emits +1 / -1 deltas only).
            let g = GEN.load(Ordering::SeqCst);
            for row in derived { let _ = tx.send(Diff { row, gen: g, sign: 1 }); }
        }
    }
    async fn materialize(&self, body: &RuleBody) -> Vec<Cursor> {
        let facts = self.facts.read().await;
        match body {
            RuleBody::Filter { src, pred } => facts.get(src)
                .map(|set| set.keys().filter(|c| pred(c)).cloned().collect())
                .unwrap_or_default(),
            RuleBody::Antijoin { left, right, key } => {
                let l = facts.get(left ).map(|s| s.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
                let r = facts.get(right).map(|s| s.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
                let r_keys: std::collections::HashSet<_> =
                    r.iter().filter_map(|c| c.get(key).map(str::to_owned)).collect();
                l.into_iter().filter(|c| c.get(key).map_or(true, |v| !r_keys.contains(v))).collect()
            }
            RuleBody::GroupCount { src, key, min, count_term } => {
                let s = facts.get(src).map(|s| s.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
                let mut counts: HashMap<String, usize> = HashMap::new();
                for c in &s { if let Some(k) = c.get(key) { *counts.entry(k.into()).or_default() += 1; } }
                counts.into_iter()
                    .filter(|(_, n)| n >= min)
                    .map(|(k, n)| {
                        let mut c = Cursor::default();
                        c.set(key, k);
                        c.set(count_term, n.to_string());
                        c
                    })
                    .collect()
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
        self.rederive(fact).await;
    }
    async fn remove(&self, fact: &str, row: Cursor, gen: Gen) {
        let removed = {
            let mut w = self.facts.write().await;
            w.entry(fact.to_string()).or_default().remove(&row).is_some()
        };
        if removed {
            let tx = self.channel(fact);
            let _ = tx.send(Diff { row, gen, sign: -1 });
            self.rederive(fact).await;
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
            self.rederive(fact).await;
        }
    }
    fn define_rule(&self, name: &str, body: RuleBody) {
        let mut r = self.rules.write().unwrap();
        r.insert(name.to_string(), body);
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
        if let Some(body) = r {
            return self.materialize(&body).await;
        }
        self.facts.read().await.get(name)
            .map(|s| s.keys().cloned().collect()).unwrap_or_default()
    }
}

// ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐
// ╳    § 4   Hooks — the Provider; Op authors call use_*()         ╳
// └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘ └─┘
//
// Hooks is what React calls "context" + "hook surface" smashed together.
// every Op gets one. holds the Store handle, the effect tx, the gen
// counter. methods are use_* by convention so call sites read like
// useStore(), useDispatch(), useEffectQueue(), useGen().

#[derive(Clone)]
struct Hooks {
    store:   Arc<dyn Store>,
    effects: mpsc::UnboundedSender<Effect>,
    gen:     Gen,
    lineage: LineageId,
}

impl Hooks {
    fn use_store(&self) -> &Arc<dyn Store> { &self.store }
    fn use_dispatch_effect(&self, e: Effect) { let _ = self.effects.send(e); }
    fn use_gen(&self) -> Gen { self.gen }
}

// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
// ▒  § 5   Effect — fire-and-forget mutations leaving the system   ▒
// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
//
// CQRS split: queries+yields live behind Hooks (request/response).
// fire-and-forget mutations land in this enum and travel out the effect
// channel. the saga side decides what to do.

#[derive(Debug)]
enum Effect {
    Print(String),
    // WriteFile { path: PathBuf, bytes: Vec<u8> },
    // PublishDiag { uri: String, ... },
}

// ◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤
// ◣◢ § 6   Op — async stream selector, the closest cousin of a       ◣
// ◣◢       React component. takes Stream<Cursor>, returns Stream<Cursor>.
// ◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤◥◤
//
// no Vec<Box<dyn Op>> object-safety drama: ops compose by stream chaining
// in code, like .pipe() in rxjs. an outer enum plays the IR role for
// ops sourced from .sprf scripts. for this POC, pipelines are built in
// build_workload() with concrete struct ops.

trait Op: Send + Sync + 'static {
    fn ident(&self) -> [u8; 32];
    fn run(self: Arc<Self>, hooks: Hooks, input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>;
}

fn ident_of(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    for p in parts { h.update(p); h.update(b"|"); }
    *h.finalize().as_bytes()
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//          § 7   concrete ops — Fs, Ast, Fact, Select, Print
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//
// each op is a struct holding its own params. Op::run is async-stream
// flavored. ast-grep called directly. cursors carry whatever terms the
// upstream op set; this op may extend them.

/// Walks `root`, emits batches of cursors (BATCH paths each) with FS=path.
const BATCH: usize = 64;
struct Fs { root: PathBuf, ext: String }
impl Op for Fs {
    fn ident(&self) -> [u8; 32] {
        ident_of(&[b"fs", self.root.to_string_lossy().as_bytes(), self.ext.as_bytes()])
    }
    fn run(self: Arc<Self>, _h: Hooks, _in: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        Box::pin(async_stream::stream! {
            let mut files = Vec::new();
            walk(&me.root, &me.ext, &mut files);
            let mut buf: Vec<Cursor> = Vec::with_capacity(BATCH);
            for f in files {
                let mut c = Cursor::default();
                c.set("FS", f.display().to_string());
                buf.push(c);
                if buf.len() >= BATCH { yield std::mem::take(&mut buf); }
            }
            if !buf.is_empty() { yield buf; }
        })
    }
}

fn walk(root: &PathBuf, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().map_or(false, |n| n == "target" || n.to_string_lossy().starts_with('.')) {
                continue;
            }
            walk(&p, ext, out);
        } else if p.extension().map_or(false, |x| x == ext) { out.push(p); }
    }
}

/// Reads cursor[FS] file contents, runs `pattern` (rust ast-grep), emits
/// one downstream cursor per match. captures named in the pattern are
/// set on the cursor (e.g. NAME). LO/HI track the match range.
struct AstNm { pattern_src: String, capture_names: Vec<String> }
impl AstNm {
    fn new(pat: &str, captures: &[&str]) -> Self {
        Self { pattern_src: pat.to_string(),
               capture_names: captures.iter().map(|s| s.to_string()).collect() }
    }
}
impl Op for AstNm {
    fn ident(&self) -> [u8; 32] {
        ident_of(&[b"ast", b"rust", self.pattern_src.as_bytes()])
    }
    fn run(self: Arc<Self>, _h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        let pat = Arc::new(Pattern::new(&me.pattern_src, SupportLang::Rust));
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let mut out: Vec<Cursor> = Vec::with_capacity(batch.len() * 4);
                for c in &batch {
                    let path = match c.get("FS") { Some(p) => p.to_string(), None => continue };
                    let Ok(src) = std::fs::read_to_string(&path) else { continue };
                    let grep: AstGrep<StrDoc<SupportLang>> = AstGrep::new(&src, SupportLang::Rust);
                    for nm in grep.root().find_all(&*pat) {
                        let env = nm.get_env();
                        let r = nm.range();
                        let mut child = c.clone();
                        child.set("LO", (r.start as u64).to_string());
                        child.set("HI", (r.end   as u64).to_string());
                        for nm_name in &me.capture_names {
                            if let Some(node) = env.get_match(nm_name) {
                                child.set(nm_name, node.text().to_string());
                            }
                        }
                        out.push(child);
                    }
                }
                if !out.is_empty() { yield out; }
            }
        })
    }
}

/// Insert each upstream batch into the named fact via insert_many. Pass-through.
struct Fact { name: String }
impl Op for Fact {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"fact", self.name.as_bytes()]) }
    fn run(self: Arc<Self>, h: Hooks, mut input: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                h.use_store().insert_many(&me.name, batch.clone(), h.use_gen()).await;
                yield batch;
            }
        })
    }
}

/// Source op: subscribes to `name` (fact or rule), emits batches of cursors
/// per diff burst. SIGN/GEN terms set per row so downstream can branch.
/// Coalesces backlog into one batch per wakeup.
struct Select { name: String }
impl Op for Select {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"select", self.name.as_bytes()]) }
    fn run(self: Arc<Self>, h: Hooks, _in: BoxStream<'static, Vec<Cursor>>)
        -> BoxStream<'static, Vec<Cursor>>
    {
        let mut sub = h.use_store().select(&self.name);
        Box::pin(async_stream::stream! {
            while let Some(d) = sub.next().await {
                // drain any contiguously-ready diffs into one batch
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

/// Sink: format upstream cursors via a tiny `{TERM}` template; dispatches
/// one Print effect per row but iterates the whole batch in one task wake.
/// Emits nothing downstream.
struct Print { template: String }
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

// ▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟
// ▜▛  § 8   Reducer — turns Actions into store mutations + pipelines  ▜▛
// ▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟
//
// the dispatch table. one match arm per ActionKind. drives the
// indexer pipeline on Run, drives a single-file re-index on
// FileChanged. the LSP/CLI/HTTP adapters all hand actions to dispatch().

static GEN: AtomicU64 = AtomicU64::new(0);
static LIN: AtomicU64 = AtomicU64::new(0);

struct Reducer { store: Arc<dyn Store>, effects_tx: mpsc::UnboundedSender<Effect> }
impl Reducer {
    fn new_action(kind: ActionKind, parent: Option<(Gen, LineageId)>) -> Action {
        Action {
            gen: GEN.fetch_add(1, Ordering::SeqCst) + 1,
            parent,
            kind,
        }
    }
    async fn dispatch(&self, action: Action) {
        let lin = LIN.fetch_add(1, Ordering::SeqCst) + 1;
        let hooks = Hooks {
            store:   self.store.clone(),
            effects: self.effects_tx.clone(),
            gen:     action.gen,
            lineage: lin,
        };
        match action.kind {
            ActionKind::Run { root } => {
                eprintln!("▶ Run @ gen {} root={}", action.gen, root.display());
                run_indexer(&hooks, root).await;
            }
            ActionKind::FileChanged { path } => {
                eprintln!("▶ FileChanged @ gen {} path={}", action.gen, path.display());
                let p = path.display().to_string();
                self.store.forget_by("fns",  "FS", &p, action.gen).await;
                self.store.forget_by("uses", "FS", &p, action.gen).await;
                index_one(&hooks, path).await;
            }
            ActionKind::Quit => { eprintln!("▶ Quit @ gen {}", action.gen); }
        }
    }
}

// ╶─━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━╴
//        § 9   workload — index every fn+use, rules over them
// ╶─━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━╴
//
// builds two pipelines per Run action: fns-from-`fn $NAME`,
// uses-from-`use $PATH`. defines two rules at startup: imports_used_2plus
// (group-count) + unused_fns (antijoin between fns and uses on NAME).

async fn run_indexer(h: &Hooks, root: PathBuf) {
    let pipelines: Vec<Vec<Arc<dyn Op>>> = vec![
        vec![
            Arc::new(Fs { root: root.clone(), ext: "rs".into() }),
            Arc::new(AstNm::new("fn $NAME",  &["NAME"])),
            Arc::new(Fact { name: "fns".into() }),
        ],
        vec![
            Arc::new(Fs { root: root.clone(), ext: "rs".into() }),
            Arc::new(AstNm::new("use $PATH", &["PATH"])),
            Arc::new(Fact { name: "uses".into() }),
        ],
    ];
    let tasks: Vec<_> = pipelines.into_iter().map(|chain| {
        let h = h.clone();
        tokio::spawn(async move { drive(chain, h).await })
    }).collect();
    for t in tasks { let _ = t.await; }
}

async fn index_one(h: &Hooks, path: PathBuf) {
    let chains: Vec<Vec<Arc<dyn Op>>> = vec![
        vec![
            Arc::new(SinglePath { path: path.clone() }),
            Arc::new(AstNm::new("fn $NAME",  &["NAME"])),
            Arc::new(Fact { name: "fns".into() }),
        ],
        vec![
            Arc::new(SinglePath { path: path.clone() }),
            Arc::new(AstNm::new("use $PATH", &["PATH"])),
            Arc::new(Fact { name: "uses".into() }),
        ],
    ];
    for chain in chains { drive(chain, h.clone()).await; }
}

/// Source op variant: emit a one-row batch for a known path.
struct SinglePath { path: PathBuf }
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

/// Compose a chain of ops into one stream and drain it to /dev/null
/// (Print sinks dispatch effects out of band; we just need the stream
/// to complete).
async fn drive(chain: Vec<Arc<dyn Op>>, h: Hooks) {
    if chain.is_empty() { return; }
    let mut iter = chain.into_iter();
    let first = iter.next().unwrap();
    let empty: BoxStream<'static, Vec<Cursor>> = Box::pin(futures::stream::empty());
    let mut s = first.run(h.clone(), empty);
    for op in iter { s = op.run(h.clone(), s); }
    while s.next().await.is_some() {}
}

// ╔══════════════════════════════════════════════════════════════════════╗
// ║       § 10   main — argv → Action → dispatch → drain effects         ║
// ╚══════════════════════════════════════════════════════════════════════╝
//
// argv: v4-proto <root> [--changed <path>]. the CLI is the stub for the
// HTTP front: parses argv into Actions, hands them to the Reducer, then
// blocks until the rule subscription stream goes idle.

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root: PathBuf = args.first().cloned().map(PathBuf::from).unwrap_or_else(|| ".".into());
    let changed: Option<PathBuf> = args.iter().position(|a| a == "--changed")
        .and_then(|i| args.get(i+1).cloned()).map(PathBuf::from);

    let store = MemStore::new() as Arc<dyn Store>;

    // Define rules at startup. dependencies on facts are derived structurally.
    store.define_rule("unused_fns", RuleBody::Antijoin {
        left: "fns".into(), right: "uses".into(), key: "NAME".into(),
    });
    store.define_rule("imports_used_2plus", RuleBody::GroupCount {
        src: "uses".into(), key: "PATH".into(), min: 2, count_term: "COUNT".into(),
    });

    let (eff_tx, mut eff_rx) = mpsc::unbounded_channel();
    let reducer = Arc::new(Reducer { store: store.clone(), effects_tx: eff_tx });

    // Effect drain task — the saga side. Just prints for now.
    let saga = tokio::spawn(async move {
        while let Some(e) = eff_rx.recv().await {
            match e { Effect::Print(s) => println!("{}", s) }
        }
    });

    // Background watchers — print rule diffs as they happen. Spawned
    // BEFORE the indexer Run so they catch first inserts.
    {
        let h = Hooks {
            store: store.clone(),
            effects: reducer.effects_tx.clone(),
            gen: 0, lineage: 0,
        };
        let chain: Vec<Arc<dyn Op>> = vec![
            Arc::new(Select { name: "unused_fns".into() }),
            Arc::new(Print  { template: "  unused_fn  {SIGN} gen={GEN} NAME={NAME} FS={FS}".into() }),
        ];
        tokio::spawn(async move { drive(chain, h).await });
    }
    {
        let h = Hooks {
            store: store.clone(),
            effects: reducer.effects_tx.clone(),
            gen: 0, lineage: 0,
        };
        let chain: Vec<Arc<dyn Op>> = vec![
            Arc::new(Select { name: "imports_used_2plus".into() }),
            Arc::new(Print  { template: "  hot_use   {SIGN} gen={GEN} PATH={PATH} COUNT={COUNT}".into() }),
        ];
        tokio::spawn(async move { drive(chain, h).await });
    }

    // Action 1: cold-start indexer.
    let t0 = std::time::Instant::now();
    reducer.dispatch(Reducer::new_action(ActionKind::Run { root }, None)).await;
    eprintln!("⏱  indexer round done in {:?}", t0.elapsed());

    // Action 2 (optional): re-index one file. retract+insert proves rule diffs.
    if let Some(p) = changed {
        let parent_g = GEN.load(Ordering::SeqCst);
        let act = Reducer::new_action(ActionKind::FileChanged { path: p }, Some((parent_g, 0)));
        reducer.dispatch(act).await;
    }

    // Let the broadcast channels drain.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Snapshot summary.
    let fns = store.snapshot("fns").await;
    let uses = store.snapshot("uses").await;
    let unused = store.snapshot("unused_fns").await;
    let hot = store.snapshot("imports_used_2plus").await;
    eprintln!("── snapshot: fns={} uses={} unused_fns={} hot_imports={}",
              fns.len(), uses.len(), unused.len(), hot.len());

    // Wind down the saga.
    drop(reducer);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(20), saga).await;
    Ok(())
}
