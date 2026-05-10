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
//!     Arc::new(FsComponent::new(root, batch))           as Arc<dyn Component<Next = Cursor>>,
//!     Arc::new(AstNmComponent::new(pat_src, lang)),
//!     Arc::new(CountComponent { count: counter.clone() }),
//! ]);
//! expand(&pipe, queue, vec![Arc::new(Cursor::default())], ExpandOpts::default().with_batch_cap(batch));
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

use ast_grep_config::RuleCore;
use ast_grep_core::{source::StrDoc, AstGrep, Language, Pattern};
use ast_grep_language::SupportLang;
use effect_runtime::v2::{
    par_render, splice_into, splice_into_at, BarrierScope, Component, ComponentLifecycle,
    Diag, NextKey, Node, PendingSummary, Purity, QueueBackend, QueueRow, RenderCtx, Wake,
};
use ignore::WalkBuilder;

use crate::config::SprfConfig;
use crate::compile::lower::op_def::DslInterp;
use crate::source::{resolve_path_text, SourceReader};
use crate::store::SprfStore;
use crate::{Coord, Cursor, CursorValue, Interner, WhereBytesId};

pub const FILE_DOMAIN: &str = "file";

pub fn file_dirty_key(path: impl AsRef<std::path::Path>) -> NextKey {
    let path = path.as_ref().to_string_lossy();
    NextKey(*blake3::hash(path.as_bytes()).as_bytes())
}

// ─── path ─────────────────────────────────────────────────────────────────

pub struct PathComponent {
    root:    PathBuf,
    raw:     Option<Arc<str>>,
    interps: Option<Arc<Vec<DslInterp>>>,
    store:   Option<Arc<SprfStore>>,
}

impl PathComponent {
    pub fn new(root: PathBuf) -> Self {
        Self { root, raw: None, interps: None, store: None }
    }
    pub fn with_template(mut self, raw: Arc<str>, interps: Vec<DslInterp>) -> Self {
        self.raw = Some(raw);
        self.interps = Some(Arc::new(interps));
        self
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s);
        self
    }
}

impl Component for PathComponent {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let raw = match (&self.raw, &self.interps) {
            (Some(raw), Some(interps)) => {
                crate::template::render_segments(raw.as_ref(), interps, c, ctx)
            }
            (Some(raw), None) => raw.to_string(),
            _ => c.value.to_string(),
        };
        if raw.is_empty() {
            return Node::Done;
        }
        let path = resolve_path_text(&self.root, &raw);
        let path_str = path.to_string_lossy().to_string();
        let mut child = c.clone();
        child.value = Arc::from(path_str.as_str());
        child.set("FS", path_str.as_str());
        if let Some(store) = &self.store {
            child.value_id = store.intern_string(path_str.as_str());
            child.cursor_value = CursorValue::String(child.value_id);
            child.set_synthetic("FS", path_str.as_str(), store);
        }
        Node::Emit(Arc::new(child))
    }
}

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
    pub batch: usize,
    /// Layer 0c.2 — content-derived intern store. When `Some`, each
    /// emitted Cursor also stamps the FS term in coord-space via
    /// `set_at` alongside the legacy `raw_terms` write. SYNTHETIC
    /// coord because no file content has been read yet at source-op
    /// time; the path string still dedupes through `_strings`.
    store:     Option<Arc<SprfStore>>,
    /// Layer 5b.2 — config handle. Required when upstream cursor's
    /// Coord.rev != 0 so the (slug, root) tuple can be resolved for
    /// `git ls-tree`.
    config:    Option<Arc<SprfConfig>>,
    include_exts: Option<Arc<Vec<String>>>,
    source_filter: Option<Arc<dyn Fn(&str) -> bool + Send + Sync>>,
    telemetry: Option<Arc<FsTelemetry>>,
}

#[derive(Default)]
pub struct FsTelemetry {
    pub seen_files:     AtomicU64,
    pub emitted:        AtomicU64,
    pub ext_skipped:    AtomicU64,
    pub filter_skipped: AtomicU64,
}

impl FsComponent {
    pub fn new(root: PathBuf, batch: usize) -> Self {
        Self {
            root,
            batch,
            store: None,
            config: None,
            include_exts: None,
            source_filter: None,
            telemetry: None,
        }
    }
    /// Attach the intern store for coord-space stamping.
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
    /// Attach the config handle. Needed for the `Coord.rev != 0`
    /// branch which resolves `(slug, root)` per repo for ls-tree.
    pub fn with_config(mut self, c: Arc<SprfConfig>) -> Self {
        self.config = Some(c); self
    }
    pub fn with_include_exts(mut self, exts: Vec<String>) -> Self {
        self.include_exts = Some(Arc::new(exts)); self
    }
    pub fn with_source_filter(
        mut self,
        f: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    ) -> Self {
        self.source_filter = Some(f); self
    }
    pub fn with_telemetry(mut self, telemetry: Arc<FsTelemetry>) -> Self {
        self.telemetry = Some(telemetry); self
    }
    fn include_path(&self, path: impl AsRef<std::path::Path>) -> bool {
        let Some(exts) = &self.include_exts else { return true };
        let Some(ext) = path.as_ref().extension().and_then(|s| s.to_str()) else { return false };
        exts.iter().any(|want| want.eq_ignore_ascii_case(ext))
    }
    fn include_source_value(&self, value: &str) -> bool {
        self.source_filter
            .as_ref()
            .map(|f| f(value))
            .unwrap_or(true)
    }
}

impl Component for FsComponent {
    type Next = Cursor;

    fn kind(&self) -> &'static str { "fs" }

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

        // Layer 5b.2 — git rev branch. Each upstream row may carry a
        // distinct `Coord.rev`, so iterate per-row and dispatch
        // independently when any row has rev != 0.
        let any_rev = self.store.as_ref().map(|s| {
            rows.iter().any(|r| s.coord_of(r.value.at).map(|c| c.rev != 0).unwrap_or(false))
        }).unwrap_or(false);
        if any_rev {
            for parent in rows {
                let store  = match self.store.as_ref()  { Some(s) => s.clone(), None => continue };
                let config = match self.config.as_ref() { Some(c) => c.clone(), None => continue };
                let parent_coord = match store.coord_of(parent.value.at) {
                    Some(c) => c,
                    None    => continue,
                };
                if parent_coord.rev == 0 { continue; }

                // Resolve (slug, root) and the rev's oid hex.
                let Some((slug, _remote)) = store.lookup_repo(parent_coord.repo) else { continue };
                let slug_str: &str = slug.as_ref();
                let Some(root_pb) = config.repos.iter().find(|r| r.slug == slug_str).map(|r| r.root.clone()) else { continue };
                let Some((_repo_id, oid_arc, _ts)) = store.lookup_rev(parent_coord.rev) else { continue };

                let entries = walk_via_ls_tree(&root_pb, oid_arc.as_ref());
                if entries.is_empty() { continue; }
                let mut emitted: Vec<Node<Cursor>> = Vec::with_capacity(entries.len());
                for (path, _blob_oid) in entries {
                    if let Some(t) = &self.telemetry {
                        t.seen_files.fetch_add(1, Ordering::Relaxed);
                    }
                    if !self.include_path(&path) {
                        if let Some(t) = &self.telemetry {
                            t.ext_skipped.fetch_add(1, Ordering::Relaxed);
                        }
                        continue;
                    }
                    if !self.include_source_value(&path) {
                        if let Some(t) = &self.telemetry {
                            t.filter_skipped.fetch_add(1, Ordering::Relaxed);
                        }
                        continue;
                    }
                    let mut child = parent.value.as_ref().clone();
                    // cursor.value = path (legacy bare-string surface).
                    child.value    = Arc::<str>::from(path.as_str());
                    child.value_id = crate::StringId::of(&path);
                    child.cursor_value = CursorValue::String(child.value_id);
                    // Legacy raw_terms FS write so `${FS}` reads still resolve.
                    child.set("FS", path.as_str());
                    // Coord-space FS term (synthetic — no content read).
                    child.set_synthetic("FS", path.as_str(), &store);
                    // Coord preserves repo+rev, fs=0 (no file id until read).
                    let coord = Coord {
                        repo: parent_coord.repo,
                        rev:  parent_coord.rev,
                        fs:   0,
                        lo:   0,
                        hi:   0,
                    };
                    child.at = store.intern_ref(coord);
                    if let Some(t) = &self.telemetry {
                        t.emitted.fetch_add(1, Ordering::Relaxed);
                    }
                    emitted.push(Node::Emit(Arc::new(child)));
                }
                if emitted.is_empty() { continue; }
                let many = Node::Many(emitted);
                splice_into_at(parent, many, ctx.depth + 1, ctx.expand_tick, queue, 0);
            }
            return;
        }

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

        // Inherit Coord (repo/rev/fs=0) from the bootstrap parent so a
        // bare `fs` rooted at a non-zero rev would still be reachable.
        // Today the rev branch above runs for any non-zero rev; this WT
        // path runs only when all upstream rows have rev == 0.
        let parent_coord = self.store.as_ref()
            .and_then(|s| s.coord_of(parent.value.at));

        for entry in WalkBuilder::new(&self.root).hidden(true).git_ignore(false).build() {
            let Ok(e) = entry else { continue };
            if !e.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
            let p = e.into_path();
            if let Some(t) = &self.telemetry {
                t.seen_files.fetch_add(1, Ordering::Relaxed);
            }
            if !self.include_path(&p) {
                if let Some(t) = &self.telemetry {
                    t.ext_skipped.fetch_add(1, Ordering::Relaxed);
                }
                continue;
            }

            let path_str = p.display().to_string();
            if !self.include_source_value(&path_str) {
                if let Some(t) = &self.telemetry {
                    t.filter_skipped.fetch_add(1, Ordering::Relaxed);
                }
                continue;
            }
            let mut c = parent.value.as_ref().clone();
            // Pure-query contract: cursor.value carries the path string.
            // glob/re downstream match against it; read uses it as the
            // source path.
            c.value = Arc::<str>::from(path_str.as_str());
            // Legacy raw_terms FS write so `${FS}` reads still resolve
            // until call-sites finish migrating to ${&.value}.
            c.set("FS", path_str.as_str());
            // Worktree path cursors keep coord-space cold. `read` or a
            // downstream matcher materializes durable file/ref rows only
            // after bytes are actually needed.
            c.value_id = crate::StringId::of(path_str.as_str());
            c.cursor_value = CursorValue::String(c.value_id);
            if let (Some(store), Some(parent_coord)) = (&self.store, parent_coord) {
                if parent_coord.repo != 0 || parent_coord.rev != 0 {
                    c.set_synthetic("FS", path_str.as_str(), store);
                    let coord = Coord {
                        repo: parent_coord.repo,
                        rev:  parent_coord.rev,
                        fs:   0,
                        lo:   0,
                        hi:   0,
                    };
                    c.at = store.intern_ref(coord);
                }
            }
            if let Some(t) = &self.telemetry {
                t.emitted.fetch_add(1, Ordering::Relaxed);
            }
            buf.push(Node::Emit(Arc::new(c)));

            if buf.len() >= self.batch { flush(&mut buf, &mut next_idx); }
        }
        flush(&mut buf, &mut next_idx);
    }
}

/// Layer 5b.2 — shell `git ls-tree -r -z <oid>` and parse null-delimited
/// `<mode> <type> <oid>\t<path>\0` blob entries. Ported from
/// `v3/crates/sprefa/src/readers/git.rs::walk_wt_with_ls_tree`. Returns
/// `(path, raw_oid)` pairs; non-blob entries (trees, commits) are
/// skipped. libgit2 tree walk is forbidden by memory
/// `project_v4_git_blob_walk_uses_shell` — shell ls-tree is the proven
/// fast path at sprefa scale.
fn walk_via_ls_tree(root: &std::path::Path, rev: &str) -> Vec<(String, [u8; 20])> {
    let out = match std::process::Command::new("git")
        .args(["ls-tree", "-r", "-z", rev])
        .current_dir(root)
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    let mut result = Vec::new();
    for entry in out.split(|&b| b == 0) {
        if entry.is_empty() { continue; }
        let Some(tab) = entry.iter().position(|&b| b == b'\t') else { continue };
        let header = &entry[..tab];
        let path   = match std::str::from_utf8(&entry[tab + 1..]) {
            Ok(s) => s.to_string(),
            Err(_) => continue,
        };
        let parts: Vec<&[u8]> = header.splitn(3, |&b| b == b' ').collect();
        if parts.len() != 3 || parts[1] != b"blob" { continue; }
        let oid_hex = match std::str::from_utf8(parts[2]) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if oid_hex.len() != 40 { continue; }
        let mut oid = [0u8; 20];
        let ok = oid_hex.as_bytes().chunks(2).zip(oid.iter_mut()).all(|(pair, byte)| {
            let hi = (pair[0] as char).to_digit(16);
            let lo = (pair[1] as char).to_digit(16);
            match (hi, lo) {
                (Some(h), Some(l)) => { *byte = (h * 16 + l) as u8; true }
                _ => false,
            }
        });
        if !ok { continue; }
        result.push((path, oid));
    }
    result
}

/// Tier-2 op. ast-grep matcher. Pattern compiled once at construction;
/// each batch fans across rayon via `par_render`. Per cursor: read FS
/// file, prefilter with `Pattern::fixed_string`, parse, match, emit
/// one child per hit with LO/HI byte range stamped.
pub struct AstNmComponent {
    root:    PathBuf,
    lang:    SupportLang,
    pattern: Arc<Pattern<SupportLang>>,
    fixed:   Arc<str>,
    /// Slow path: when Some, the body contains a SubPipe carveout and
    /// the ast-grep Pattern is rebuilt per cursor against
    /// `template::render_segments`. `pattern` / `fixed` are unused on
    /// this path. Pattern recompile is more expensive than regex (full
    /// tree-sitter Query build); v0 takes the cost — a future commit
    /// can introduce a Pattern cache if usage warrants. (See bd note:
    /// subpipe-pattern-recompile-cache.)
    dyn_body: Option<(Arc<str>, Arc<Vec<crate::compile::lower::op_def::DslInterp>>)>,
    /// Layer 0c.2 — content-derived intern store. When `Some`, each
    /// match cursor stamps focal `value_id`/`at` and per-match coord
    /// terms alongside legacy `LO`/`HI`/raw_terms writes.
    store:   Option<Arc<SprfStore>>,
    config:  Option<Arc<SprfConfig>>,
    telemetry: Option<Arc<AstTelemetry>>,
}

#[derive(Default)]
pub struct AstTelemetry {
    pub input_rows:         AtomicU64,
    pub source_read_rows:   AtomicU64,
    pub source_read_bytes:  AtomicU64,
    pub source_utf8_rows:   AtomicU64,
    pub source_utf8_bytes:  AtomicU64,
    pub source_read_errors: AtomicU64,
    pub legacy_rows:        AtomicU64,
    pub prefilter_skips:    AtomicU64,
    pub parses:             AtomicU64,
    pub matches:            AtomicU64,
    pub pattern_ns:         AtomicU64,
    pub source_read_ns:     AtomicU64,
    pub prefilter_ns:       AtomicU64,
    pub utf8_ns:            AtomicU64,
    pub legacy_ns:          AtomicU64,
    pub parse_ns:           AtomicU64,
    pub match_ns:           AtomicU64,
    pub emit_stamp_ns:      AtomicU64,
}

impl AstNmComponent {
    pub fn new(pat_src: String, lang: SupportLang) -> Self {
        let pattern = Arc::new(Pattern::new(&pat_src, lang));
        let fixed: Arc<str> = Arc::from(pattern.fixed_string().to_string().as_str());
        Self {
            root: PathBuf::from("."),
            lang,
            pattern,
            fixed,
            dyn_body: None,
            store: None,
            config: None,
            telemetry: None,
        }
    }
    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.root = root; self
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
    pub fn with_config(mut self, c: Arc<SprfConfig>) -> Self {
        self.config = Some(c); self
    }
    pub fn with_telemetry(mut self, telemetry: Arc<AstTelemetry>) -> Self {
        self.telemetry = Some(telemetry); self
    }
    /// Slow path: route every cursor through render_segments → recompile.
    /// Used when the body's interps include a SubPipe carveout.
    pub fn with_dyn_body(
        mut self,
        body:    Arc<str>,
        interps: Vec<crate::compile::lower::op_def::DslInterp>,
    ) -> Self {
        self.dyn_body = Some((body, Arc::new(interps)));
        self
    }
}

fn ast_timed<T>(
    telemetry: &Option<Arc<AstTelemetry>>,
    field: impl FnOnce(&AstTelemetry) -> &AtomicU64,
    f: impl FnOnce() -> T,
) -> T {
    if let Some(t) = telemetry {
        let t0 = std::time::Instant::now();
        let out = f();
        field(t).fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
        out
    } else {
        f()
    }
}

impl Component for AstNmComponent {
    type Next = Cursor;

    fn kind(&self) -> &'static str { "ast" }

    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let store = self.store.clone();
        let lang = self.lang;
        let pattern = self.pattern.clone();
        let fixed = self.fixed.clone();
        let dyn_body = self.dyn_body.clone();
        let telemetry = self.telemetry.clone();
        let config = self.config.clone();
        let root = self.root.clone();
        par_render(batch, move |c| {
            if c.value.is_empty() { return Node::Done; }
            if let Some(t) = &telemetry {
                t.input_rows.fetch_add(1, Ordering::Relaxed);
            }
            let parent_coord = store.as_ref().and_then(|s| s.coord_of(c.at));
            // Per-cursor pattern selection: static for term-only bodies,
            // SubPipe-bearing bodies recompile the ast-grep Pattern per
            // cursor against the rendered body.
            let (cur_pat, cur_fixed): (Arc<Pattern<SupportLang>>, Arc<str>) =
                ast_timed(&telemetry, |t| &t.pattern_ns, || {
                    match &dyn_body {
                        None => (pattern.clone(), fixed.clone()),
                        Some((body, interps)) => {
                            let pat_src = crate::template::render_segments_no_diag(
                                body.as_ref(), interps, c,
                            );
                            let p = Pattern::new(&pat_src, lang);
                            let fx: Arc<str> = Arc::from(p.fixed_string().to_string().as_str());
                            (Arc::new(p), fx)
                        }
                    }
                });
            let reader = SourceReader::new(root.clone(), store.clone(), config.clone());
            let source = ast_timed(&telemetry, |t| &t.source_read_ns, || {
                reader.read_cursor_uninterned(c)
            });
            let (
                src,
                file_id,
                repo,
                rev,
                source_path,
                source_bytes,
                prefiltered_bytes,
            ): (
                String,
                crate::FileId,
                crate::RepoId,
                crate::RevId,
                Option<Arc<str>>,
                Option<Vec<u8>>,
                bool,
            ) = if let Some(source) = source.filter(|s| s.path.as_ref() == c.value.as_ref()) {
                    if let Some(t) = &telemetry {
                        t.source_read_rows.fetch_add(1, Ordering::Relaxed);
                        t.source_read_bytes.fetch_add(source.bytes.len() as u64, Ordering::Relaxed);
                    }
                    if !cur_fixed.is_empty() {
                        let miss = ast_timed(&telemetry, |t| &t.prefilter_ns, || {
                            memchr::memmem::find(&source.bytes, cur_fixed.as_bytes()).is_none()
                        });
                        if miss {
                            if let Some(t) = &telemetry {
                                t.prefilter_skips.fetch_add(1, Ordering::Relaxed);
                            }
                            return Node::Done;
                        }
                    }
                    let source_path = source.path.clone();
                    let source_bytes = source.bytes;
                    let src = ast_timed(&telemetry, |t| &t.utf8_ns, || {
                        String::from_utf8_lossy(&source_bytes).into_owned()
                    });
                    if let Some(t) = &telemetry {
                        t.source_utf8_rows.fetch_add(1, Ordering::Relaxed);
                        t.source_utf8_bytes.fetch_add(source_bytes.len() as u64, Ordering::Relaxed);
                    }
                    (
                        src,
                        source.file,
                        source.coord.repo,
                        source.coord.rev,
                        Some(source_path),
                        Some(source_bytes),
                        !cur_fixed.is_empty(),
                    )
                } else {
                    ast_timed(&telemetry, |t| &t.legacy_ns, || {
                        if let Some(t) = &telemetry {
                            t.legacy_rows.fetch_add(1, Ordering::Relaxed);
                        }
                        let src: String = c.value.as_ref().to_string();
                        let (repo, rev) = parent_coord
                            .map(|c| (c.repo, c.rev))
                            .unwrap_or((0, 0));
                        let path_for_intern: &str = c.get("FS").unwrap_or("");
                        let file_id = store.as_ref()
                            .map(|s| s.intern_file(src.as_bytes(), path_for_intern))
                            .unwrap_or(0);
                        (src, file_id, repo, rev, None, None, false)
                    })
                };
            if !prefiltered_bytes && !cur_fixed.is_empty() {
                let miss = ast_timed(&telemetry, |t| &t.prefilter_ns, || {
                    !src.contains(&*cur_fixed)
                });
                if miss {
                    if let Some(t) = &telemetry {
                        t.prefilter_skips.fetch_add(1, Ordering::Relaxed);
                    }
                    return Node::Done;
                }
            }
            if let Some(t) = &telemetry {
                t.parses.fetch_add(1, Ordering::Relaxed);
            }
            let grep: AstGrep<StrDoc<SupportLang>> =
                ast_timed(&telemetry, |t| &t.parse_ns, || lang.ast_grep(&src));
            let ranges: Vec<std::ops::Range<usize>> = ast_timed(&telemetry, |t| &t.match_ns, || {
                grep.root().find_all(&*cur_pat).map(|nm| nm.range()).collect()
            });
            if ranges.is_empty() {
                return Node::Done;
            }
            let match_file_id = if let Some(store) = &store {
                if file_id == 0 {
                    match (source_path.as_ref(), source_bytes.as_ref()) {
                        (Some(path), Some(bytes)) => {
                            let file = store.intern_file(bytes, path);
                            let _ = store.intern_path(repo, rev, file, path);
                            file
                        }
                        _ => file_id,
                    }
                } else {
                    file_id
                }
            } else {
                file_id
            };
            let hits: Vec<Node<Cursor>> = ranges.into_iter().map(|r| {
                let mut child = c.clone();
                child.set("LO", (r.start as u64).to_string());
                child.set("HI", (r.end   as u64).to_string());
                if let Some(store) = &store {
                    ast_timed(&telemetry, |t| &t.emit_stamp_ns, || {
                        let coord = Coord {
                            repo, rev, fs: match_file_id,
                            lo: r.start as u32, hi: r.end as u32,
                        };
                        let slice = src.get(r.start..r.end).unwrap_or("");
                        // Focal coord identifies the match itself.
                        child.at = store.intern_ref(coord);
                        child.cursor_value = CursorValue::WhereBytes(WhereBytesId::from(child.at));
                        child.value = Arc::<str>::from(slice);
                        child.value_id = store.intern_string(slice);
                        store.observe_string(
                            child.value_id,
                            "ast_match",
                            WhereBytesId::from(child.at),
                            "",
                            0,
                        );
                        // Per-match coord term — `MATCH.at`/`MATCH.value`
                        // reach through one Term entry instead of LO/HI cols.
                        child.set_at("MATCH", slice, coord, store);
                        // Term-side coord projection for ${MATCH.lo}/${MATCH.hi}/${MATCH.fs}.
                        child.set("MATCH_LO", coord.lo.to_string());
                        child.set("MATCH_HI", coord.hi.to_string());
                        if let Some(path) = store.path_of(coord.fs) {
                            child.set("MATCH_FS", &*path);
                        }
                    });
                }
                Node::Emit(Arc::new(child))
            }).collect();
            if let Some(t) = &telemetry {
                t.matches.fetch_add(hits.len() as u64, Ordering::Relaxed);
            }
            Node::Many(hits)
        })
    }
}

/// Raw ast-grep RuleCore YAML matcher. Mirrors `AstNmComponent`'s
/// source-aware read path but runs a deserialised YAML rule instead of a
/// plain ast-grep pattern.
pub struct AstYamlComponent {
    root:       PathBuf,
    lang:       SupportLang,
    rule_core:  Arc<RuleCore<SupportLang>>,
    bound_caps: Arc<Vec<Arc<str>>>,
    store:      Option<Arc<SprfStore>>,
    config:     Option<Arc<SprfConfig>>,
}

impl AstYamlComponent {
    pub fn new(
        lang: SupportLang,
        rule_core: RuleCore<SupportLang>,
        bound_caps: Vec<Arc<str>>,
    ) -> Self {
        Self {
            root: PathBuf::from("."),
            lang,
            rule_core: Arc::new(rule_core),
            bound_caps: Arc::new(bound_caps),
            store: None,
            config: None,
        }
    }

    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.root = root;
        self
    }

    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s);
        self
    }

    pub fn with_config(mut self, c: Arc<SprfConfig>) -> Self {
        self.config = Some(c);
        self
    }
}

impl Component for AstYamlComponent {
    type Next = Cursor;

    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let store = self.store.clone();
        let lang = self.lang;
        let rule_core = self.rule_core.clone();
        let bound_caps = self.bound_caps.clone();
        let root = self.root.clone();
        let config = self.config.clone();
        par_render(batch, move |c| {
            if c.value.is_empty() { return Node::Done; }

            let parent_coord = store.as_ref().and_then(|s| s.coord_of(c.at));
            let reader = SourceReader::new(root.clone(), store.clone(), config.clone());
            let source = reader.read_cursor(c);
            let (src, file_id, repo, rev): (
                String,
                crate::FileId,
                crate::RepoId,
                crate::RevId,
            ) = if let Some(source) = source.filter(|s| s.path.as_ref() == c.value.as_ref()) {
                (
                    String::from_utf8_lossy(&source.bytes).into_owned(),
                    source.file,
                    source.coord.repo,
                    source.coord.rev,
                )
            } else {
                let src: String = c.value.as_ref().to_string();
                let (repo, rev) = parent_coord
                    .map(|c| (c.repo, c.rev))
                    .unwrap_or((0, 0));
                let path_for_intern: &str = c.get("FS").unwrap_or("");
                let file_id = store.as_ref()
                    .map(|s| s.intern_file(src.as_bytes(), path_for_intern))
                    .unwrap_or(0);
                (src, file_id, repo, rev)
            };

            let grep: AstGrep<StrDoc<SupportLang>> = lang.ast_grep(&src);
            let hits: Vec<Node<Cursor>> = grep.root().find_all(&*rule_core).map(|nm| {
                let r = nm.range();
                let mut child = c.clone();
                child.set("LO", (r.start as u64).to_string());
                child.set("HI", (r.end as u64).to_string());
                let slice = src.get(r.start..r.end).unwrap_or("");
                child.value = Arc::<str>::from(slice);

                if let Some(store) = &store {
                    let coord = Coord {
                        repo, rev, fs: file_id,
                        lo: r.start as u32, hi: r.end as u32,
                    };
                    child.at = store.intern_ref(coord);
                    child.cursor_value = CursorValue::WhereBytes(WhereBytesId::from(child.at));
                    child.value_id = store.intern_string(slice);
                    store.observe_string(
                        child.value_id,
                        "ast_yaml_match",
                        WhereBytesId::from(child.at),
                        "",
                        0,
                    );
                    child.set_at("MATCH", slice, coord, store);
                    child.set("MATCH_LO", coord.lo.to_string());
                    child.set("MATCH_HI", coord.hi.to_string());
                    if let Some(path) = store.path_of(coord.fs) {
                        child.set("MATCH_FS", &*path);
                    }
                }

                let env = nm.get_env();
                for cap_name in bound_caps.iter() {
                    if let Some(node) = env.get_match(cap_name.as_ref()) {
                        let nr = node.range();
                        let text = node.text().to_string();
                        child.set(cap_name.as_ref(), text.as_str());
                        if let Some(store) = &store {
                            let coord = Coord {
                                repo, rev, fs: file_id,
                                lo: nr.start as u32, hi: nr.end as u32,
                            };
                            child.set_at(cap_name.as_ref(), text.as_str(), coord, store);
                        }
                        continue;
                    }
                    let multi = env.get_multiple_matches(cap_name.as_ref());
                    if !multi.is_empty() {
                        let joined = multi
                            .iter()
                            .map(|n| n.text().to_string())
                            .collect::<Vec<_>>()
                            .join(" ");
                        child.set(cap_name.as_ref(), joined.as_str());
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
            // Substrate purification: cursor.value carries the bytes;
            // upstream `read` is the gate that materializes them.
            if c.value.is_empty() { return Node::Done; }
            let src: String = c.value.as_ref().to_string();
            let path_for_intern: &str = c.get("FS").unwrap_or("");
            let file_id = store.as_ref()
                .map(|s| s.intern_file(src.as_bytes(), path_for_intern))
                .unwrap_or(0);
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

/// Render markdown rows by applying a template to every cursor in the
/// current runtime batch, then concatenating those rows into one output
/// cursor value.
pub struct RenderMarkdownComponent {
    pub template: Arc<str>,
    pub interps:  Arc<Vec<DslInterp>>,
}

impl RenderMarkdownComponent {
    pub fn new(template: Arc<str>, interps: Vec<DslInterp>) -> Self {
        Self { template, interps: Arc::new(interps) }
    }
}

impl Component for RenderMarkdownComponent {
    type Next = Cursor;

    fn render_batch(&self, ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let Some(first) = batch.first() else { return Vec::new(); };
        let mut out = (*first).clone();
        let mut value = String::new();
        for cursor in batch {
            let rendered = crate::template::render_segments(
                self.template.as_ref(),
                &self.interps,
                cursor,
                ctx,
            );
            value.push_str(&rendered);
            if !rendered.ends_with('\n') {
                value.push('\n');
            }
        }
        out.value = Arc::from(value);
        vec![Node::Emit(Arc::new(out))]
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
    /// Static path: pre-compiled regex shared across cursors.
    re:            Arc<regex::Regex>,
    /// Slow path: when Some, the body contains a SubPipe carveout and
    /// is recompiled per cursor against `template::render_segments`.
    /// `re` is unused on this path.
    dyn_body:      Option<(Arc<str>, Arc<Vec<crate::compile::lower::op_def::DslInterp>>)>,
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
            dyn_body:      None,
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
    /// Slow path: route every cursor through render_segments → recompile.
    /// Used when the body's interps include a SubPipe carveout.
    pub fn with_dyn_body(
        mut self,
        body:    Arc<str>,
        interps: Vec<crate::compile::lower::op_def::DslInterp>,
    ) -> Self {
        self.dyn_body = Some((body, Arc::new(interps)));
        self
    }
}
impl Component for ReComponent {
    type Next = Cursor;
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let re = self.re.clone();
        let dyn_body = self.dyn_body.clone();
        let on = self.on.clone();
        let capture_names = self.capture_names.clone();
        let want_match = self.want_match;
        let interner = self.interner.clone();
        let store = self.store.clone();
        par_render(batch, move |c| {
            // Per-cursor regex selection: static `re` for term-only
            // bodies, dynamic recompile when the body has a SubPipe
            // carveout. Recompile failure drops the cursor.
            let cur_re: std::borrow::Cow<'_, regex::Regex> = match &dyn_body {
                None => std::borrow::Cow::Borrowed(re.as_ref()),
                Some((body, interps)) => {
                    let pat = crate::template::render_segments_no_diag(
                        body.as_ref(), interps, c,
                    );
                    match regex::Regex::new(&pat) {
                        Ok(r)  => std::borrow::Cow::Owned(r),
                        Err(_) => return Node::Done,
                    }
                }
            };
            let re: &regex::Regex = cur_re.as_ref();
            let mut file_id: crate::FileId = 0;
            let source_coord = store.as_ref()
                .and_then(|s| s.coord_of(c.at))
                .unwrap_or_default();
            let (haystack, content_hash_arc): (String, Option<Arc<str>>) =
                if on.is_empty() || on == "FS" {
                    if c.value.is_empty() { return Node::Done; }
                    let bytes = c.value.as_bytes();
                    let h_hex = blake3::hash(bytes).to_hex().to_string();
                    let h_arc: Option<Arc<str>> = interner.as_ref().map(|i| i.intern(&h_hex));
                    if let Some(s) = &store {
                        let path_for_intern: &str = c.get("FS").unwrap_or("");
                        file_id = s.intern_file(bytes, path_for_intern);
                    }
                    (String::from_utf8_lossy(bytes).into_owned(), h_arc)
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
                        repo: source_coord.repo, rev: source_coord.rev, fs: file_id,
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
                                repo: source_coord.repo, rev: source_coord.rev, fs: file_id,
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
/// Render a sh command body per cursor through the universal carveout
/// renderer, then shell-quote each splice. The renderer emits raw
/// (unquoted) values; this wrapper post-quotes Term + SubPipe slices so
/// the resulting command line is shell-safe.
///
/// Quoting strategy: re-render with empty interp values to find the
/// literal scaffolding; then walk interps and shell-quote each
/// per-cursor render. We instead splice and quote inline to keep one
/// pass: collect (lo, hi, rendered) triples and assemble.
fn render_sh_template(
    body: &str,
    interps: &[crate::compile::lower::op_def::DslInterp],
    cur: &Cursor,
) -> String {
    use crate::compile::lower::op_def::InterpKind;
    use crate::template::drain_subpipe;
    let mut out = String::with_capacity(body.len());
    let mut head: usize = 0;
    for interp in interps.iter() {
        let lo = interp.range.lo as usize;
        let hi = interp.range.hi as usize;
        if lo > body.len() || hi > body.len() || lo < head { continue; }
        out.push_str(&body[head..lo]);
        let value: String = match &interp.kind {
            InterpKind::Term { mode: _, field } => {
                let key: std::borrow::Cow<'_, str> = match field {
                    None    => (&*interp.name).into(),
                    Some(f) => format!("{}.{}", interp.name, f).into(),
                };
                cur.get(&key).unwrap_or("").to_string()
            }
            InterpKind::SubPipe { lowered, .. } => {
                match lowered {
                    Some(p) => drain_subpipe(p.clone(), cur),
                    None    => String::new(),
                }
            }
        };
        out.push_str(&shell_quote(&value));
        head = hi;
    }
    out.push_str(&body[head..]);
    out
}

/// Run `sh -c <rendered>` per input cursor; act on stdout per `mode`.
/// Substrate uses sync `std::process::Command` (rayon par_render gives
/// per-batch parallelism). Tokio not needed.
pub struct ShComponent {
    cmd_template: String,
    interps:      Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
    mode:         ShMode,
    bind:         Option<String>,
}
impl ShComponent {
    pub fn emit_lines(cmd: &str) -> Self {
        Self { cmd_template: cmd.into(), interps: Arc::new(Vec::new()),
               mode: ShMode::EmitLines, bind: None }
    }
    pub fn filter(cmd: &str) -> Self {
        Self { cmd_template: cmd.into(), interps: Arc::new(Vec::new()),
               mode: ShMode::FilterByExit, bind: None }
    }
    pub fn capture(cmd: &str, bind: &str) -> Self {
        Self { cmd_template: cmd.into(), interps: Arc::new(Vec::new()),
               mode: ShMode::CaptureStdout, bind: Some(bind.into()) }
    }
    /// Capture stdout into `cursor.value` only (no named term). Use
    /// when the next op operates on `&.value` (e.g. `> split\`\n\``).
    pub fn capture_value(cmd: &str) -> Self {
        Self { cmd_template: cmd.into(), interps: Arc::new(Vec::new()),
               mode: ShMode::CaptureStdout, bind: None }
    }
    /// Stamp pre-walked interps. `ShDef::lower` calls this to thread
    /// `${...}` carveouts through `render_sh_template`.
    pub fn with_interps(
        mut self,
        interps: Vec<crate::compile::lower::op_def::DslInterp>,
    ) -> Self { self.interps = Arc::new(interps); self }
}
impl Component for ShComponent {
    type Next = Cursor;
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let template = self.cmd_template.clone();
        let interps  = self.interps.clone();
        let mode     = self.mode;
        let bind     = self.bind.clone();
        par_render(batch, move |c| {
            let cmd = render_sh_template(&template, &interps, c);
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

    fn purity(&self) -> Purity { Purity::Effectful }
}

/// `sh!` — author-declared impure shell. Per-cursor cmd run, stdout
/// dropped, cursor passes through. Never cacheable.
pub struct ShBangComponent {
    cmd_template: String,
    interps:      Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
}
impl ShBangComponent {
    pub fn new(cmd: &str) -> Self {
        Self { cmd_template: cmd.into(), interps: Arc::new(Vec::new()) }
    }
    pub fn with_interps(
        mut self,
        interps: Vec<crate::compile::lower::op_def::DslInterp>,
    ) -> Self { self.interps = Arc::new(interps); self }
}
impl Component for ShBangComponent {
    type Next = Cursor;
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let template = self.cmd_template.clone();
        let interps  = self.interps.clone();
        par_render(batch, move |c| {
            let cmd = render_sh_template(&template, &interps, c);
            let _ = std::process::Command::new("sh")
                .arg("-c").arg(&cmd)
                .status();
            Node::Emit(Arc::new(c.clone()))
        })
    }

    fn purity(&self) -> Purity { Purity::Effectful }
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
    root:   PathBuf,
    /// Layer 0c.2 — content-derived intern store. When set, focal
    /// `value_id`/`at` stamp the file content + a whole-file coord.
    store:  Option<Arc<SprfStore>>,
    /// Layer 5b.2 — config handle. Required when upstream cursor's
    /// Coord.rev != 0 so the (slug, root) tuple can be resolved for
    /// `git show <oid>:<path>`.
    config: Option<Arc<SprfConfig>>,
}
impl ReadComponent {
    pub fn new() -> Self { Self { root: PathBuf::from("."), store: None, config: None } }
    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.root = root;
        self
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
    pub fn with_config(mut self, c: Arc<SprfConfig>) -> Self {
        self.config = Some(c); self
    }

    fn file_subscription_path(&self, c: &Cursor) -> Option<String> {
        if c.value.is_empty() {
            return None;
        }
        if let Some(store) = self.store.as_ref() {
            if store.coord_of(c.at).map(|coord| coord.rev != 0).unwrap_or(false) {
                return None;
            }
        }
        Some(c.value.to_string())
    }
}
impl Default for ReadComponent {
    fn default() -> Self { Self::new() }
}
impl Component for ReadComponent {
    type Next = Cursor;

    fn render_batch(&self, ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let this = Self {
            root: self.root.clone(),
            store: self.store.clone(),
            config: self.config.clone(),
        };
        par_render(batch, move |c| this.render(ctx, c))
    }

    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let inputs: Vec<&Cursor> = rows.iter().map(|r| r.value.as_ref()).collect();
        let nodes = self.render_batch(ctx, &inputs);
        for (row, node) in rows.iter().zip(nodes) {
            splice_into(row, node, ctx.depth + 1, ctx.expand_tick, queue);
            let Some(path) = self.file_subscription_path(row.value.as_ref()) else { continue };
            queue.enqueue_replacing_parked(QueueRow {
                id: 0,
                parent_id: Some(row.id),
                batch_idx: row.batch_idx,
                path: row.path.clone(),
                pipe_hash: row.pipe_hash,
                instance_id: row.instance_id,
                depth: row.depth,
                value: row.value.clone(),
                wake: Wake::Key {
                    domain: FILE_DOMAIN.into(),
                    key: file_dirty_key(path),
                },
                expand_tick: ctx.expand_tick,
                enqueued_at_ns: row.enqueued_at_ns,
            });
        }
    }

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let reader = SourceReader::new(
            self.root.clone(),
            self.store.clone(),
            self.config.clone(),
        );
        let Some(source) = reader.read_cursor(c) else {
            return Node::Done;
        };
        let s = String::from_utf8_lossy(&source.bytes).into_owned();
        let mut child = c.clone();
        child.value = Arc::<str>::from(s.as_str());
        if let Some(store) = &self.store {
            child.at = store.intern_ref(source.coord);
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

/// `> write_cursor(mode?, :NAME?)` — splice `cursor.value` into file
/// at (FS, [LO, HI)). Aggregates per file and applies right-to-left so
/// multi-cursor splices stay correct (left edits would shift right
/// offsets). One file open + one write per (file, batch).
///
/// Layer 3 — `:on=NAME` capture-internal mode. When `on_capture` is
/// `Some(NAME)`, the splice byte range comes from `cursor.term(NAME).at`
/// resolved through the SprfStore (`coord_of`) instead of the legacy
/// `FS`/`LO`/`HI` raw_terms. Falls back to focal `FS`/`LO`/`HI` when no
/// capture is named (existing behavior).
///
/// Drift-detection — before splicing, the on-disk file is hashed and
/// compared to the FileId's `content_hash` from `intern_file`. Mismatch
/// emits `write/file-drift` and skips the write to avoid clobbering.
pub struct WriteCursorComponent {
    pub mode:        WriteMode,
    /// Layer 3 — NAME of a coord-space term to splice into. `None` =
    /// focal-cursor splice (legacy behavior).
    pub on_capture:  Option<Arc<str>>,
    /// Layer 3 — coord-space store handle. Required for `on_capture`
    /// resolution and drift-detection.
    store:           Option<Arc<SprfStore>>,
}
impl WriteCursorComponent {
    pub fn new(mode: WriteMode) -> Self {
        Self { mode, on_capture: None, store: None }
    }
    /// Builder: set capture name for `:on=NAME` mode.
    pub fn on_capture(mut self, name: impl Into<Arc<str>>) -> Self {
        self.on_capture = Some(name.into()); self
    }
    /// Builder: attach the SprfStore. Required when `on_capture` is set
    /// and powers drift-detection in either mode.
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
}
impl Component for WriteCursorComponent {
    type Next = Cursor;
    fn render_batch(&self, ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        use std::collections::BTreeMap;
        // Group hits by FS path string. Each hit also carries its
        // FileId (Some(_) when the capture/focal coord was resolved
        // through the store; None on the legacy raw_terms path) so
        // drift-detection can run per file.
        let mut by_file: BTreeMap<String, (Option<crate::FileId>, Vec<(usize, usize, Arc<str>)>)>
            = BTreeMap::new();
        let mut out: Vec<Node<Cursor>> = Vec::with_capacity(batch.len());
        for c in batch {
            // Always pass the cursor through unchanged.
            out.push(Node::Emit(Arc::new((*c).clone())));

            // ── capture-internal mode (Layer 3) ────────────────────
            if let Some(name) = &self.on_capture {
                let Some(store) = self.store.as_ref() else {
                    ctx.diag.emit(Diag::warn(
                        "write/no-store",
                        format!("write_cursor(:on={}) requires SprfStore wiring", name),
                    ));
                    continue;
                };
                let name_id = crate::StringId::of(name.as_ref());
                let Some(term) = c.term(name_id) else {
                    ctx.diag.emit(Diag::warn(
                        "write/missing-capture",
                        format!("write_cursor(:on={}): cursor has no term named {}", name, name),
                    ));
                    continue;
                };
                let coord = match store.coord_of(term.at) {
                    Some(c) => c,
                    None => {
                        ctx.diag.emit(Diag::warn(
                            "write/no-coord",
                            format!("write_cursor(:on={}): unknown Ref id", name),
                        ));
                        continue;
                    }
                };
                if coord.fs == 0 {
                    ctx.diag.emit(Diag::warn(
                        "write/synthetic-target",
                        format!("write_cursor(:on={}): capture has synthetic coord (no file)", name),
                    ));
                    continue;
                }
                let Some((_hash, path)) = store.lookup_file(coord.fs) else {
                    ctx.diag.emit(Diag::warn(
                        "write/no-coord",
                        format!("write_cursor(:on={}): FileId {} not in _files", name, coord.fs),
                    ));
                    continue;
                };
                let entry = by_file.entry(path.to_string())
                    .or_insert_with(|| (Some(coord.fs), Vec::new()));
                entry.1.push((coord.lo as usize, coord.hi as usize, c.value.clone()));
                continue;
            }

            // ── focal mode ─────────────────────────────────────────
            // Prefer the coord stamped by source-aware producers such
            // as read/comment/ast. Legacy FS/LO/HI raw terms stay as a
            // compatibility path for older tests and hand-built cursors.
            let target = self.store.as_ref()
                .and_then(|s| {
                    let coord = s.coord_of(c.at)?;
                    if coord.fs == 0 { return None; }
                    let path = s.path_of(coord.fs)?;
                    Some((path.to_string(), Some(coord.fs), coord.lo as usize, coord.hi as usize))
                })
                .or_else(|| {
                    let (Some(fs), Some(lo_s), Some(hi_s)) =
                        (c.get("FS"), c.get("LO"), c.get("HI"))
                    else { return None; };
                    let (Ok(lo), Ok(hi)) = (lo_s.parse::<usize>(), hi_s.parse::<usize>())
                    else { return None; };
                    let file_id_opt: Option<crate::FileId> = self.store.as_ref()
                        .and_then(|s| s.coord_of(c.at))
                        .filter(|coord| coord.fs != 0)
                        .map(|coord| coord.fs);
                    Some((fs.to_string(), file_id_opt, lo, hi))
                });
            let Some((path, file_id_opt, lo, hi)) = target else { continue };
            let entry = by_file.entry(path)
                .or_insert((file_id_opt, Vec::new()));
            // If different cursors on the same path disagree on FileId,
            // first-seen wins; mismatches are unusual for the legacy
            // path and the drift check still runs against that one.
            if entry.0.is_none() { entry.0 = file_id_opt; }
            entry.1.push((lo, hi, c.value.clone()));
        }
        for (path, (file_id_opt, mut hits)) in by_file {
            // Right-to-left order so earlier splice offsets remain valid.
            hits.sort_by(|a, b| b.0.cmp(&a.0));
            let Ok(bytes) = std::fs::read(&path) else { continue };
            // ── drift-detection ──────────────────────────────────
            // When the cursor came through intern_file we have a
            // recorded content_hash. Re-hash the file on disk and skip
            // the write if it has changed since intern.
            if let (Some(file_id), Some(store)) = (file_id_opt, self.store.as_ref()) {
                if let Some((expected_hash, _)) = store.lookup_file(file_id) {
                    let on_disk_hash = *blake3::hash(&bytes).as_bytes();
                    if on_disk_hash != expected_hash {
                        ctx.diag.emit(Diag::warn(
                            "write/file-drift",
                            format!("{}: on-disk content changed since intern_file; skipping write", path),
                        ));
                        continue;
                    }
                }
            }
            let original = bytes;
            let mut buf: Vec<u8> = original.clone();
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
            if buf != original && std::fs::write(&path, &buf).is_ok() {
                ctx.bus.dispatch_dirty(FILE_DOMAIN, Some(file_dirty_key(&path)));
            }
        }
        out
    }

    fn purity(&self) -> Purity { Purity::Effectful }
}

#[derive(Clone)]
pub enum WriteFilePath {
    Static(PathBuf),
    Template {
        raw:     Arc<str>,
        interps: Arc<Vec<DslInterp>>,
    },
}

/// `> write_file(path?)` — write `cursor.value` to a file. Path is
/// resolved as: explicit arg/body overrides cursor.FS.
/// Emits the cursor through unchanged. Per-cursor immediate write —
/// caller's responsibility to ensure unique paths if multiple cursors
/// fan out (otherwise last-writer-wins).
pub struct WriteFileComponent {
    /// Pre-resolved or templated path. If `None`, reads `cursor.FS` at run time.
    pub path: Option<WriteFilePath>,
}
impl WriteFileComponent {
    pub fn new(path: Option<WriteFilePath>) -> Self { Self { path } }
}
impl Component for WriteFileComponent {
    type Next = Cursor;
    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let path: PathBuf = match &self.path {
            Some(WriteFilePath::Static(p)) => p.clone(),
            Some(WriteFilePath::Template { raw, interps }) => PathBuf::from(
                crate::template::render_segments(raw.as_ref(), interps, c, ctx)
            ),
            None    => match c.get("FS") {
                Some(s) => PathBuf::from(s),
                None    => return Node::Emit(Arc::new(c.clone())),
            },
        };
        if std::fs::read(&path).ok().as_deref() == Some(c.value.as_bytes()) {
            return Node::Emit(Arc::new(c.clone()));
        }
        if std::fs::write(&path, c.value.as_bytes()).is_ok() {
            ctx.bus.dispatch_dirty(FILE_DOMAIN, Some(file_dirty_key(&path)));
        }
        Node::Emit(Arc::new(c.clone()))
    }

    fn purity(&self) -> Purity { Purity::Effectful }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectMode {
    Complete,
    ReadySnapshot,
    ReadyAppend,
}

struct CollectState {
    rows: Vec<Cursor>,
    flushed_len: usize,
}

pub struct CollectComponent {
    mode: CollectMode,
    states: Mutex<BTreeMap<BarrierScope, CollectState>>,
}

impl CollectComponent {
    pub fn new(mode: CollectMode) -> Self {
        Self {
            mode,
            states: Mutex::new(BTreeMap::new()),
        }
    }

    fn emit_rows(
        &self,
        scope: BarrierScope,
        queue: &dyn QueueBackend<Cursor>,
        rows: &[Cursor],
    ) {
        if rows.is_empty() {
            return;
        }

        let mut out = rows[0].clone();
        let mut value = String::new();
        for row in rows {
            value.push_str(row.value.as_ref());
        }
        out.value = Arc::from(value);

        queue.enqueue(QueueRow {
            id:             0,
            parent_id:      None,
            batch_idx:      0,
            path:           Vec::new(),
            pipe_hash:      scope.pipe_hash,
            instance_id:    scope.instance_id,
            depth:          scope.depth + 1,
            value:          Arc::new(out),
            wake:           Wake::Immediate,
            expand_tick:    scope.expand_tick,
            enqueued_at_ns: 0,
        });
    }
}

impl Component for CollectComponent {
    type Next = Cursor;

    fn dispatch(
        &self,
        ctx: &RenderCtx,
        rows: &[QueueRow<Cursor>],
        _queue: &dyn QueueBackend<Cursor>,
    ) {
        let scope = BarrierScope {
            pipe_hash: rows.first().map(|r| r.pipe_hash).unwrap_or(ctx.pipe),
            instance_id: rows.first().map(|r| r.instance_id).unwrap_or(0),
            expand_tick: ctx.expand_tick,
            depth: ctx.depth,
        };
        let mut states = self.states.lock().unwrap();
        let state = states.entry(scope).or_insert_with(|| CollectState {
            rows: Vec::new(),
            flushed_len: 0,
        });
        for row in rows {
            state.rows.push(row.value.as_ref().clone());
        }
    }

    fn lifecycle(&self) -> ComponentLifecycle { ComponentLifecycle::Barrier }

    fn idle(
        &self,
        _ctx: &RenderCtx,
        scope: BarrierScope,
        _pending: PendingSummary,
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let mut states = self.states.lock().unwrap();
        let Some(state) = states.get_mut(&scope) else {
            return;
        };
        match self.mode {
            CollectMode::Complete => {}
            CollectMode::ReadySnapshot => {
                if state.rows.len() == state.flushed_len {
                    return;
                }
                self.emit_rows(scope, queue, &state.rows);
                state.flushed_len = state.rows.len();
            }
            CollectMode::ReadyAppend => {
                if state.rows.len() == state.flushed_len {
                    return;
                }
                self.emit_rows(scope, queue, &state.rows[state.flushed_len..]);
                state.flushed_len = state.rows.len();
            }
        }
    }

    fn complete(
        &self,
        _ctx: &RenderCtx,
        scope: BarrierScope,
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let mut states = self.states.lock().unwrap();
        let Some(state) = states.remove(&scope) else {
            return;
        };
        match self.mode {
            CollectMode::Complete => {
                self.emit_rows(scope, queue, &state.rows);
            }
            CollectMode::ReadySnapshot => {
                self.emit_rows(scope, queue, &state.rows);
            }
            CollectMode::ReadyAppend => {
                self.emit_rows(scope, queue, &state.rows[state.flushed_len..]);
            }
        }
    }
}

/// Json/Yaml/Toml brace-pattern matcher Component. Reads
/// `cursor.get("FS")` as a file path, parses with the configured
/// `TargetFormat`, runs the compiled pattern, and emits one child
/// cursor per matched row with each capture set as a term plus
/// `LO`/`HI` byte offsets pointing at the row's first capture.
pub struct JsonComponent {
    root:     PathBuf,
    compiled: Arc<crate::cst::dsls::json::JsonCompiled>,
    /// Slow path: when Some, body+interps recompile per cursor. The
    /// `compiled` slot is unused on this path. Recompile cost: full
    /// brace-pattern compile. v0 takes the cost; cache when warranted.
    dyn_body: Option<(
        Arc<str>,
        Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
        crate::cst::dsls::json::TargetFormat,
    )>,
    /// Layer 0c.2 — content-derived intern store.
    store:    Option<Arc<SprfStore>>,
    config:   Option<Arc<SprfConfig>>,
}

impl JsonComponent {
    pub fn new(compiled: crate::cst::dsls::json::JsonCompiled) -> Self {
        Self {
            root: PathBuf::from("."),
            compiled: Arc::new(compiled),
            dyn_body: None,
            store: None,
            config: None,
        }
    }
    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.root = root;
        self
    }
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.store = Some(s); self
    }
    pub fn with_config(mut self, c: Arc<SprfConfig>) -> Self {
        self.config = Some(c); self
    }
    /// Slow path: route every cursor through render_segments → recompile.
    /// Fmt is captured here so the per-cursor compile rebuilds with the
    /// same TargetFormat the static path would have used.
    pub fn with_dyn_body(
        mut self,
        body:    Arc<str>,
        interps: Vec<crate::compile::lower::op_def::DslInterp>,
        fmt:     crate::cst::dsls::json::TargetFormat,
    ) -> Self {
        self.dyn_body = Some((body, Arc::new(interps), fmt));
        self
    }
}

impl Component for JsonComponent {
    type Next = Cursor;
    fn render_batch(&self, _ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        let compiled = self.compiled.clone();
        let dyn_body = self.dyn_body.clone();
        let store = self.store.clone();
        let config = self.config.clone();
        let root = self.root.clone();
        par_render(batch, move |c| {
            if c.value.is_empty() { return Node::Done; }
            let reader = SourceReader::new(root.clone(), store.clone(), config.clone());
            let source = reader.read_cursor(c);
            let bytes_cow: std::borrow::Cow<'_, [u8]> = match source.as_ref() {
                Some(source) if source.path.as_ref() == c.value.as_ref() => {
                    std::borrow::Cow::Borrowed(source.bytes.as_slice())
                }
                _ => std::borrow::Cow::Borrowed(c.value.as_bytes()),
            };
            let bytes = bytes_cow.as_ref();
            let parent_coord = store.as_ref()
                .and_then(|s| s.coord_of(c.at))
                .unwrap_or_default();
            let (file_id, repo, rev) = match source.as_ref() {
                Some(source) if source.path.as_ref() == c.value.as_ref() => {
                    (source.file, source.coord.repo, source.coord.rev)
                }
                _ => {
                    let path_for_intern: &str = c.get("FS").unwrap_or("");
                    let file_id = store.as_ref()
                        .map(|s| s.intern_file(bytes, path_for_intern))
                        .unwrap_or(0);
                    (file_id, parent_coord.repo, parent_coord.rev)
                }
            };
            // Per-cursor pattern selection: SubPipe-bearing bodies
            // recompile JsonCompiled per cursor. JsonCompiled isn't Clone,
            // so the dynamic arm builds owned; the static arm references
            // the shared `compiled` Arc.
            let dyn_built: Option<crate::cst::dsls::json::JsonCompiled> = match &dyn_body {
                None => None,
                Some((body, interps, fmt)) => {
                    let pat_src = crate::template::render_segments_no_diag(
                        body.as_ref(), interps, c,
                    );
                    let built = match crate::cst::dsls::json::JsonDsl::compile_typed(
                        pat_src.as_bytes(),
                    ) {
                        Ok(b)  => b.with_format(*fmt),
                        Err(_) => return Node::Done,
                    };
                    Some(built)
                }
            };
            let cur_compiled: &crate::cst::dsls::json::JsonCompiled =
                dyn_built.as_ref().unwrap_or_else(|| compiled.as_ref());
            let rows = cur_compiled.match_grouped(bytes, 0);
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
                        repo, rev, fs: file_id,
                        lo: *lo0 as u32, hi: *hi0 as u32,
                    };
                    child.at = store.intern_ref(focal);
                    let focal_slice = caps[0].3.as_ref();
                    child.value_id = store.intern_string(focal_slice);
                    for (name, lo, hi, value) in &caps {
                        let coord = Coord {
                            repo, rev, fs: file_id,
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
