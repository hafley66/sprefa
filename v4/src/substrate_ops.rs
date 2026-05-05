//! Three Components for v4-bench's `--substrate` path. Bare mode
//! only (Fs > AstNm > Count). Smallest-viable port: no telemetry, no
//! interner, no Layer-2 / Layer-3, no Store. The point is an A/B
//! perf number against the native v4 stream pipeline.
//!
//! Wire from v4_bench.rs:
//!
//! ```ignore
//! use effect_runtime::v2::*;
//! use v4::substrate_ops::{FsComponent, AstNmComponent, CountComponent};
//!
//! let pipe = PipeInstance::new(vec![
//!     Arc::new(FsComponent::new(root, exts, batch))     as Arc<dyn Component<Next = Cursor>>,
//!     Arc::new(AstNmComponent::new(pat_src, lang)),
//!     Arc::new(CountComponent { count: counter.clone() }),
//! ]);
//! expand(&pipe, queue, vec![Arc::new(Cursor::default())], ExpandOpts::default().with_batch_cap(batch));
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ast_grep_core::{source::StrDoc, AstGrep, Language, Pattern};
use ast_grep_language::SupportLang;
use effect_runtime::v2::{
    par_render, splice_into_at, Component, EventBus, Node, QueueBackend,
    QueueRow, RenderCtx,
};
use ignore::WalkBuilder;

use crate::{Cursor, Interner, Store};

/// Helper: pack a Vec of cursors into the right Node shape.
/// 0 -> Done, 1 -> Emit, N -> Many of Emit.
fn nodes_from(out: Vec<Cursor>) -> Node<Cursor> {
    match out.len() {
        0 => Node::Done,
        1 => Node::Emit(Arc::new(out.into_iter().next().unwrap())),
        _ => Node::Many(out.into_iter().map(|c| Node::Emit(Arc::new(c))).collect()),
    }
}

/// Tier-3 source op. Ignores its input cursor (the bootstrap seed),
/// walks `root` with `WalkBuilder`, splices one child Cursor per
/// matching file into the queue. Sync — no tokio split. The producer
/// blocks the driver thread until the walk is done; AstNm doesn't
/// start until then. Trade-off accepted for slice-1 simplicity.
pub struct FsComponent {
    pub root:  PathBuf,
    pub exts:  Vec<String>,
    pub batch: usize,
}

impl FsComponent {
    pub fn new(root: PathBuf, exts: Vec<String>, batch: usize) -> Self {
        Self { root, exts, batch }
    }
}

impl Component for FsComponent {
    type Next = Cursor;

    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        // One bootstrap row produces all children. Extra seeds (if
        // any) are ignored — not the model here.
        let parent = match rows.first() {
            Some(r) => r,
            None    => return,
        };

        // Streaming flushes share one parent — must thread a running
        // batch_idx offset across calls so sibling paths stay unique.
        // (The path collision is invisible for content-keyed
        // memoization but breaks position-keyed reconcile via cascade
        // of "doomed" already-enqueued descendants.)
        let mut buf: Vec<Node<Cursor>> = Vec::with_capacity(self.batch);
        let mut next_idx: u32 = 0;
        let mut flush = |buf: &mut Vec<Node<Cursor>>, next_idx: &mut u32| {
            if buf.is_empty() { return; }
            let many = Node::Many(std::mem::take(buf));
            let n = splice_into_at(parent, many, ctx.depth + 1, ctx.expand_tick, queue, *next_idx);
            *next_idx += n as u32;
        };

        for entry in WalkBuilder::new(&self.root).hidden(true).git_ignore(false).build() {
            let Ok(e) = entry else { continue };
            if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
            let p = e.into_path();
            let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
            if !self.exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }

            let mut c = Cursor::default();
            c.set("FS", p.display().to_string());
            buf.push(Node::Emit(Arc::new(c)));

            if buf.len() >= self.batch { flush(&mut buf, &mut next_idx); }
        }
        flush(&mut buf, &mut next_idx);
    }
}

/// Tier-2 op. ast-grep matcher. Pattern compiled once at construction;
/// each batch fans across rayon via `par_render`. Per cursor: read FS
/// file, prefilter with `Pattern::fixed_string`, parse, match, emit
/// one child per hit with LO/HI byte range stamped.
pub struct AstNmComponent {
    lang:    SupportLang,
    pattern: Arc<Pattern<SupportLang>>,
    fixed:   Arc<str>,
}

impl AstNmComponent {
    pub fn new(pat_src: String, lang: SupportLang) -> Self {
        let pattern = Arc::new(Pattern::new(&pat_src, lang));
        let fixed: Arc<str> = Arc::from(pattern.fixed_string().to_string().as_str());
        Self { lang, pattern, fixed }
    }
}

impl Component for AstNmComponent {
    type Next = Cursor;

    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        par_render(batch, |c| {
            let Some(path) = c.get("FS") else { return Node::Done };
            let Ok(bytes)  = std::fs::read(path) else { return Node::Done };
            // tree-sitter takes any bytes; UTF-8 errors → ERROR nodes.
            let src: String = unsafe { String::from_utf8_unchecked(bytes) };
            if !self.fixed.is_empty() && !src.contains(&*self.fixed) {
                return Node::Done;
            }
            let grep: AstGrep<StrDoc<SupportLang>> = self.lang.ast_grep(&src);
            let hits: Vec<Node<Cursor>> = grep.root().find_all(&*self.pattern).map(|nm| {
                let r = nm.range();
                let mut child = c.clone();
                child.set("LO", (r.start as u64).to_string());
                child.set("HI", (r.end   as u64).to_string());
                Node::Emit(Arc::new(child))
            }).collect();
            if hits.is_empty() { Node::Done } else { Node::Many(hits) }
        })
    }
}

/// Multi-pattern AstNm. Parses each file once; runs all patterns
/// against the same `AstGrep<StrDoc>`. Each match cursor carries `PAT`
/// = pattern label. Prefilter: file is read+parsed if ANY pattern's
/// fixed_string is present (or any pattern has no fixed_string).
pub struct MultiAstNmComponent {
    lang:     SupportLang,
    patterns: Arc<Vec<(Arc<str>, Arc<Pattern<SupportLang>>, Arc<str>)>>,
}

impl MultiAstNmComponent {
    pub fn new(patterns: Vec<(String, String)>, lang: SupportLang) -> Self {
        let pats: Vec<(Arc<str>, Arc<Pattern<SupportLang>>, Arc<str>)> = patterns
            .into_iter()
            .map(|(label, src)| {
                let p  = Pattern::new(&src, lang);
                let fx: Arc<str> = Arc::from(p.fixed_string().to_string().as_str());
                (Arc::<str>::from(label.as_str()), Arc::new(p), fx)
            })
            .collect();
        Self { lang, patterns: Arc::new(pats) }
    }
}

impl Component for MultiAstNmComponent {
    type Next = Cursor;

    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let pats = self.patterns.clone();
        let lang = self.lang;
        par_render(batch, move |c| {
            let Some(path) = c.get("FS") else { return Node::Done };
            let Ok(bytes)  = std::fs::read(path) else { return Node::Done };
            let src: String = unsafe { String::from_utf8_unchecked(bytes) };
            let any_fires = pats.iter().any(|(_, _, fx)| fx.is_empty() || src.contains(&**fx));
            if !any_fires { return Node::Done; }
            let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(&src);
            let mut hits: Vec<Node<Cursor>> = Vec::new();
            for (label, pat, fx) in pats.iter() {
                if !fx.is_empty() && !src.contains(&**fx) { continue; }
                for nm in grep.root().find_all(&**pat) {
                    let r = nm.range();
                    let mut child = c.clone();
                    child.set_arc("PAT", label.clone());
                    child.set("LO",  (r.start as u64).to_string());
                    child.set("HI",  (r.end   as u64).to_string());
                    hits.push(Node::Emit(Arc::new(child)));
                }
            }
            if hits.is_empty() { Node::Done } else { Node::Many(hits) }
        })
    }
}

/// Tier-3 source op. Emits exactly one Cursor with FS=path. Used for
/// the bench's --file mode (single-file scaling probe).
pub struct SinglePathComponent {
    pub path: PathBuf,
}

impl SinglePathComponent {
    pub fn new(path: PathBuf) -> Self { Self { path } }
}

impl Component for SinglePathComponent {
    type Next = Cursor;

    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let parent = match rows.first() { Some(r) => r, None => return };
        let mut c = Cursor::default();
        c.set("FS", self.path.display().to_string());
        let node = Node::Emit(Arc::new(c));
        splice_into_at(parent, node, ctx.depth + 1, ctx.expand_tick, queue, 0);
    }
}

/// Tier-2 sink. Pass-through that calls `store.insert_many(fact, batch, gen)`
/// for every dispatch batch. Schema declared at construction (idempotent).
/// Gen comes from `ctx.expand_tick` — substrate equivalent of v4's Hooks.gen.
///
/// Store-injection pattern: Component holds Arc<dyn Store> at construction.
/// No RenderCtx changes needed.
pub struct FactWriteComponent {
    store: Arc<dyn Store>,
    fact:  String,
    cols:  Vec<String>,
}

impl FactWriteComponent {
    pub fn new(store: Arc<dyn Store>, fact: impl Into<String>, cols: &[&str]) -> Self {
        let fact = fact.into();
        let cols: Vec<String> = cols.iter().map(|s| s.to_string()).collect();
        let cols_ref: Vec<&str> = cols.iter().map(|s| s.as_str()).collect();
        store.ensure_schema(&fact, &cols_ref);
        Self { store, fact, cols }
    }
}

impl Component for FactWriteComponent {
    type Next = Cursor;

    fn render_batch(&self, ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let _cols = &self.cols;
        // insert_many wants owned Vec<Cursor>; clone is cheap (Cursor.terms
        // is a small Vec of Arc<str> pairs).
        let owned: Vec<Cursor> = batch.iter().map(|c| (*c).clone()).collect();
        let gen = ctx.expand_tick;
        let store = self.store.clone();
        let fact  = self.fact.clone();
        // Substrate is sync. We need to run an async insert. Bench main is
        // #[tokio::main] so Handle::current() is valid; spin a blocking
        // section that drives the future to completion.
        let handle = tokio::runtime::Handle::current();
        let _ = tokio::task::block_in_place(|| {
            handle.block_on(store.insert_many(&fact, owned, gen))
        });
        // Pass-through: emit each cursor downstream so consumers can count
        // / read further.
        batch.iter().map(|c| Node::Emit(Arc::new((*c).clone()))).collect()
    }
}

/// Periodic commit. Counts dispatch batches; every Nth, calls
/// `store.commit(gen)`. Pass-through. Useful to measure store commit
/// overhead under reactive-shaped workloads (many small commits).
pub struct CommitEveryComponent {
    store: Arc<dyn Store>,
    every: usize,
    seen:  std::sync::atomic::AtomicUsize,
}

impl CommitEveryComponent {
    pub fn new(store: Arc<dyn Store>, every: usize) -> Self {
        Self { store, every: every.max(1), seen: std::sync::atomic::AtomicUsize::new(0) }
    }
}

impl Component for CommitEveryComponent {
    type Next = Cursor;

    fn render_batch(&self, ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let n = self.seen.fetch_add(1, Ordering::Relaxed) + 1;
        if n % self.every == 0 {
            let store = self.store.clone();
            let gen = ctx.expand_tick;
            let handle = tokio::runtime::Handle::current();
            let _ = tokio::task::block_in_place(|| handle.block_on(store.commit(gen)));
        }
        batch.iter().map(|c| Node::Emit(Arc::new((*c).clone()))).collect()
    }
}

/// Tier-1 op. Counts every cursor that reaches it. `Node::Done`
/// terminates the row.
pub struct CountComponent {
    pub count: Arc<AtomicU64>,
}

impl Component for CountComponent {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, _c: &Cursor) -> Node<Cursor> {
        self.count.fetch_add(1, Ordering::Relaxed);
        Node::Done
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   Pure cursor mutators (former SimpleOp variants)
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░

/// Split `from` term's value by `sep`, emit one child cursor per
/// non-empty piece, bound to `into` term.
pub struct SplitComponent { pub from: String, pub sep: String, pub into: String }
impl SplitComponent {
    pub fn new(from: &str, sep: &str, into: &str) -> Self {
        Self { from: from.into(), sep: sep.into(), into: into.into() }
    }
}
impl Component for SplitComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let Some(s) = c.get(&self.from) else { return Node::Done };
        let out: Vec<Cursor> = s.split(self.sep.as_str())
            .filter(|p| !p.is_empty())
            .map(|p| { let mut k = c.clone(); k.set(&self.into, p); k })
            .collect();
        nodes_from(out)
    }
}

/// Render a template with `${TERM}` substitutions, bind result to `into`.
pub struct FormatComponent { pub template: String, pub into: String }
impl FormatComponent {
    pub fn new(template: &str, into: &str) -> Self {
        Self { template: template.into(), into: into.into() }
    }
}
impl Component for FormatComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let re = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap();
        let rendered = re.replace_all(&self.template, |caps: &regex::Captures| {
            c.get(&caps[1]).unwrap_or("").to_string()
        }).into_owned();
        let mut out = c.clone();
        out.set(&self.into, rendered);
        Node::Emit(Arc::new(out))
    }
}

/// Trim whitespace on `from` term, write back to `into` (or `from`
/// itself if `into == from`).
pub struct TrimComponent { pub from: String, pub into: String }
impl TrimComponent {
    pub fn new(from: &str, into: &str) -> Self {
        Self { from: from.into(), into: into.into() }
    }
    pub fn in_place(name: &str) -> Self { Self::new(name, name) }
}
impl Component for TrimComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let Some(s) = c.get(&self.from) else { return Node::Emit(Arc::new(c.clone())) };
        let trimmed = s.trim().to_string();
        let mut out = c.clone();
        out.set(&self.into, trimmed);
        Node::Emit(Arc::new(out))
    }
}

/// Extract `Path::file_name` from `from` term, bind to `into`.
pub struct BasenameComponent { pub from: String, pub into: String }
impl BasenameComponent {
    pub fn new(from: &str, into: &str) -> Self {
        Self { from: from.into(), into: into.into() }
    }
}
impl Component for BasenameComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let Some(s) = c.get(&self.from) else { return Node::Emit(Arc::new(c.clone())) };
        let bn = std::path::Path::new(s).file_name()
            .and_then(|b| b.to_str()).unwrap_or("").to_string();
        let mut out = c.clone();
        out.set(&self.into, bn);
        Node::Emit(Arc::new(out))
    }
}

/// Extract `Path::parent` from `from` term, bind to `into`.
pub struct DirnameComponent { pub from: String, pub into: String }
impl DirnameComponent {
    pub fn new(from: &str, into: &str) -> Self {
        Self { from: from.into(), into: into.into() }
    }
}
impl Component for DirnameComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let Some(s) = c.get(&self.from) else { return Node::Emit(Arc::new(c.clone())) };
        let dn = std::path::Path::new(s).parent()
            .and_then(|p| p.to_str()).unwrap_or("").to_string();
        let mut out = c.clone();
        out.set(&self.into, dn);
        Node::Emit(Arc::new(out))
    }
}

/// Split a term's text by `\n`, emit one child cursor per non-empty
/// line bound to `target`.
pub struct LineComponent { pub source: String, pub target: String }
impl LineComponent {
    pub fn on(term: impl Into<String>) -> Self {
        let s = term.into();
        Self { target: s.clone(), source: s }
    }
    pub fn into_term(mut self, target: impl Into<String>) -> Self {
        self.target = target.into(); self
    }
}
impl Component for LineComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let Some(text) = c.get(&self.source).map(|s| s.to_string()) else { return Node::Done };
        let out: Vec<Cursor> = text.split('\n')
            .filter(|l| !l.is_empty())
            .map(|line| { let mut child = c.clone(); child.set(&self.target, line); child })
            .collect();
        nodes_from(out)
    }
}

/// Regex match. Reads `on` term as the haystack (or the file at FS if
/// `on == "FS"`). Stamps LO/HI/MATCH/captures on each hit. When `on ==
/// "FS"`, also stamps `CONTENT_HASH` interned via `Interner`.
pub struct ReComponent {
    on:            String,
    capture_names: Vec<String>,
    want_match:    bool,
    re:            Arc<regex::Regex>,
    interner:      Option<Arc<Interner>>,
}
impl ReComponent {
    pub fn new(pat: &str, captures: &[&str]) -> Self {
        Self {
            on:            "FS".to_string(),
            capture_names: captures.iter().map(|s| s.to_string()).collect(),
            want_match:    true,
            re:            Arc::new(regex::Regex::new(pat).expect("compile regex")),
            interner:      None,
        }
    }
    pub fn on_term(mut self, term: &str) -> Self { self.on = term.to_string(); self }
    pub fn with_match_text(mut self, on: bool) -> Self { self.want_match = on; self }
    pub fn with_interner(mut self, interner: Arc<Interner>) -> Self {
        self.interner = Some(interner); self
    }
}
impl Component for ReComponent {
    type Next = Cursor;
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let re = self.re.clone();
        let on = self.on.clone();
        let capture_names = self.capture_names.clone();
        let want_match = self.want_match;
        let interner = self.interner.clone();
        par_render(batch, move |c| {
            let (haystack, content_hash_arc): (String, Option<Arc<str>>) =
                if on == "FS" {
                    let Some(path) = c.get("FS") else { return Node::Done };
                    let Ok(bytes) = std::fs::read(path) else { return Node::Done };
                    let h_hex = blake3::hash(&bytes).to_hex().to_string();
                    let h_arc: Option<Arc<str>> = interner.as_ref().map(|i| i.intern(&h_hex));
                    let s = match String::from_utf8(bytes) {
                        Ok(s) => s,
                        Err(e) => unsafe { String::from_utf8_unchecked(e.into_bytes()) },
                    };
                    (s, h_arc)
                } else {
                    match c.get(&on) {
                        Some(s) => (s.to_string(), None),
                        None => return Node::Done,
                    }
                };
            let mut hits: Vec<Node<Cursor>> = Vec::new();
            for caps in re.captures_iter(&haystack) {
                let m0 = caps.get(0).unwrap();
                let mut child = c.clone();
                if let Some(arc) = &content_hash_arc {
                    child.set_arc("CONTENT_HASH", arc.clone());
                }
                child.set("LO", (m0.start() as u64).to_string());
                child.set("HI", (m0.end()   as u64).to_string());
                if want_match { child.set("MATCH", m0.as_str()); }
                for (i, name) in capture_names.iter().enumerate() {
                    if let Some(g) = caps.get(i + 1) {
                        child.set(name, g.as_str());
                    }
                }
                hits.push(Node::Emit(Arc::new(child)));
            }
            if hits.is_empty() { Node::Done } else { Node::Many(hits) }
        })
    }
}

/// Brute-force IN-batch fact-read with bound (`key_term`) and unbound
/// (`project`) terms. Per dispatch batch:
///   1. collect distinct key_term values
///   2. one `store.read_in` call
///   3. group rows by key_term
///   4. cross-product cursor × matching rows
///
/// N input cursors with M avg matches each → 1 query, N × M outputs.
pub struct FactReadComponent {
    store:    Arc<dyn Store>,
    fact:     String,
    key_term: String,
    project:  Vec<String>,
}
impl FactReadComponent {
    pub fn new(
        store:    Arc<dyn Store>,
        fact:     impl Into<String>,
        key_term: impl Into<String>,
        project:  &[&str],
    ) -> Self {
        Self {
            store, fact: fact.into(), key_term: key_term.into(),
            project: project.iter().map(|s| s.to_string()).collect(),
        }
    }
}
impl Component for FactReadComponent {
    type Next = Cursor;
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        use std::collections::{HashMap, HashSet};
        let mut keys: Vec<String> = Vec::with_capacity(batch.len());
        let mut seen: HashSet<String> = HashSet::new();
        for c in batch {
            if let Some(v) = c.get(&self.key_term) {
                if seen.insert(v.to_string()) { keys.push(v.to_string()); }
            }
        }
        let store = self.store.clone();
        let fact  = self.fact.clone();
        let key_term = self.key_term.clone();
        let project  = self.project.clone();
        let handle = tokio::runtime::Handle::current();
        let rows: Vec<Cursor> = tokio::task::block_in_place(|| {
            handle.block_on(store.read_in(&fact, &key_term, keys, project))
        });
        let mut by_key: HashMap<String, Vec<Cursor>> = HashMap::new();
        for r in rows {
            if let Some(k) = r.get(&self.key_term) {
                by_key.entry(k.to_string()).or_default().push(r);
            }
        }
        let mut out: Vec<Node<Cursor>> = Vec::new();
        for c in batch {
            let Some(k) = c.get(&self.key_term) else {
                out.push(Node::Done);
                continue;
            };
            let Some(matches) = by_key.get(k) else {
                out.push(Node::Done);
                continue;
            };
            let hits: Vec<Node<Cursor>> = matches.iter().map(|m| {
                let mut child = (*c).clone();
                for col in &self.project {
                    if let Some(v) = m.get(col) { child.set(col, v); }
                }
                Node::Emit(Arc::new(child))
            }).collect();
            out.push(if hits.is_empty() { Node::Done } else { Node::Many(hits) });
        }
        out
    }
}

/// Sh modes — same surface as v4 native.
#[derive(Debug, Clone, Copy)]
pub enum ShMode { EmitLines, FilterByExit, CaptureStdout }

fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' { out.push_str("'\\''"); } else { out.push(ch); }
    }
    out.push('\'');
    out
}
fn render_template(template: &str, cur: &Cursor) -> String {
    let re = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap();
    re.replace_all(template, |caps: &regex::Captures| {
        shell_quote(cur.get(&caps[1]).unwrap_or(""))
    }).into_owned()
}

/// Run `sh -c <rendered>` per input cursor; act on stdout per `mode`.
/// Substrate uses sync `std::process::Command` (rayon par_render gives
/// per-batch parallelism). Tokio not needed.
pub struct ShComponent {
    cmd_template: String,
    mode:         ShMode,
    bind:         Option<String>,
}
impl ShComponent {
    pub fn emit_lines(cmd: &str) -> Self {
        Self { cmd_template: cmd.into(), mode: ShMode::EmitLines, bind: None }
    }
    pub fn filter(cmd: &str) -> Self {
        Self { cmd_template: cmd.into(), mode: ShMode::FilterByExit, bind: None }
    }
    pub fn capture(cmd: &str, bind: &str) -> Self {
        Self { cmd_template: cmd.into(), mode: ShMode::CaptureStdout, bind: Some(bind.into()) }
    }
}
impl Component for ShComponent {
    type Next = Cursor;
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let template = self.cmd_template.clone();
        let mode     = self.mode;
        let bind     = self.bind.clone();
        par_render(batch, move |c| {
            let cmd = render_template(&template, c);
            let res = std::process::Command::new("sh")
                .arg("-c").arg(&cmd)
                .output();
            let Ok(o) = res else { return Node::Done };
            match mode {
                ShMode::EmitLines => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let hits: Vec<Node<Cursor>> = stdout.split('\n')
                        .filter(|l| !l.is_empty())
                        .map(|line| {
                            let mut child = c.clone();
                            child.set("LINE", line);
                            Node::Emit(Arc::new(child))
                        }).collect();
                    if hits.is_empty() { Node::Done } else { Node::Many(hits) }
                }
                ShMode::FilterByExit => {
                    if o.status.success() { Node::Emit(Arc::new(c.clone())) } else { Node::Done }
                }
                ShMode::CaptureStdout => {
                    let stdout = String::from_utf8_lossy(&o.stdout).trim_end().to_string();
                    let mut child = c.clone();
                    if let Some(b) = &bind { child.set(b, stdout.as_str()); }
                    Node::Emit(Arc::new(child))
                }
            }
        })
    }
}

/// `sh!` — author-declared impure shell. Per-cursor cmd run, stdout
/// dropped, cursor passes through. Never cacheable.
pub struct ShBangComponent { cmd_template: String }
impl ShBangComponent {
    pub fn new(cmd: &str) -> Self { Self { cmd_template: cmd.into() } }
}
impl Component for ShBangComponent {
    type Next = Cursor;
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let template = self.cmd_template.clone();
        par_render(batch, move |c| {
            let cmd = render_template(&template, c);
            let _ = std::process::Command::new("sh")
                .arg("-c").arg(&cmd)
                .status();
            Node::Emit(Arc::new(c.clone()))
        })
    }
}

/// Seed source: emit a fixed Vec<Cursor> once. Useful as a chain head
/// when the first real op is per-cursor but you want it to fire once.
pub struct SeedComponent { pub rows: Vec<Cursor> }
impl SeedComponent {
    pub fn one() -> Self { Self { rows: vec![Cursor::default()] } }
    pub fn new(rows: Vec<Cursor>) -> Self { Self { rows } }
}
impl Component for SeedComponent {
    type Next = Cursor;
    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let parent = match rows.first() { Some(r) => r, None => return };
        let many: Vec<Node<Cursor>> = self.rows.iter()
            .map(|c| Node::Emit(Arc::new(c.clone())))
            .collect();
        if many.is_empty() { return; }
        splice_into_at(parent, Node::Many(many), ctx.depth + 1, ctx.expand_tick, queue, 0);
    }
}

/// Multi-root walker. For each `(slug, root)`, walks files matching
/// `exts` and emits a Cursor with `REPO=slug` and `FS=path`. Streams
/// flushes across roots using `next_idx` to keep child paths unique.
/// Optional Interner shares Arc<str> heap across emitted REPO/FS terms.
pub struct RepoComponent {
    roots:    Vec<(String, PathBuf)>,
    exts:     Vec<String>,
    batch:    usize,
    interner: Option<Arc<Interner>>,
}
impl RepoComponent {
    pub fn new(roots: Vec<(String, PathBuf)>, exts: Vec<String>) -> Self {
        Self { roots, exts, batch: 4096, interner: None }
    }
    pub fn auto(roots: &[PathBuf], exts: Vec<String>) -> Self {
        let labelled: Vec<(String, PathBuf)> = roots.iter().map(|p| {
            let slug = p.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p.display().to_string());
            (slug, p.clone())
        }).collect();
        Self::new(labelled, exts)
    }
    pub fn with_batch(mut self, n: usize) -> Self { self.batch = n; self }
    pub fn with_interner(mut self, i: Arc<Interner>) -> Self {
        self.interner = Some(i); self
    }
}
impl Component for RepoComponent {
    type Next = Cursor;
    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let parent = match rows.first() { Some(r) => r, None => return };
        let mut buf: Vec<Node<Cursor>> = Vec::with_capacity(self.batch);
        let mut next_idx: u32 = 0;
        let mut flush = |buf: &mut Vec<Node<Cursor>>, next_idx: &mut u32| {
            if buf.is_empty() { return; }
            let many = Node::Many(std::mem::take(buf));
            let n = splice_into_at(parent, many, ctx.depth + 1, ctx.expand_tick, queue, *next_idx);
            *next_idx += n as u32;
        };

        for (slug, root) in &self.roots {
            let slug_arc: Arc<str> = match &self.interner {
                Some(i) => i.intern(slug),
                None    => Arc::from(slug.as_str()),
            };
            for entry in WalkBuilder::new(root).hidden(true).git_ignore(false).build() {
                let Ok(e) = entry else { continue };
                if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
                let p = e.into_path();
                let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
                if !self.exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }
                let mut c = Cursor::default();
                c.set_arc("REPO", slug_arc.clone());
                let fs_arc: Arc<str> = match &self.interner {
                    Some(i) => i.intern(&p.display().to_string()),
                    None    => Arc::from(p.display().to_string().as_str()),
                };
                c.set_arc("FS", fs_arc);
                buf.push(Node::Emit(Arc::new(c)));
                if buf.len() >= self.batch { flush(&mut buf, &mut next_idx); }
            }
        }
        flush(&mut buf, &mut next_idx);
    }
}

/// Per-input-cursor: read the path at `term`, walk it, emit one child
/// Cursor per matching file with `REPO=basename(term)` + `FS=path`.
/// Preserves all upstream terms on each emitted child.
pub struct RepoFromTermComponent {
    term:     String,
    exts:     Vec<String>,
    interner: Option<Arc<Interner>>,
}
impl RepoFromTermComponent {
    pub fn new(term: impl Into<String>, exts: Vec<String>) -> Self {
        Self { term: term.into(), exts, interner: None }
    }
    pub fn with_interner(mut self, i: Arc<Interner>) -> Self {
        self.interner = Some(i); self
    }
}
impl Component for RepoFromTermComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let Some(root_str) = c.get(&self.term) else { return Node::Done };
        let root = PathBuf::from(root_str);
        let slug = root.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| root.display().to_string());
        let slug_arc: Arc<str> = match &self.interner {
            Some(i) => i.intern(&slug),
            None    => Arc::from(slug.as_str()),
        };
        let mut hits: Vec<Node<Cursor>> = Vec::new();
        for entry in WalkBuilder::new(&root).hidden(true).git_ignore(false).build() {
            let Ok(e) = entry else { continue };
            if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
            let p = e.into_path();
            let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
            if !self.exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }
            let mut child = c.clone();
            child.set_arc("REPO", slug_arc.clone());
            let fs_arc: Arc<str> = match &self.interner {
                Some(i) => i.intern(&p.display().to_string()),
                None    => Arc::from(p.display().to_string().as_str()),
            };
            child.set_arc("FS", fs_arc);
            hits.push(Node::Emit(Arc::new(child)));
        }
        if hits.is_empty() { Node::Done } else { Node::Many(hits) }
    }
}

/// Render a `{TERM}` template per cursor and println! the result.
/// Side-effect terminal — emits `Done` after printing. (v4 native uses
/// the Hooks effect channel; the bench's only consumer is a sink that
/// drains it, so a direct println is equivalent for substrate scope.)
pub struct PrintComponent { pub template: String }
impl PrintComponent {
    pub fn new(template: impl Into<String>) -> Self { Self { template: template.into() } }
}
impl Component for PrintComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let mut s = self.template.clone();
        for (n, v) in &c.terms {
            s = s.replace(&format!("{{{}}}", n), v);
        }
        println!("{}", s);
        Node::Done
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   tests — smoke for each ported Component
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use effect_runtime::v2::{
        expand, ExpandOpts, MemQueue, PipeInstance, QueueBackend,
    };

    struct Collector { sink: Arc<Mutex<Vec<Cursor>>> }
    impl Component for Collector {
        type Next = Cursor;
        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.sink.lock().unwrap().push(c.clone());
            Node::Done
        }
    }

    fn run_pipe(
        ops:  Vec<Arc<dyn Component<Next = Cursor>>>,
        seed: Vec<Cursor>,
    ) -> Vec<Cursor> {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let mut steps = ops;
        steps.push(Arc::new(Collector { sink: sink.clone() }));
        let pipe = PipeInstance::new(steps);
        let seed: Vec<Arc<Cursor>> = seed.into_iter().map(Arc::new).collect();
        expand(&pipe, queue, seed, ExpandOpts::default().with_batch_cap(64));
        let v = sink.lock().unwrap().clone();
        v
    }

    fn cur(terms: &[(&str, &str)]) -> Cursor {
        let mut c = Cursor::default();
        for (n, v) in terms { c.set(n, *v); }
        c
    }

    #[test]
    fn split_emits_one_child_per_non_empty_piece() {
        let out = run_pipe(
            vec![Arc::new(SplitComponent::new("RAW", ",", "X"))],
            vec![cur(&[("RAW", "a,b,,c")])],
        );
        let xs: Vec<&str> = out.iter().filter_map(|c| c.get("X")).collect();
        assert_eq!(xs, vec!["a", "b", "c"]);
    }

    #[test]
    fn format_renders_template_with_terms() {
        let out = run_pipe(
            vec![Arc::new(FormatComponent::new("hello ${X}", "MSG"))],
            vec![cur(&[("X", "world")])],
        );
        assert_eq!(out[0].get("MSG"), Some("hello world"));
    }

    #[test]
    fn trim_in_place_strips_whitespace() {
        let out = run_pipe(
            vec![Arc::new(TrimComponent::in_place("S"))],
            vec![cur(&[("S", "  hi  \n")])],
        );
        assert_eq!(out[0].get("S"), Some("hi"));
    }

    #[test]
    fn basename_dirname_split_path() {
        let out = run_pipe(
            vec![
                Arc::new(BasenameComponent::new("FS", "FILE")),
                Arc::new(DirnameComponent::new("FS", "DIR")),
            ],
            vec![cur(&[("FS", "/a/b/file.rs")])],
        );
        assert_eq!(out[0].get("FILE"), Some("file.rs"));
        assert_eq!(out[0].get("DIR"),  Some("/a/b"));
    }

    #[test]
    fn line_splits_text_by_newline() {
        let out = run_pipe(
            vec![Arc::new(LineComponent::on("TEXT").into_term("L"))],
            vec![cur(&[("TEXT", "alpha\nbeta\n\ngamma")])],
        );
        let ls: Vec<&str> = out.iter().filter_map(|c| c.get("L")).collect();
        assert_eq!(ls, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn re_matches_text_term() {
        let out = run_pipe(
            vec![
                Arc::new(ReComponent::new(r"(\w+)=(\d+)", &["K", "V"])
                    .on_term("KV")
                    .with_match_text(false)),
            ],
            vec![cur(&[("KV", "x=1 y=22")])],
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].get("K"), Some("x"));
        assert_eq!(out[0].get("V"), Some("1"));
        assert_eq!(out[1].get("K"), Some("y"));
        assert_eq!(out[1].get("V"), Some("22"));
    }

    #[test]
    fn seed_emits_fixed_rows_once() {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(SeedComponent::new(vec![cur(&[("X", "1")]), cur(&[("X", "2")])]))
                as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);
        // Seed-shape needs ONE bootstrap row; the Component ignores its
        // value and emits self.rows.
        expand(&pipe, queue, vec![Arc::new(Cursor::default())], ExpandOpts::default());
        let v = sink.lock().unwrap();
        let xs: Vec<&str> = v.iter().filter_map(|c| c.get("X")).collect();
        assert_eq!(xs, vec!["1", "2"]);
    }

    #[test]
    fn sh_capture_stdout_binds_to_term() {
        let out = run_pipe(
            vec![Arc::new(ShComponent::capture("printf alpha", "OUT"))],
            vec![Cursor::default()],
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].get("OUT"), Some("alpha"));
    }

    #[test]
    fn sh_filter_by_exit_drops_failed() {
        let out = run_pipe(
            vec![Arc::new(ShComponent::filter("false"))],
            vec![Cursor::default()],
        );
        assert!(out.is_empty());
    }
}

