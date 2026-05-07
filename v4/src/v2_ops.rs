//! Three Components for v4-bench's `v2` path. Bare mode
//! only (Fs > AstNm > Count). Smallest-viable port: no telemetry, no
//! interner, no Layer-2 / Layer-3, no Store. The point is an A/B
//! perf number against the native v4 stream pipeline.
//!
//! Wire from v4_bench.rs:
//!
//! ```ignore
//! use effect_runtime::v2::*;
//! use v4::v2_ops::{FsComponent, AstNmComponent, CountComponent};
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
    par_render, splice_into_at, Component, Node, QueueBackend,
    QueueRow, RenderCtx,
};
use ignore::WalkBuilder;

use crate::config::SprfConfig;
use crate::store::SprfStore;
use crate::{Coord, Cursor, Interner};

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
///
/// PERF TODO (~200ms gain on linux/63k files): split the walk onto a
/// dedicated thread so AstNm starts batching in parallel with the
/// directory scan. Also add a `(path, mtime, size) → content_hash`
/// fast-path: warm walks short-circuit the file read entirely when the
/// triple matches. Independent of the v2 pipeline; pure source-side
/// optimization.
pub struct FsComponent {
    pub root:  PathBuf,
    pub exts:  Vec<String>,
    pub batch: usize,
    /// Layer 0c.2 — content-derived intern store. When `Some`, each
    /// emitted Cursor also stamps the FS term in coord-space via
    /// `set_at` alongside the legacy `raw_terms` write. SYNTHETIC
    /// coord because no file content has been read yet at source-op
    /// time; the path string still dedupes through `_strings`.
    store:     Option<Arc<SprfStore>>,
}

impl FsComponent {
    pub fn new(root: PathBuf, exts: Vec<String>, batch: usize) -> Self {
        Self { root, exts, batch, store: None }
    }
    /// Attach the intern store for coord-space stamping.
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
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
            if !self.exts.is_empty() {
                let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
                if !self.exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }
            }

            let path_str = p.display().to_string();
            let mut c = Cursor::default();
            c.set("FS", path_str.as_str());
            // Layer 0c.2 — coord-space mate for the FS term. Path-only
            // (no content read yet); coord stays SYNTHETIC, but the path
            // string interns into `_strings`.
            if let Some(store) = &self.store {
                c.set_synthetic("FS", path_str.as_str(), store);
            }
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
    /// Layer 0c.2 — content-derived intern store. When `Some`, each
    /// match cursor stamps focal `value_id`/`at` and per-match coord
    /// terms alongside legacy `LO`/`HI`/raw_terms writes.
    store:   Option<Arc<SprfStore>>,
}

impl AstNmComponent {
    pub fn new(pat_src: String, lang: SupportLang) -> Self {
        let pattern = Arc::new(Pattern::new(&pat_src, lang));
        let fixed: Arc<str> = Arc::from(pattern.fixed_string().to_string().as_str());
        Self { lang, pattern, fixed, store: None }
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
}

impl Component for AstNmComponent {
    type Next = Cursor;

    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let store = self.store.clone();
        par_render(batch, move |c| {
            let Some(path) = c.get("FS") else { return Node::Done };
            let Ok(bytes)  = std::fs::read(path) else { return Node::Done };
            // tree-sitter takes any bytes; UTF-8 errors → ERROR nodes.
            // Layer 0c.2 — intern file (content-keyed) once per parsed
            // file when a store is attached; reuse the FileId across
            // every match cursor emitted from this parse.
            let file_id = store.as_ref()
                .map(|s| s.intern_file(&bytes, path))
                .unwrap_or(0);
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
                if let Some(store) = &store {
                    let coord = Coord {
                        repo: 0, rev: 0, fs: file_id,
                        lo: r.start as u32, hi: r.end as u32,
                    };
                    let slice = src.get(r.start..r.end).unwrap_or("");
                    // Focal coord identifies the match itself.
                    child.at = store.intern_ref(coord);
                    child.value_id = store.intern_string(slice);
                    // Per-match coord term — `MATCH.at`/`MATCH.value`
                    // reach through one Term entry instead of LO/HI cols.
                    child.set_at("MATCH", slice, coord, store);
                    // Term-side coord projection for ${MATCH.lo}/${MATCH.hi}/${MATCH.fs}.
                    child.set("MATCH_LO", coord.lo.to_string());
                    child.set("MATCH_HI", coord.hi.to_string());
                    if let Some(path) = store.path_of(coord.fs) {
                        child.set("MATCH_FS", &*path);
                    }
                }
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
    /// Layer 0c.2 — content-derived intern store. See `AstNmComponent`.
    store:    Option<Arc<SprfStore>>,
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
        Self { lang, patterns: Arc::new(pats), store: None }
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
}

impl Component for MultiAstNmComponent {
    type Next = Cursor;

    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let pats = self.patterns.clone();
        let lang = self.lang;
        let store = self.store.clone();
        par_render(batch, move |c| {
            let Some(path) = c.get("FS") else { return Node::Done };
            let Ok(bytes)  = std::fs::read(path) else { return Node::Done };
            let file_id = store.as_ref()
                .map(|s| s.intern_file(&bytes, path))
                .unwrap_or(0);
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
                    if let Some(store) = &store {
                        let coord = Coord {
                            repo: 0, rev: 0, fs: file_id,
                            lo: r.start as u32, hi: r.end as u32,
                        };
                        let slice = src.get(r.start..r.end).unwrap_or("");
                        child.at = store.intern_ref(coord);
                        child.value_id = store.intern_string(slice);
                        child.set_at("MATCH", slice, coord, store);
                        // Term-side coord projection for ${MATCH.lo}/${MATCH.hi}/${MATCH.fs}.
                        child.set("MATCH_LO", coord.lo.to_string());
                        child.set("MATCH_HI", coord.hi.to_string());
                        if let Some(path) = store.path_of(coord.fs) {
                            child.set("MATCH_FS", &*path);
                        }
                        // PAT label is synthetic (no source coord; it
                        // names the pattern not a span of source).
                        child.set_synthetic("PAT", label.as_ref(), store);
                    }
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
    /// Layer 0c.2 — content-derived intern store.
    store:    Option<Arc<SprfStore>>,
}

impl SinglePathComponent {
    pub fn new(path: PathBuf) -> Self { Self { path, store: None } }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
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
        let path_str = self.path.display().to_string();
        let mut c = Cursor::default();
        c.set("FS", path_str.as_str());
        if let Some(store) = &self.store {
            c.set_synthetic("FS", path_str.as_str(), store);
        }
        let node = Node::Emit(Arc::new(c));
        splice_into_at(parent, node, ctx.depth + 1, ctx.expand_tick, queue, 0);
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

/// Split a string by `sep`, emit one child cursor per non-empty piece.
///   from = None  → reads `cursor.value`
///   from = Some  → reads named term
///   into = None  → writes `cursor.value`
///   into = Some  → also writes named term (value is rebound regardless)
pub struct SplitComponent { pub from: Option<String>, pub sep: String, pub into: Option<String> }
impl SplitComponent {
    pub fn new(from: &str, sep: &str, into: &str) -> Self {
        Self { from: Some(from.into()), sep: sep.into(), into: Some(into.into()) }
    }
    pub fn on_value(sep: &str) -> Self {
        Self { from: None, sep: sep.into(), into: None }
    }
    pub fn from_term(mut self, name: &str) -> Self { self.from = Some(name.into()); self }
    pub fn into_term(mut self, name: &str) -> Self { self.into = Some(name.into()); self }
}
impl Component for SplitComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let src: &str = match &self.from {
            Some(name) => match c.get(name) { Some(s) => s, None => return Node::Done },
            None       => &c.value,
        };
        let out: Vec<Cursor> = src.split(self.sep.as_str())
            .filter(|p| !p.is_empty())
            .map(|p| {
                let mut k = c.clone();
                k.value = Arc::<str>::from(p);
                if let Some(into) = &self.into { k.set(into, p); }
                k
            })
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
    /// Layer 0c.2 — content-derived intern store.
    store:         Option<Arc<SprfStore>>,
}
impl ReComponent {
    pub fn new(pat: &str, captures: &[&str]) -> Self {
        Self {
            on:            "FS".to_string(),
            capture_names: captures.iter().map(|s| s.to_string()).collect(),
            want_match:    true,
            re:            Arc::new(regex::Regex::new(pat).expect("compile regex")),
            interner:      None,
            store:         None,
        }
    }
    pub fn on_term(mut self, term: &str) -> Self { self.on = term.to_string(); self }
    pub fn with_match_text(mut self, on: bool) -> Self { self.want_match = on; self }
    pub fn with_interner(mut self, interner: Arc<Interner>) -> Self {
        self.interner = Some(interner); self
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
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
        let store = self.store.clone();
        par_render(batch, move |c| {
            // file_id stays SYNTHETIC unless we actually read a file
            // (on == FS). Term matches work on cursor.value text and
            // don't carry a coord-fs on the parent today.
            let mut file_id: crate::FileId = 0;
            let (haystack, content_hash_arc): (String, Option<Arc<str>>) =
                if on == "FS" {
                    let Some(path) = c.get("FS") else { return Node::Done };
                    let Ok(bytes) = std::fs::read(path) else { return Node::Done };
                    let h_hex = blake3::hash(&bytes).to_hex().to_string();
                    let h_arc: Option<Arc<str>> = interner.as_ref().map(|i| i.intern(&h_hex));
                    if let Some(s) = &store {
                        file_id = s.intern_file(&bytes, path);
                    }
                    let s_text = match String::from_utf8(bytes) {
                        Ok(s) => s,
                        Err(e) => unsafe { String::from_utf8_unchecked(e.into_bytes()) },
                    };
                    (s_text, h_arc)
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
                if let Some(store) = &store {
                    let match_coord = Coord {
                        repo: 0, rev: 0, fs: file_id,
                        lo: m0.start() as u32, hi: m0.end() as u32,
                    };
                    child.at = store.intern_ref(match_coord);
                    child.value_id = store.intern_string(m0.as_str());
                    if want_match {
                        child.set_at("MATCH", m0.as_str(), match_coord, store);
                        // Term-side coord projection for ${MATCH.lo}/${MATCH.hi}/${MATCH.fs}.
                        child.set("MATCH_LO", match_coord.lo.to_string());
                        child.set("MATCH_HI", match_coord.hi.to_string());
                        if let Some(path) = store.path_of(match_coord.fs) {
                            child.set("MATCH_FS", &*path);
                        }
                    }
                    if let Some(arc) = &content_hash_arc {
                        // CONTENT_HASH is per-file metadata, not a span;
                        // synthetic so it dedupes globally.
                        child.set_synthetic("CONTENT_HASH", arc.as_ref(), store);
                    }
                    for (i, name) in capture_names.iter().enumerate() {
                        if let Some(g) = caps.get(i + 1) {
                            let group_coord = Coord {
                                repo: 0, rev: 0, fs: file_id,
                                lo: g.start() as u32, hi: g.end() as u32,
                            };
                            child.set_at(name, g.as_str(), group_coord, store);
                            // Term-side coord projection for ${<name>.lo}/${<name>.hi}/${<name>.fs}.
                            child.set(&format!("{}_LO", name), group_coord.lo.to_string());
                            child.set(&format!("{}_HI", name), group_coord.hi.to_string());
                            if let Some(path) = store.path_of(group_coord.fs) {
                                child.set(&format!("{}_FS", name), &*path);
                            }
                        }
                    }
                }
                hits.push(Node::Emit(Arc::new(child)));
            }
            if hits.is_empty() { Node::Done } else { Node::Many(hits) }
        })
    }
}

/// Sh modes. EmitLines retained for back-compat but the language
/// surface no longer exposes it: `sh\`cmd\` > split\`\n\`` is the
/// idiomatic combo.
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
    // Layer 0c.3 — accept the four host carveout forms:
    //   ${X}       ${X?}      ${X.field}      ${&.field}
    // The regex captures the full inner key (stem + optional `?` + optional
    // `.field`); the lookup helper builds the Cursor::get-shaped key.
    let re = regex::Regex::new(
        r"\$\{(&\.[A-Za-z_][A-Za-z0-9_]*|[A-Za-z_][A-Za-z0-9_]*\??(?:\.[A-Za-z_][A-Za-z0-9_]*)?)\}"
    ).unwrap();
    re.replace_all(template, |caps: &regex::Captures| {
        let raw = &caps[1];
        // Strip Bind-mode `?` from a bare `X?` for the lookup; field-form
        // `X?.field` is illegal upstream and won't reach here, but defend
        // by routing `X?` → `X` lookup (treating bind-at-render as read).
        let key: std::borrow::Cow<'_, str> = match raw.split_once('.') {
            None => match raw.strip_suffix('?') { Some(s) => s.into(), None => raw.into() },
            Some(_) => raw.into(),
        };
        shell_quote(cur.get(&key).unwrap_or(""))
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
    /// Capture stdout into `cursor.value` only (no named term). Use
    /// when the next op operates on `&.value` (e.g. `> split\`\n\``).
    pub fn capture_value(cmd: &str) -> Self {
        Self { cmd_template: cmd.into(), mode: ShMode::CaptureStdout, bind: None }
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
                    // Always rebind cursor.value so `> split` and
                    // friends can chain off &.value with no named term.
                    child.value = Arc::<str>::from(stdout.as_str());
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

/// `repo` op modes.
///
/// Args form (`Roots`) walks the filesystem under each `(slug, root)`
/// pair as today. Bare form (`FromConfig`) is the Layer 5a generator:
/// emit one synthetic cursor per `RepoConfig` entry — no walk, no read.
/// The bare form is the canonical "all repos sprefa knows about" source
/// per the user-locked design; downstream ops (`> rev() > fs()`) do
/// the actual filesystem work in 5b.
pub enum RepoMode {
    /// Args form: explicit (slug, root) pairs walked as today.
    Roots(Vec<(String, PathBuf)>),
    /// Bare form: one cursor per configured repo. No filesystem walk.
    FromConfig,
}

/// Multi-source repo op. Two shapes:
///
///  1. `repo(:slug, :path) ...` — walks the filesystem under each pair
///     and emits a cursor with `REPO=slug` and `FS=path` per matching
///     file. Behavior identical to pre-Layer-5a.
///  2. `repo()` — reads `LowerCtx::config` (XDG repos.toml) and emits
///     one synthetic cursor per `RepoConfig` entry: `cursor.value =
///     slug`, `REPO` term = slug, `Coord { repo: intern_repo(slug, ""),
///     rest: 0 }`. No fs walk; the cursor is the seed for downstream
///     `> rev() > fs()` chains.
pub struct RepoComponent {
    mode:     RepoMode,
    exts:     Vec<String>,
    batch:    usize,
    interner: Option<Arc<Interner>>,
    /// Layer 0c.2 — content-derived intern store.
    store:    Option<Arc<SprfStore>>,
    /// Layer 5a — config handle for the FromConfig generator. Ignored
    /// in the Roots variant.
    config:   Option<Arc<SprfConfig>>,
}
impl RepoComponent {
    pub fn new(roots: Vec<(String, PathBuf)>, exts: Vec<String>) -> Self {
        Self {
            mode: RepoMode::Roots(roots),
            exts, batch: 4096,
            interner: None, store: None, config: None,
        }
    }
    /// Bare-form ctor: no roots, no walk. The cursor stream is driven
    /// by `LowerCtx::config` plumbed in via `with_config`.
    pub fn from_config() -> Self {
        Self {
            mode: RepoMode::FromConfig,
            exts: Vec::new(), batch: 4096,
            interner: None, store: None, config: None,
        }
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
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
    pub fn with_config(mut self, c: Arc<SprfConfig>) -> Self {
        self.config = Some(c); self
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

        match &self.mode {
            RepoMode::FromConfig => {
                let Some(cfg) = self.config.as_ref() else { return };
                for entry in &cfg.repos {
                    if entry.slug.is_empty() { continue; }
                    let slug_arc: Arc<str> = match &self.interner {
                        Some(i) => i.intern(&entry.slug),
                        None    => Arc::from(entry.slug.as_str()),
                    };
                    let mut c = Cursor::default();
                    // Legacy bare-string surface: REPO term + cursor.value
                    // = slug so `&.value` and `${REPO}` both resolve.
                    c.value = slug_arc.clone();
                    c.set_arc("REPO", slug_arc.clone());
                    if let Some(store) = &self.store {
                        let repo_id = store.intern_repo(&entry.slug, "");
                        let coord = Coord {
                            repo: repo_id, rev: 0, fs: 0, lo: 0, hi: 0,
                        };
                        c.value_id = crate::StringId::of(slug_arc.as_ref());
                        c.at       = store.intern_ref(coord);
                        c.set_synthetic("REPO", slug_arc.as_ref(), store);
                    }
                    buf.push(Node::Emit(Arc::new(c)));
                    if buf.len() >= self.batch { flush(&mut buf, &mut next_idx); }
                }
                flush(&mut buf, &mut next_idx);
                return;
            }
            RepoMode::Roots(_) => { /* fall through */ }
        }

        let RepoMode::Roots(roots) = &self.mode else { unreachable!() };
        for (slug, root) in roots {
            let slug_arc: Arc<str> = match &self.interner {
                Some(i) => i.intern(slug),
                None    => Arc::from(slug.as_str()),
            };
            for entry in WalkBuilder::new(root).hidden(true).git_ignore(false).build() {
                let Ok(e) = entry else { continue };
                if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
                let p = e.into_path();
                if !self.exts.is_empty() {
                    let Some(ext) = p.extension().and_then(|s| s.to_str()) else { continue };
                    if !self.exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }
                }
                let path_str = p.display().to_string();
                let mut c = Cursor::default();
                c.set_arc("REPO", slug_arc.clone());
                let fs_arc: Arc<str> = match &self.interner {
                    Some(i) => i.intern(&path_str),
                    None    => Arc::from(path_str.as_str()),
                };
                c.set_arc("FS", fs_arc);
                if let Some(store) = &self.store {
                    // Repo and FS are pre-file metadata at this stage —
                    // no content read yet, so coord stays SYNTHETIC.
                    // Both strings still intern through `_strings`.
                    c.set_synthetic("REPO", slug_arc.as_ref(), store);
                    c.set_synthetic("FS", path_str.as_str(), store);
                }
                buf.push(Node::Emit(Arc::new(c)));
                if buf.len() >= self.batch { flush(&mut buf, &mut next_idx); }
            }
        }
        flush(&mut buf, &mut next_idx);
    }
}

/// Layer 5b.1 — `rev` op. Reads each upstream cursor's `Coord.repo`,
/// resolves a list of revspecs (`:HEAD`, `:HEAD~5`, `:main`, `:v1.0`,
/// …) against the `git2::Repository` at the configured `(slug, root)`,
/// and emits one child cursor per (upstream cursor × revspec).
///
/// libgit2 is the right tool here: `revparse_single` is an O(1) hot
/// path. The blob-walking ban (memory: project_v4_git_blob_walk_uses_shell)
/// targets tree iteration, which lives in 5b.2 (shell `ls-tree`).
///
/// Bare form `rev()` defaults to `[":HEAD"]`. Multi-spec form
/// `rev(:HEAD, :HEAD~1)` emits two cursors per upstream repo cursor.
///
/// Output cursor:
///   - `cursor.value`    = oid hex (legacy Arc<str>)
///   - `cursor.value_id` = StringId::of(oid hex)
///   - `cursor.at`       = intern_ref(Coord { repo: parent.repo, rev,
///                                            fs: 0, lo: 0, hi: 0 })
///   - synthetic REV term = oid hex (when store attached)
///   - legacy raw_terms REV = oid hex (back-compat for `${REV}` reads)
pub struct RevComponent {
    /// Atom revspecs to resolve per upstream cursor. Empty in the
    /// constructor is sugared to `["HEAD"]` so bare `rev()` defaults
    /// to HEAD.
    revspecs: Vec<String>,
    config:   Option<Arc<SprfConfig>>,
    store:    Option<Arc<SprfStore>>,
    /// Cache opened repos by slug. `git2::Repository` is `Send + !Sync`;
    /// hence `Mutex`. The outer `Mutex<HashMap>` guards the cache itself.
    repos: std::sync::Mutex<
        std::collections::HashMap<String, Arc<std::sync::Mutex<git2::Repository>>>,
    >,
}

impl RevComponent {
    pub fn new(revspecs: Vec<String>) -> Self {
        let revspecs = if revspecs.is_empty() {
            vec!["HEAD".to_string()]
        } else { revspecs };
        Self {
            revspecs, config: None, store: None,
            repos: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
    pub fn with_config(mut self, c: Arc<SprfConfig>) -> Self {
        self.config = Some(c); self
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }

    fn open(&self, slug: &str, root: &PathBuf) -> Option<Arc<std::sync::Mutex<git2::Repository>>> {
        let mut cache = self.repos.lock().unwrap();
        if let Some(r) = cache.get(slug) { return Some(r.clone()); }
        let repo = git2::Repository::open(root).ok()?;
        let arc  = Arc::new(std::sync::Mutex::new(repo));
        cache.insert(slug.to_string(), arc.clone());
        Some(arc)
    }
}

impl Component for RevComponent {
    type Next = Cursor;
    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let Some(store) = self.store.as_ref() else { return };
        let Some(cfg)   = self.config.as_ref() else { return };

        for parent in rows {
            // Resolve upstream Coord.repo via the intern store.
            let parent_coord = match store.coord_of(parent.value.at) {
                Some(c) => c,
                None    => continue,
            };
            let repo_id = parent_coord.repo;
            if repo_id == 0 { continue; }
            let (slug, _remote) = match store.lookup_repo(repo_id) {
                Some(m) => m,
                None    => continue,
            };
            let slug_str: &str = slug.as_ref();
            // Find (slug, root) in SprfConfig.
            let root: PathBuf = match cfg.repos.iter().find(|r| r.slug == slug_str) {
                Some(rc) => rc.root.clone(),
                None     => continue,
            };
            let Some(repo_arc) = self.open(slug_str, &root) else { continue };

            let mut emitted: Vec<Node<Cursor>> = Vec::with_capacity(self.revspecs.len());
            {
                let repo = repo_arc.lock().unwrap();
                for spec in &self.revspecs {
                    let obj = match repo.revparse_single(spec) {
                        Ok(o)  => o,
                        Err(_) => continue, // skip bad spec; no panic
                    };
                    let commit = match obj.peel_to_commit() {
                        Ok(c)  => c,
                        Err(_) => continue,
                    };
                    let oid_hex = commit.id().to_string();
                    let ts      = commit.time().seconds();
                    let rev_id  = store.intern_rev(repo_id, &oid_hex, ts);
                    let coord   = Coord {
                        repo: repo_id, rev: rev_id,
                        fs: 0, lo: 0, hi: 0,
                    };
                    let oid_arc: Arc<str> = Arc::from(oid_hex.as_str());
                    let mut child = parent.value.as_ref().clone();
                    child.value    = oid_arc.clone();
                    child.value_id = crate::StringId::of(&oid_hex);
                    child.at       = store.intern_ref(coord);
                    // Legacy raw_terms surface so `${REV}` reads still
                    // resolve through Cursor::get.
                    child.set_arc("REV", oid_arc.clone());
                    child.set_synthetic("REV", &oid_hex, store);
                    emitted.push(Node::Emit(Arc::new(child)));
                }
            }

            if emitted.is_empty() { continue; }
            let many = Node::Many(emitted);
            splice_into_at(parent, many, ctx.depth + 1, ctx.expand_tick, queue, 0);
        }
    }
}

/// Per-input-cursor: read the path at `term`, walk it, emit one child
/// Cursor per matching file with `REPO=basename(term)` + `FS=path`.
/// Preserves all upstream terms on each emitted child.
pub struct RepoFromTermComponent {
    term:     String,
    exts:     Vec<String>,
    interner: Option<Arc<Interner>>,
    /// Layer 0c.2 — content-derived intern store.
    store:    Option<Arc<SprfStore>>,
}
impl RepoFromTermComponent {
    pub fn new(term: impl Into<String>, exts: Vec<String>) -> Self {
        Self { term: term.into(), exts, interner: None, store: None }
    }
    pub fn with_interner(mut self, i: Arc<Interner>) -> Self {
        self.interner = Some(i); self
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
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
            let path_str = p.display().to_string();
            let mut child = c.clone();
            child.set_arc("REPO", slug_arc.clone());
            let fs_arc: Arc<str> = match &self.interner {
                Some(i) => i.intern(&path_str),
                None    => Arc::from(path_str.as_str()),
            };
            child.set_arc("FS", fs_arc);
            if let Some(store) = &self.store {
                child.set_synthetic("REPO", slug_arc.as_ref(), store);
                child.set_synthetic("FS", path_str.as_str(), store);
            }
            hits.push(Node::Emit(Arc::new(child)));
        }
        if hits.is_empty() { Node::Done } else { Node::Many(hits) }
    }
}

/// Render a `${TERM}` template per cursor, prepend optional prefix,
/// and println! the result. Cursor flows through unchanged — print is
/// a side-tap, not a sink. `template = None` prints a debug dump of
/// the cursor.terms (every binding) so `print()` with no args is
/// useful for ad-hoc tracing.
pub struct PrintComponent {
    pub template: Option<String>,
    pub prefix:   Option<String>,
}
impl PrintComponent {
    pub fn new(template: impl Into<String>) -> Self {
        Self { template: Some(template.into()), prefix: None }
    }
    pub fn dump() -> Self { Self { template: None, prefix: None } }
    pub fn with_prefix(mut self, p: impl Into<String>) -> Self {
        self.prefix = Some(p.into()); self
    }
}
impl Component for PrintComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let body = match &self.template {
            Some(t) => {
                let re = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap();
                re.replace_all(t, |caps: &regex::Captures| {
                    c.get(&caps[1]).unwrap_or("").to_string()
                }).into_owned()
            }
            None => {
                let mut s = String::new();
                for (i, (n, v)) in c.raw_terms.iter().enumerate() {
                    if i > 0 { s.push(' '); }
                    s.push_str(n); s.push('='); s.push_str(v);
                }
                s
            }
        };
        match &self.prefix {
            Some(p) => println!("{p}: {body}"),
            None    => println!("{body}"),
        }
        Node::Emit(Arc::new(c.clone()))
    }
}

/// `> void` — drop the cursor.
pub struct VoidComponent;
impl Component for VoidComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, _c: &Cursor) -> Node<Cursor> { Node::Done }
}

/// `> read` — load file bytes (UTF-8) at cursor.FS into cursor.value.
/// No-FS cursors pass through unchanged. UTF-8 errors fall through to
/// from_utf8_lossy so we never drop a row on encoding.
pub struct ReadComponent {
    /// Layer 0c.2 — content-derived intern store. When set, focal
    /// `value_id`/`at` stamp the file content + a whole-file coord.
    store: Option<Arc<SprfStore>>,
}
impl ReadComponent {
    pub fn new() -> Self { Self { store: None } }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
}
impl Default for ReadComponent {
    fn default() -> Self { Self::new() }
}
impl Component for ReadComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let Some(path) = c.get("FS") else { return Node::Emit(Arc::new(c.clone())) };
        let bytes = match std::fs::read(path) { Ok(b) => b, Err(_) => return Node::Done };
        let s = String::from_utf8_lossy(&bytes).into_owned();
        let mut child = c.clone();
        child.value = Arc::<str>::from(s.as_str());
        if let Some(store) = &self.store {
            // Focal coord covers the whole file (lo=0, hi=len).
            let file_id = store.intern_file(&bytes, path);
            let coord = Coord {
                repo: 0, rev: 0, fs: file_id,
                lo: 0, hi: bytes.len() as u32,
            };
            child.at = store.intern_ref(coord);
            child.value_id = store.intern_string(&s);
        }
        Node::Emit(Arc::new(child))
    }
}

/// `> comment(open_re, close_re?)` — narrow cursor.value to regions
/// bounded by comment markers. Reads `cursor.value` as the haystack.
/// Single-arg = sequential; two-arg = paired (LIFO nesting). Each
/// emitted cursor has LO/HI byte offsets stamped (relative to the
/// original value) and `value` rebound to the inner slice.
pub struct CommentComponent {
    open:  Arc<regex::Regex>,
    close: Option<Arc<regex::Regex>>,
    /// Layer 0c.2 — content-derived intern store.
    store: Option<Arc<SprfStore>>,
}
impl CommentComponent {
    pub fn sequential(open_re: &str) -> Result<Self, regex::Error> {
        Ok(Self { open: Arc::new(regex::Regex::new(open_re)?), close: None, store: None })
    }
    pub fn paired(open_re: &str, close_re: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            open:  Arc::new(regex::Regex::new(open_re)?),
            close: Some(Arc::new(regex::Regex::new(close_re)?)),
            store: None,
        })
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
}
impl Component for CommentComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let hay = c.value.clone();
        let opens: Vec<(usize, usize)> = self.open.find_iter(&hay)
            .map(|m| (m.start(), m.end())).collect();
        if opens.is_empty() { return Node::Done; }

        let regions: Vec<(usize, usize)> = match &self.close {
            None => {
                // Sequential: [open[i].end, open[i+1].start) — last → EOF.
                let mut out = Vec::with_capacity(opens.len());
                for (i, (_, oe)) in opens.iter().enumerate() {
                    let next_lo = opens.get(i + 1).map(|(s, _)| *s).unwrap_or(hay.len());
                    out.push((*oe, next_lo));
                }
                out
            }
            Some(cre) => {
                // Paired LIFO: walk opens + closes in source order, push opens,
                // pop on close → [open.end, close.start).
                let closes: Vec<(usize, usize)> = cre.find_iter(&hay)
                    .map(|m| (m.start(), m.end())).collect();
                let mut events: Vec<(usize, bool, usize)> = Vec::new(); // (pos, is_open, end)
                for (s, e) in &opens   { events.push((*s, true,  *e)); }
                for (s, e) in &closes  { events.push((*s, false, *e)); }
                events.sort_by_key(|e| e.0);
                let mut stack: Vec<usize> = Vec::new();
                let mut out: Vec<(usize, usize)> = Vec::new();
                for (pos, is_open, end) in events {
                    if is_open { stack.push(end); }
                    else if let Some(open_end) = stack.pop() {
                        out.push((open_end, pos));
                    }
                }
                // unpaired opens → collapse to the marker line itself
                while let Some(open_end) = stack.pop() {
                    out.push((open_end, open_end));
                }
                out
            }
        };

        let hits: Vec<Node<Cursor>> = regions.into_iter().map(|(lo, hi)| {
            let mut child = c.clone();
            child.set("LO", lo.to_string());
            child.set("HI", hi.to_string());
            let slice = &hay[lo..hi];
            child.value = Arc::<str>::from(slice);
            if let Some(store) = &self.store {
                // Comment operates on cursor.value, not a file. fs slot
                // inherits from the parent cursor's coord (synthetic if
                // unset). Lo/hi address bytes inside the value text.
                let parent_fs = store.coord_of(c.at).map(|c| c.fs).unwrap_or(0);
                let coord = Coord {
                    repo: 0, rev: 0, fs: parent_fs,
                    lo: lo as u32, hi: hi as u32,
                };
                child.at = store.intern_ref(coord);
                child.value_id = store.intern_string(slice);
                child.set_at("MATCH", slice, coord, store);
                // Term-side coord projection for ${MATCH.lo}/${MATCH.hi}/${MATCH.fs}.
                child.set("MATCH_LO", coord.lo.to_string());
                child.set("MATCH_HI", coord.hi.to_string());
                if let Some(path) = store.path_of(coord.fs) {
                    child.set("MATCH_FS", &*path);
                }
            }
            Node::Emit(Arc::new(child))
        }).collect();
        if hits.is_empty() { Node::Done } else { Node::Many(hits) }
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   Writers — reverse polarity. cursor → bytes on disk.
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteMode { Replace, Append, Prepend, Wrap }

/// `> write_cursor(mode?)` — splice `cursor.value` into file at
/// (FS, [LO, HI)). Aggregates per file and applies right-to-left so
/// multi-cursor splices stay correct (left edits would shift right
/// offsets). One file open + one write per (file, batch).
pub struct WriteCursorComponent { pub mode: WriteMode }
impl WriteCursorComponent {
    pub fn new(mode: WriteMode) -> Self { Self { mode } }
}
impl Component for WriteCursorComponent {
    type Next = Cursor;
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        use std::collections::BTreeMap;
        // Group hits by FS.
        let mut by_file: BTreeMap<String, Vec<(usize, usize, Arc<str>)>> = BTreeMap::new();
        let mut out: Vec<Node<Cursor>> = Vec::with_capacity(batch.len());
        for c in batch {
            let (Some(fs), Some(lo_s), Some(hi_s)) = (c.get("FS"), c.get("LO"), c.get("HI")) else {
                out.push(Node::Emit(Arc::new((*c).clone()))); continue;
            };
            let (Ok(lo), Ok(hi)) = (lo_s.parse::<usize>(), hi_s.parse::<usize>()) else {
                out.push(Node::Emit(Arc::new((*c).clone()))); continue;
            };
            by_file.entry(fs.to_string()).or_default().push((lo, hi, c.value.clone()));
            out.push(Node::Emit(Arc::new((*c).clone())));
        }
        for (path, mut hits) in by_file {
            // Right-to-left order so earlier splice offsets remain valid.
            hits.sort_by(|a, b| b.0.cmp(&a.0));
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let mut buf: Vec<u8> = bytes;
            for (lo, hi, ins) in hits {
                let lo = lo.min(buf.len());
                let hi = hi.min(buf.len()).max(lo);
                let (slice_lo, slice_hi) = match self.mode {
                    WriteMode::Replace => (lo, hi),
                    WriteMode::Append  => (hi, hi),
                    WriteMode::Prepend => (lo, lo),
                    WriteMode::Wrap    => {
                        // Wrap = ${ins}<original>${ins}. Splice prepend then
                        // skip — handled inline so the loop still runs once.
                        let ins_b = ins.as_bytes();
                        // append at hi
                        buf.splice(hi..hi, ins_b.iter().copied());
                        // prepend at lo
                        buf.splice(lo..lo, ins_b.iter().copied());
                        continue;
                    }
                };
                buf.splice(slice_lo..slice_hi, ins.as_bytes().iter().copied());
            }
            let _ = std::fs::write(&path, &buf);
        }
        out
    }
}

/// `> write_file(path?)` — write `cursor.value` to a file. Path is
/// resolved as: explicit arg (atom or term-read) overrides cursor.FS.
/// Emits the cursor through unchanged. Per-cursor immediate write —
/// caller's responsibility to ensure unique paths if multiple cursors
/// fan out (otherwise last-writer-wins).
pub struct WriteFileComponent {
    /// Pre-resolved path. If `None`, reads `cursor.FS` at run time.
    pub path: Option<PathBuf>,
}
impl WriteFileComponent {
    pub fn new(path: Option<PathBuf>) -> Self { Self { path } }
}
impl Component for WriteFileComponent {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let path: PathBuf = match &self.path {
            Some(p) => p.clone(),
            None    => match c.get("FS") {
                Some(s) => PathBuf::from(s),
                None    => return Node::Emit(Arc::new(c.clone())),
            },
        };
        let _ = std::fs::write(&path, c.value.as_bytes());
        Node::Emit(Arc::new(c.clone()))
    }
}

/// Json/Yaml/Toml brace-pattern matcher Component. Reads
/// `cursor.get("FS")` as a file path, parses with the configured
/// `TargetFormat`, runs the compiled pattern, and emits one child
/// cursor per matched row with each capture set as a term plus
/// `LO`/`HI` byte offsets pointing at the row's first capture.
pub struct JsonComponent {
    compiled: Arc<crate::cst::dsls::json::JsonCompiled>,
    /// Layer 0c.2 — content-derived intern store.
    store:    Option<Arc<SprfStore>>,
}

impl JsonComponent {
    pub fn new(compiled: crate::cst::dsls::json::JsonCompiled) -> Self {
        Self { compiled: Arc::new(compiled), store: None }
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
}

impl Component for JsonComponent {
    type Next = Cursor;
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let compiled = self.compiled.clone();
        let store = self.store.clone();
        par_render(batch, move |c| {
            let Some(path) = c.get("FS") else { return Node::Done };
            let Ok(bytes)  = std::fs::read(path) else { return Node::Done };
            let file_id = store.as_ref()
                .map(|s| s.intern_file(&bytes, path))
                .unwrap_or(0);
            let rows = compiled.match_grouped(&bytes, 0);
            if rows.is_empty() { return Node::Done; }
            let hits: Vec<Node<Cursor>> = rows.into_iter().filter_map(|caps| {
                if caps.is_empty() { return None; }
                let mut child = c.clone();
                let (_, lo0, hi0, _) = &caps[0];
                child.set("LO", lo0.to_string());
                child.set("HI", hi0.to_string());
                for (name, _lo, _hi, value) in &caps {
                    child.set(name.as_ref(), value.as_ref());
                }
                if let Some(store) = &store {
                    // Focal coord = first capture's span (current
                    // legacy LO/HI semantics).
                    let focal = Coord {
                        repo: 0, rev: 0, fs: file_id,
                        lo: *lo0 as u32, hi: *hi0 as u32,
                    };
                    child.at = store.intern_ref(focal);
                    let focal_slice = caps[0].3.as_ref();
                    child.value_id = store.intern_string(focal_slice);
                    for (name, lo, hi, value) in &caps {
                        let coord = Coord {
                            repo: 0, rev: 0, fs: file_id,
                            lo: *lo as u32, hi: *hi as u32,
                        };
                        child.set_at(name.as_ref(), value.as_ref(), coord, store);
                        // Term-side coord projection for ${<name>.lo}/${<name>.hi}/${<name>.fs}.
                        child.set(&format!("{}_LO", name), coord.lo.to_string());
                        child.set(&format!("{}_HI", name), coord.hi.to_string());
                        if let Some(path) = store.path_of(coord.fs) {
                            child.set(&format!("{}_FS", name), &*path);
                        }
                    }
                }
                Some(Node::Emit(Arc::new(child)))
            }).collect();
            if hits.is_empty() { Node::Done } else { Node::Many(hits) }
        })
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

