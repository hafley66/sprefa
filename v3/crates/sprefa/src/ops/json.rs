//! json — walk structured data files (JSON / YAML / TOML) using brace patterns.
//!
//! Usage:
//!   fs(**/package.json) > json({ name: $N, version: $V })
//!   repo(r) > rev(main) > json({ ** : { image: $I } })   ← self-enumerates

use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream::StreamExt;
use tracing::{info_span, Instrument};

use rustc_hash::FxHashMap;

use crate::types::{Capture, Cursor, FilePath, ParseSite, SlotKey, Tri};
use crate::diagnostic::{Diagnostic, Renderer};
use crate::pattern::CompiledPattern;
use crate::op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation,
    Operator, Pipeline, ProgramCtx,
};
use crate::data::{parse_by_ext, DataKind, DataNode, AnyDataNode};
use crate::jq_path;
use crate::walk::compile::compile_steps;
use crate::walk::compiled::{CompiledKeyMatcher, CompiledStep};
use crate::walk::walker::{walk, Captures};
use crate::walk::brace_parse::{parse_body, ScanAnnotation};

// ---------------------------------------------------------------------------
// JSON_TREE slot
// ---------------------------------------------------------------------------

/// Newtype wrapping the parsed JSON/YAML/TOML tree. Keeps `SlotKey<JsonTree>`
/// distinct from any naive `SlotKey<AnyDataNode>` an unrelated op might declare.
pub struct JsonTree(pub Arc<AnyDataNode>);

impl std::fmt::Debug for JsonTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JsonTree(<{:?}>)", self.0.kind())
    }
}

/// Published by the json op on every emitted output cursor. Downstream ops
/// that want to walk the same tree without re-parsing read this slot via
/// `cursor.get_slot(JSON_TREE)`.
pub const JSON_TREE: SlotKey<JsonTree> = SlotKey::new();

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct JsonFactory;

impl Operator for JsonFactory {
    fn name(&self) -> &'static str { "json" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("json-none")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::WalkerPattern }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "json".to_string(),
            detail: "json({ key: $CAP })".to_string(),
            doc:    "# json\n\nWalk JSON/YAML/TOML files with a brace pattern. Binds captures from matching keys.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        // json({ pattern }) — body lives in the paren slot since {..} is inside (..),
        // so paren_src.src = "{ version: $VER }".
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(JsonDiag::ParseBody {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from("json requires a pattern argument (e.g. json({ key: $V }))"),
            }) as Box<dyn Diagnostic>]
        })?;

        let src = paren.src.trim();
        let (steps, annotations) = parse_body(src).map_err(|e| {
            let msg = e.to_string();
            let code = if msg.contains("requires a bare capture var") {
                "json/annotation-requires-capture"
            } else {
                "json/parse-body"
            };
            vec![Box::new(JsonDiag::ParseBodyCode {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from(msg.as_str()),
                code: Arc::from(code),
            }) as Box<dyn Diagnostic>]
        })?;

        let compiled = compile_steps(&steps).map_err(|e| {
            vec![Box::new(JsonDiag::ParseBody {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from(e.to_string().as_str()),
            }) as Box<dyn Diagnostic>]
        })?;

        Ok(Pipeline::Op(Arc::new(JsonOp {
            mode: JsonMode::Pattern(PatternState {
                compiled:    Arc::from(compiled.into_boxed_slice()),
                annotations: Arc::from(annotations.into_boxed_slice()),
            }),
            parse_site:  inv.parse_site.clone(),
        }).into()))
    }

    // Wildcard op: used by LSP partial-eval completion. Enumerates every
    // jq-path reachable in upstream JSON/YAML/TOML files, emits one
    // cursor per unique path. witness() returns the jq-path for labelling,
    // witness_insert() returns a sprf pattern for source insertion.
    fn wildcard_instance(&self, parse_site: Arc<ParseSite>) -> Option<Arc<dyn Op>> {
        Some(Arc::new(JsonOp {
            mode: JsonMode::Wildcard,
            parse_site,
        }))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct JsonOp {
    mode:       JsonMode,
    parse_site: Arc<ParseSite>,
}

pub struct PatternState {
    compiled:    Arc<[CompiledStep]>,
    annotations: Arc<[ScanAnnotation]>,
}

pub enum JsonMode {
    Pattern(PatternState),
    Wildcard,
}

const JSON_EXTS: &[&str] = &["json", "yaml", "yml", "toml"];

fn collect_pattern_captures(steps: &[CompiledStep]) -> Vec<Arc<str>> {
    let mut seen: Vec<Arc<str>> = Vec::new();
    fn push(seen: &mut Vec<Arc<str>>, name: &str) {
        if !seen.iter().any(|s| s.as_ref() == name) {
            seen.push(Arc::from(name));
        }
    }
    fn walk(seen: &mut Vec<Arc<str>>, steps: &[CompiledStep]) {
        for s in steps {
            match s {
                CompiledStep::Key { capture: Some(n), .. }
                | CompiledStep::KeyMatch { capture: Some(n), .. }
                | CompiledStep::Leaf { capture: Some(n) }
                | CompiledStep::CaptureAny { capture: Some(n) } => push(seen, n),
                CompiledStep::Object { entries } => {
                    for e in entries {
                        if let CompiledKeyMatcher::Capture(n) = &e.key { push(seen, n); }
                        walk(seen, &e.value);
                    }
                }
                CompiledStep::Array { item } => walk(seen, item),
                _ => {}
            }
        }
    }
    walk(&mut seen, steps);
    seen
}

fn file_ext(fp: &FilePath) -> Option<String> {
    fp.0.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
}

impl Op for JsonOp {
    fn name(&self) -> &'static str { "json" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn witness(&self, c: &Cursor) -> Option<Arc<str>> {
        match &self.mode {
            // Pattern mode: fs path is the witness (one cursor per matched file).
            JsonMode::Pattern(_) => {
                c.fs.as_ref().map(|fp| Arc::from(fp.0.to_string_lossy().as_ref()))
            }
            // Wildcard mode: pull latest json-evidence entry whose parse_site
            // matches — its `matched` slot holds the jq-path we stamped in pipe.
            JsonMode::Wildcard => {
                let site = self.parse_site.as_ref();
                c.evidence.iter().rev()
                    .find(|ev| ev.op_name == "json" && ev.parse_site.as_ref() == site)
                    .map(|ev| ev.matched.clone())
            }
        }
    }

    fn witness_insert(&self, c: &Cursor) -> Option<Arc<str>> {
        match &self.mode {
            JsonMode::Pattern(_) => self.witness(c),
            JsonMode::Wildcard => {
                let jq = self.witness(c)?;
                let sprf = jq_path_to_sprf_pattern(&jq)?;
                Some(Arc::from(sprf))
            }
        }
    }

    fn hover_self(&self) -> String {
        "# json\n\nWalk JSON/YAML/TOML files with a brace pattern. Binds captures from matching keys.".to_string()
    }

    fn binds_captures(&self) -> Vec<Arc<str>> {
        match &self.mode {
            JsonMode::Pattern(ps) => collect_pattern_captures(&ps.compiled),
            JsonMode::Wildcard    => vec![],
        }
    }

    fn hover_capture(&self, cap: &str, cursors: &[Cursor]) -> Option<String> {
        let site = self.parse_site.as_ref();
        let header = format!("**`${cap}`** values:");
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .filter(|c| c.evidence.iter().any(|ev|
                ev.op_name == "json" && ev.parse_site.as_ref() == site
            ))
            .filter_map(|c| {
                c.captures.get(cap).map(|capture| (
                    c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                    c.rev.to_string(),
                    capture.value.to_string(),
                ))
            })
            .collect();
        hover_render_grouped(&header, &entries)
    }

    fn hover_match(&self, site: &crate::types::ParseSite, cursors: &[Cursor]) -> Option<String> {
        let header = "**json** pattern site";
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .flat_map(|c| {
                c.evidence.iter()
                    .filter(|ev| ev.op_name == "json" && ev.parse_site.as_ref() == site)
                    .map(move |ev| (
                        c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                        c.rev.to_string(),
                        ev.matched.to_string(),
                    ))
                    .collect::<Vec<_>>()
            })
            .collect();
        if entries.is_empty() {
            return Some(format!("{header}\n\n(no matches yet)"));
        }
        hover_render_grouped(header, &entries)
    }

    fn pipe(&self, input: BoxStream<'static, Arc<[Cursor]>>, ctx: OpCtx)
        -> BoxStream<'static, Arc<[Cursor]>>
    {
        let _pipe_span = info_span!("op.json.pipe").entered();
        let ps = match &self.mode {
            JsonMode::Pattern(ps) => ps,
            JsonMode::Wildcard    => return wildcard_pipe(self.parse_site.clone(), input, ctx),
        };
        let compiled    = ps.compiled.clone();
        let annotations = ps.annotations.clone();
        let parse_site  = self.parse_site.clone();
        let reader      = ctx.reader.clone();
        let diags       = ctx.diags.clone();

        // Content-side stamping map: capture var -> sigil. Used to stamp
        // walker-extracted Captures with scan_pointer + Tri::Claimed. This
        // is the scout half of the claim/verify pair; a separate checker
        // pass downgrades unverified claims.
        // TODO: when line()/ast-grep land, hoist this stamping into a shared
        // walker-driver helper so it isn't re-implemented per op.
        let stamp_map: Arc<FxHashMap<Arc<str>, Arc<str>>> = {
            let mut m: FxHashMap<Arc<str>, Arc<str>> = FxHashMap::default();
            for ann in annotations.iter() {
                m.insert(Arc::<str>::from(ann.var.as_str()), ann.sigil.clone());
            }
            Arc::new(m)
        };

        // Per-file walk outcome = one output batch. A single input cursor may
        // enumerate multiple candidate files and therefore emit several batches.
        // Rows from one walk form a coherent sibling group (rule 4), so they
        // stay together in one Arc<[Cursor]>.
        input
            .flat_map(move |batch| {
                let _batch_span = info_span!("op.json.batch", n = batch.len()).entered();
                let cursors: Vec<Cursor> = batch.iter().cloned().collect();
                let compiled    = compiled.clone();
                let parse_site  = parse_site.clone();
                let reader      = reader.clone();
                let diags       = diags.clone();
                let stamp_map   = stamp_map.clone();
                futures_util::stream::iter(cursors)
                    .flat_map(move |c| {
                        let compiled    = compiled.clone();
                        let parse_site  = parse_site.clone();
                        let reader      = reader.clone();
                        let diags       = diags.clone();
                        let stamp_map   = stamp_map.clone();
                        // Compute per-file batches inside an async block, then
                        // emit each as its own stream item.
                        let per_cursor = async move {
                            // Slot-reuse fast path: upstream already parsed this
                            // file. Skip reader fetch + parse; drive walker
                            // directly on the Arc-shared tree. Only safe when
                            // `c.fs` is bound (single-file focus) — self-enum
                            // (`c.fs == None`) enumerates candidates and the
                            // stored tree addresses only one of them.
                            if let (Some(fp), Some(tree_arc)) =
                                (c.fs.as_ref(), c.get_slot(JSON_TREE))
                            {
                                match file_ext(fp) {
                                    Some(ref e) if JSON_EXTS.contains(&e.as_str()) => {}
                                    _ => return Vec::<Arc<[Cursor]>>::new(),
                                }
                                // tree_arc: Arc<JsonTree>; inner: Arc<AnyDataNode>.
                                let inner: Arc<AnyDataNode> = tree_arc.0.clone();
                                let outcome = walk(inner.as_ref(), &compiled);
                                let rows = outcome.rows;

                                if rows.is_empty() {
                                    let dump = tree_dump(inner.as_ref(), 4, 200);
                                    diags.0(Box::new(JsonDiag::NoMatch {
                                        site: (*parse_site).clone(),
                                        file: Arc::from(fp.0.to_string_lossy().as_ref()),
                                        dump: Arc::from(dump.as_str()),
                                    }));
                                    return Vec::<Arc<[Cursor]>>::new();
                                }
                                if !outcome.missed_keys.is_empty() {
                                    diags.0(Box::new(JsonDiag::PartialMatch {
                                        site:   (*parse_site).clone(),
                                        file:   Arc::from(fp.0.to_string_lossy().as_ref()),
                                        missed: outcome.missed_keys.clone(),
                                    }));
                                }
                                // Translate capture spans → cursor.byte_range.
                                // Captures already index into the full file bytes
                                // (walker sees the whole parsed tree), so no
                                // offset translation is needed here.
                                let raw_bytes_opt = c.content.as_ref().cloned();
                                let file_batch: Vec<Cursor> = rows.into_iter().map(|row| {
                                    let span = row_byte_span(&row.captures);
                                    let mut c2 = c.clone();
                                    c2.fs = Some(fp.clone());
                                    for (name, wc) in row.captures {
                                        let kind = raw_bytes_opt.as_ref()
                                            .map(|b| classify_capture(b, wc.byte_start, wc.byte_end))
                                            .unwrap_or(crate::types::CaptureKind::Synthesized);
                                        let cap = {
                                            use crate::types::CaptureKind;
                                            let base = match kind {
                                                CaptureKind::SpanBacked { span } =>
                                                    Capture::span_backed(wc.text.clone(), span),
                                                CaptureKind::Synthesized =>
                                                    Capture::new(wc.text.clone()),
                                            };
                                            match stamp_map.get(&name) {
                                                Some(sigil) => base.with_scan(sigil.clone(), Tri::Claimed),
                                                None => base,
                                            }
                                        };
                                        c2.captures.insert(name, cap);
                                    }
                                    c2.set_slot(JSON_TREE, JsonTree(Arc::clone(&inner)));
                                    if let Some(r) = span {
                                        c2.byte_range = Some(r);
                                    }
                                    c2
                                }).collect();
                                return vec![Arc::<[Cursor]>::from(file_batch.into_boxed_slice())];
                            }

                            // Source-of-truth contract: cursor.content (sliced by
                                                        // byte_range) is authoritative when set. Reader is the
                                                        // populate path used only when content is empty. This is
                                                        // what makes `… > &.$STR > json(…)` parse the rebased
                                                        // bytes instead of re-fetching the outer file.
                            let candidates: Vec<(FilePath, Bytes)> = if let Some(content) = c.content.as_ref() {
                                let bytes = match &c.byte_range {
                                    Some(r) => content.slice(r.clone()),
                                    None    => content.as_ref().clone(),
                                };
                                let fp = c.fs.clone().unwrap_or_else(|| {
                                    FilePath(Arc::from(std::path::Path::new("<rebased>")))
                                });
                                vec![(fp, bytes)]
                            } else {
                                match &c.fs {
                                    Some(fp) => {
                                        match file_ext(fp) {
                                            Some(ref e) if JSON_EXTS.contains(&e.as_str()) => {}
                                            _ => return Vec::<Arc<[Cursor]>>::new(),
                                        }
                                        let mut s = reader.bytes(&c.repo, &c.rev, fp);
                                        let raw = s.next().await.unwrap_or_default();
                                        vec![(fp.clone(), raw)]
                                    }
                                    None => {
                                        let all = CompiledPattern::compile("**")
                                            .expect("'**' is a valid glob");
                                        let mut s = reader.files(&c.repo, &c.rev, &all);
                                        let files = s.next().await.unwrap_or_default();
                                        let mut out = vec![];
                                        for fp in files {
                                            match file_ext(&fp) {
                                                Some(ref e) if JSON_EXTS.contains(&e.as_str()) => {}
                                                _ => continue,
                                            }
                                            let mut s2 = reader.bytes(&c.repo, &c.rev, &fp);
                                            let raw = s2.next().await.unwrap_or_default();
                                            out.push((fp, raw));
                                        }
                                        out
                                    }
                                }
                            };

                            if c.fs.is_none() && candidates.is_empty() {
                                diags.0(Box::new(JsonDiag::NoCandidates {
                                    site: (*parse_site).clone(),
                                    msg:  Arc::from(format!(
                                        "json() found no .json/.yaml/.yml/.toml files in {}@{}",
                                        c.repo, c.rev
                                    )),
                                }));
                            }

                            let mut batches: Vec<Arc<[Cursor]>> = Vec::new();
                            for (fp, raw) in candidates {
                                // Default to "json" when fs has no recognized json/yaml/toml ext
                                                                // (e.g. synthesized "<rebased>" path from the
                                                                // content-priority branch above).
                                let ext = file_ext(&fp)
                                    .filter(|e| JSON_EXTS.contains(&e.as_str()))
                                    .unwrap_or_else(|| "json".to_string());
                                let arc_bytes = Arc::new(raw.clone());
                                let tree = match parse_by_ext(&ext, arc_bytes) {
                                    Ok(t) => t,
                                    Err(e) => {
                                        diags.0(Box::new(JsonDiag::ParseError {
                                            site: (*parse_site).clone(),
                                            msg:  Arc::from(e.to_string().as_str()),
                                        }));
                                        continue;
                                    }
                                };

                                let tree_arc = Arc::new(tree);
                                let outcome = walk(tree_arc.as_ref(), &compiled);
                                let rows = outcome.rows;

                                if rows.is_empty() {
                                    let dump = tree_dump(tree_arc.as_ref(), 4, 200);
                                    diags.0(Box::new(JsonDiag::NoMatch {
                                        site: (*parse_site).clone(),
                                        file: Arc::from(fp.0.to_string_lossy().as_ref()),
                                        dump: Arc::from(dump.as_str()),
                                    }));
                                    continue;
                                }

                                if !outcome.missed_keys.is_empty() {
                                    diags.0(Box::new(JsonDiag::PartialMatch {
                                        site:   (*parse_site).clone(),
                                        file:   Arc::from(fp.0.to_string_lossy().as_ref()),
                                        missed: outcome.missed_keys.clone(),
                                    }));
                                }

                                // Rows from one file = one output batch (the
                                // grouped sibling set from that walk). Every
                                // row cursor shares the same Arc<AnyDataNode>
                                // via JSON_TREE — downstream json > json reuses
                                // without re-parsing.
                                let content_arc = Arc::new(raw.clone());
                                let file_batch: Vec<Cursor> = rows.into_iter().map(|row| {
                                    let span = row_byte_span(&row.captures);
                                    let mut c2 = c.clone();
                                    c2.fs = Some(fp.clone());
                                    c2.content = Some(content_arc.clone());
                                    for (name, wc) in row.captures {
                                        let kind = classify_capture(&raw, wc.byte_start, wc.byte_end);
                                        let cap = {
                                            use crate::types::CaptureKind;
                                            let base = match kind {
                                                CaptureKind::SpanBacked { span } =>
                                                    Capture::span_backed(wc.text.clone(), span),
                                                CaptureKind::Synthesized =>
                                                    Capture::new(wc.text.clone()),
                                            };
                                            match stamp_map.get(&name) {
                                                Some(sigil) => base.with_scan(sigil.clone(), Tri::Claimed),
                                                None => base,
                                            }
                                        };
                                        c2.captures.insert(name, cap);
                                    }
                                    c2.set_slot(JSON_TREE, JsonTree(Arc::clone(&tree_arc)));
                                    if let Some(r) = span {
                                        c2.byte_range = Some(r);
                                    }
                                    c2
                                }).collect();
                                batches.push(Arc::<[Cursor]>::from(file_batch.into_boxed_slice()));
                            }
                            batches
                        };
                        futures_util::stream::once(per_cursor)
                            .flat_map(|batches| futures_util::stream::iter(batches))
                    })
            })
            .boxed()
    }
}

// ---------------------------------------------------------------------------
// Wildcard-mode pipe: enumerate jq-paths over upstream data files
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Segment {
    Key(String),
    ArrayHop,
}

fn wildcard_pipe(
    parse_site: Arc<ParseSite>,
    input:      BoxStream<'static, Arc<[Cursor]>>,
    ctx:        OpCtx,
) -> BoxStream<'static, Arc<[Cursor]>> {
    let reader = ctx.reader.clone();
    input
        .flat_map(move |batch| {
            let _batch_span = info_span!("op.json.batch", n = batch.len()).entered();
            let cursors: Vec<Cursor> = batch.iter().cloned().collect();
            let parse_site = parse_site.clone();
            let reader     = reader.clone();
            futures_util::stream::iter(cursors)
                .then(move |c| {
        let parse_site = parse_site.clone();
        let reader     = reader.clone();
        async move {
            let candidates: Vec<(FilePath, Bytes)> = match &c.fs {
                Some(fp) => {
                    match file_ext(fp) {
                        Some(ref e) if JSON_EXTS.contains(&e.as_str()) => {}
                        _ => return Arc::<[Cursor]>::from(Vec::<Cursor>::new().into_boxed_slice()),
                    }
                    let mut s = reader.bytes(&c.repo, &c.rev, fp);
                    let raw = s.next().await.unwrap_or_default();
                    vec![(fp.clone(), raw)]
                }
                None => {
                    let all = CompiledPattern::compile("**").expect("'**' is a valid glob");
                    let mut s = reader.files(&c.repo, &c.rev, &all);
                    let files = s.next().await.unwrap_or_default();
                    let mut out = vec![];
                    for fp in files {
                        match file_ext(&fp) {
                            Some(ref e) if JSON_EXTS.contains(&e.as_str()) => {}
                            _ => continue,
                        }
                        let mut s2 = reader.bytes(&c.repo, &c.rev, &fp);
                        let raw = s2.next().await.unwrap_or_default();
                        out.push((fp, raw));
                    }
                    out
                }
            };

            let mut seen: std::collections::HashSet<String> = Default::default();
            let mut out_cursors: Vec<Cursor> = vec![];

            'files: for (fp, raw) in candidates {
                let ext = match file_ext(&fp) { Some(e) => e, None => continue };
                let arc_bytes = Arc::new(raw);
                let tree = match parse_by_ext(&ext, arc_bytes) {
                    Ok(t)  => t,
                    Err(_) => continue, // wildcard mode silent — diagnostics are for real runs
                };

                let mut paths: Vec<String> = Vec::new();
                enumerate_paths(&tree, &mut Vec::new(), 0, &mut paths);

                for p in paths {
                    if !seen.insert(p.clone()) { continue; }
                    let mut c2 = c.clone();
                    c2.fs = Some(fp.clone());
                    c2.evidence.push(crate::types::OpEvidence {
                        op_name:    "json",
                        parse_site: parse_site.clone(),
                        matched:    Arc::from(p.as_str()),
                        capture:    None,
                    });
                    out_cursors.push(c2);
                    if seen.len() >= 1000 { break 'files; }
                }
            }

            // Wildcard mode is per-cursor fan-out; one input cursor = one
            // output batch containing every unique jq-path discovered.
            Arc::<[Cursor]>::from(out_cursors.into_boxed_slice())
        }
                })
        })
        .boxed()
}

const WILDCARD_DEPTH_CAP: usize = 8;

fn enumerate_paths(
    node:  &AnyDataNode,
    segs:  &mut Vec<Segment>,
    depth: usize,
    out:   &mut Vec<String>,
) {
    if depth > WILDCARD_DEPTH_CAP { return; }
    match node.kind() {
        DataKind::Object => {
            for (k, v) in node.entries() {
                let key = k.as_scalar_text().map(|s| s.into_owned()).unwrap_or_default();
                segs.push(Segment::Key(key));
                enumerate_paths(&v, segs, depth + 1, out);
                segs.pop();
            }
        }
        DataKind::Array => {
            // Single representative shape per array (items[0]); the emitted
            // jq-path form is `.…[]`, matching the [...$CAP] walker grammar.
            if let Some(first) = node.items().next() {
                segs.push(Segment::ArrayHop);
                enumerate_paths(&first, segs, depth + 1, out);
                segs.pop();
            } else {
                segs.push(Segment::ArrayHop);
                out.push(segs_to_jq(segs));
                segs.pop();
            }
        }
        DataKind::Scalar | DataKind::Null => {
            if !segs.is_empty() {
                out.push(segs_to_jq(segs));
            }
        }
    }
}

fn segs_to_jq(segs: &[Segment]) -> String {
    let mut s = String::new();
    for seg in segs {
        match seg {
            Segment::Key(k)   => { s.push('.'); s.push_str(k); }
            Segment::ArrayHop => { s.push_str("[]"); }
        }
    }
    s
}

// ---------------------------------------------------------------------------
// jq-path → sprf pattern lowering
// ---------------------------------------------------------------------------

fn is_ident_safe(k: &str) -> bool {
    let mut it = k.chars();
    match it.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    it.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn cap_name_from_key(k: &str) -> String {
    k.replace('-', "_").to_ascii_uppercase()
}

/// Lower a jq-path like `.a.b[].c` into a sprf brace pattern.
/// Empty / root paths return None so callers skip the suggestion.
fn jq_path_to_sprf_pattern(path: &str) -> Option<String> {
    // Parse into segments.
    let segs = parse_jq_path(path)?;
    if segs.is_empty() { return None; }

    // Find last non-array key for capture name.
    let last_key = segs.iter().rev().find_map(|s| match s {
        Segment::Key(k) => Some(k.as_str()),
        _ => None,
    })?;
    let cap = cap_name_from_key(last_key);

    // Build from innermost outward.
    let n = segs.len();
    let trailing_array = matches!(segs.last(), Some(Segment::ArrayHop));

    // Inner starts as either leaf-capture `$CAP` or a leaf-final array
    // collector `[...$CAP]` wrapping nothing else.
    let mut inner: String;
    let start_ix: usize;
    if trailing_array {
        inner = format!("[...${}]", cap);
        start_ix = n - 1;
    } else {
        inner = format!("${}", cap);
        start_ix = n;
    }

    // Walk remaining segments right-to-left, wrapping.
    for i in (0..start_ix).rev() {
        match &segs[i] {
            Segment::Key(k) => {
                let key_tok = if is_ident_safe(k) { k.clone() } else { format!("\"{}\"", k) };
                inner = format!("{{ {}: {} }}", key_tok, inner);
            }
            Segment::ArrayHop => {
                // Non-leaf array: wildcard iterator.
                inner = format!("[...$_] {}", inner);
            }
        }
    }
    Some(inner)
}

fn parse_jq_path(path: &str) -> Option<Vec<Segment>> {
    let s = path.trim();
    if s.is_empty() || s == "." { return None; }
    let mut segs = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' { i += 1; }
                if start == i { return None; }
                segs.push(Segment::Key(s[start..i].to_string()));
            }
            b'[' => {
                // Accept only `[]` for now; indexed forms collapse to []
                if bytes.get(i+1) == Some(&b']') {
                    segs.push(Segment::ArrayHop);
                    i += 2;
                } else {
                    // Skip to ] (indexed subscripts from jq get flattened).
                    while i < bytes.len() && bytes[i] != b']' { i += 1; }
                    if i < bytes.len() { i += 1; }
                    segs.push(Segment::ArrayHop);
                }
            }
            _ => return None,
        }
    }
    Some(segs)
}

// ---------------------------------------------------------------------------
// byte_range narrowing helper
// ---------------------------------------------------------------------------

/// Compute a byte window covering every capture in a walker row. Returns
/// `Some(start..end)` when at least one capture is present, `None` otherwise.
///
/// The walker surfaces per-capture byte ranges (scalar leaves and keys); it
/// does not track the "matched subtree root" span per row. The union of
/// capture spans is a cheap, correct proxy: for a single-capture pattern it
/// equals the value's own span (matches the design memo §4.1 `r_a`), and for
/// multi-capture rows it bounds the captured data. A later walker touch can
/// tighten this to the true subtree root span.
/// Classify a walker capture by its raw bytes in the source file.
///
/// JSON objects `{...}` and arrays `[...]` are span-backed: the raw bytes
/// are valid JSON and can be re-parsed or narrowed by CursorRef. Everything
/// else (strings — already unescaped in `wc.text` — numbers, booleans,
/// null) is synthesized: the raw span includes quotes or differs in encoding
/// from the logical value.
///
/// Falls back to `Synthesized` when `bs == be` (empty range) or when
/// `bytes` is shorter than the span (shouldn't happen; defensive).
fn classify_capture(bytes: &[u8], bs: u32, be: u32) -> crate::types::CaptureKind {
    use crate::types::CaptureKind;
    let bs = bs as usize;
    let be = be as usize;
    if be <= bs || be > bytes.len() {
        return CaptureKind::Synthesized;
    }
    let first = bytes[bs..be]
        .iter()
        .copied()
        .find(|b| !b.is_ascii_whitespace());
    match first {
        Some(b'{') | Some(b'[') => CaptureKind::SpanBacked { span: bs..be },
        _                       => CaptureKind::Synthesized,
    }
}

fn row_byte_span(caps: &Captures) -> Option<std::ops::Range<usize>> {
    let mut iter = caps.values();
    let first = iter.next()?;
    let mut lo = first.byte_start;
    let mut hi = first.byte_end;
    for wc in iter {
        if wc.byte_start < lo { lo = wc.byte_start; }
        if wc.byte_end   > hi { hi = wc.byte_end;   }
    }
    Some((lo as usize)..(hi as usize))
}

// ---------------------------------------------------------------------------
// Tree dump helper (file-private)
// ---------------------------------------------------------------------------

/// Render a depth-limited jq-path listing of `node` up to `max_depth` levels.
/// Each line is one leaf or container header. Truncated at `max_lines` with
/// "...(more)" appended if the tree exceeds the cap.
fn tree_dump(node: &AnyDataNode, max_depth: u32, max_lines: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    dump_node(node, &mut String::new(), 0, max_depth, &mut lines, max_lines);
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        lines.push("...(more)".to_owned());
    }
    lines.join("\n")
}

fn dump_node(
    node:      &AnyDataNode,
    path:      &mut String,
    depth:     u32,
    max_depth: u32,
    lines:     &mut Vec<String>,
    max_lines: usize,
) {
    if lines.len() >= max_lines { return; }
    match node.kind() {
        DataKind::Object => {
            let label = if path.is_empty() { ".".to_owned() } else { path.clone() };
            lines.push(format!("{label}: {{}}"));
            if depth < max_depth {
                for (k, v) in node.entries() {
                    if lines.len() >= max_lines { return; }
                    let key_text = k.as_scalar_text()
                        .map(|s| s.into_owned())
                        .unwrap_or_default();
                    let mut child_path = path.clone();
                    jq_path::push_key(&mut child_path, &key_text);
                    dump_node(&v, &mut child_path, depth + 1, max_depth, lines, max_lines);
                }
            }
        }
        DataKind::Array => {
            let label = if path.is_empty() { ".".to_owned() } else { path.clone() };
            lines.push(format!("{label}: []"));
            if depth < max_depth {
                for (i, item) in node.items().enumerate() {
                    if lines.len() >= max_lines { return; }
                    let mut child_path = path.clone();
                    jq_path::push_index(&mut child_path, i as u32);
                    dump_node(&item, &mut child_path, depth + 1, max_depth, lines, max_lines);
                }
            }
        }
        DataKind::Scalar => {
            let val = node.as_scalar_text()
                .map(|s| s.into_owned())
                .unwrap_or_default();
            let label = if path.is_empty() { ".".to_owned() } else { path.clone() };
            // Truncate long scalar values for readability
            let display = if val.len() > 60 { format!("{}...", &val[..60]) } else { val };
            lines.push(format!("{label}: {display}"));
        }
        DataKind::Null => {
            let label = if path.is_empty() { ".".to_owned() } else { path.clone() };
            lines.push(format!("{label}: null"));
        }
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum JsonDiag {
    ParseBody {
        site: crate::types::ParseSite,
        msg:  Arc<str>,
    },
    ParseBodyCode {
        site: crate::types::ParseSite,
        msg:  Arc<str>,
        code: Arc<str>,
    },
    ParseError {
        site: crate::types::ParseSite,
        msg:  Arc<str>,
    },
    NoCandidates {
        site: crate::types::ParseSite,
        msg:  Arc<str>,
    },
    NoMatch {
        site: crate::types::ParseSite,
        file: Arc<str>,
        dump: Arc<str>,
    },
    PartialMatch {
        site:   crate::types::ParseSite,
        file:   Arc<str>,
        missed: Vec<Arc<str>>,
    },
}

impl Diagnostic for JsonDiag {
    fn code(&self) -> &str {
        match self {
            JsonDiag::ParseBody      { .. }       => "json/parse-body",
            JsonDiag::ParseBodyCode  { code, .. } => code,
            JsonDiag::ParseError     { .. }       => "json/parse-error",
            JsonDiag::NoCandidates   { .. }       => "json/no-candidates",
            JsonDiag::NoMatch        { .. }       => "json/no-match",
            JsonDiag::PartialMatch   { .. }       => "json/partial-match",
        }
    }
    fn severity(&self) -> crate::types::Severity {
        match self {
            JsonDiag::ParseBody     { .. }
            | JsonDiag::ParseBodyCode { .. }
            | JsonDiag::ParseError  { .. } => crate::types::Severity::Error,
            JsonDiag::NoCandidates  { .. } => crate::types::Severity::Warn,
            JsonDiag::NoMatch       { .. } => crate::types::Severity::Error,
            JsonDiag::PartialMatch  { .. } => crate::types::Severity::Warn,
        }
    }
    fn primary(&self) -> &crate::types::ParseSite {
        match self {
            JsonDiag::ParseBody      { site, .. } => site,
            JsonDiag::ParseBodyCode  { site, .. } => site,
            JsonDiag::ParseError     { site, .. } => site,
            JsonDiag::NoCandidates   { site, .. } => site,
            JsonDiag::NoMatch        { site, .. } => site,
            JsonDiag::PartialMatch   { site, .. } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            JsonDiag::ParseBody { site, msg } => {
                out.header(self.code(), self.severity(), msg);
                out.primary(site);
            }
            JsonDiag::ParseBodyCode { site, msg, .. } => {
                out.header(self.code(), self.severity(), msg);
                out.primary(site);
            }
            JsonDiag::ParseError { site, msg } => {
                out.header(self.code(), self.severity(), msg);
                out.primary(site);
            }
            JsonDiag::NoCandidates { site, msg } => {
                out.header(self.code(), self.severity(), msg);
                out.primary(site);
            }
            JsonDiag::NoMatch { site, file, dump } => {
                let msg = format!("pattern found no matches in {file} (file parsed OK; keys/shape differ)");
                out.header(self.code(), self.severity(), &msg);
                out.primary(site);
                out.note(dump);
            }
            JsonDiag::PartialMatch { site, file, missed } => {
                let joined = missed.iter().map(|k| k.as_ref()).collect::<Vec<_>>().join(", ");
                let msg = format!("pattern partially matched in {file}; missing keys: {joined}");
                out.header(self.code(), self.severity(), &msg);
                out.primary(site);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::types::{Capture, FilePath, OpEvidence, RunId};

    fn dummy_site() -> Arc<ParseSite> {
        Arc::new(ParseSite {
            file:       Arc::from(Path::new("test.sprf")),
            path:       Arc::from(vec![].into_boxed_slice()),
            byte_range: 0..1,
        })
    }

    fn base_cursor(rev: &str, fs: Option<&str>) -> Cursor {
        Cursor {
            run_id: RunId(0),
            repo:   Arc::from("org/repo"),
            rev:    Arc::from(rev),
            fs:     fs.map(|p| FilePath(Arc::from(Path::new(p)))),
            ..Default::default()
        }
    }

    fn make_op(site: &Arc<ParseSite>) -> JsonOp {
        use crate::walk::brace_parse::parse_body;
        use crate::walk::compile::compile_steps;
        let (steps, anns) = parse_body("{ name: $N }").unwrap();
        let compiled = compile_steps(&steps).unwrap();
        JsonOp {
            mode: JsonMode::Pattern(PatternState {
                compiled:    Arc::from(compiled.into_boxed_slice()),
                annotations: Arc::from(anns.into_boxed_slice()),
            }),
            parse_site:  site.clone(),
        }
    }

    #[test]
    fn hover_capture_groups_by_file_rev() {
        let site = dummy_site();
        let op = make_op(&site);

        let mut c1 = base_cursor("main", Some("crates/a/Cargo.toml"));
        c1.evidence.push(OpEvidence {
            op_name:    "json",
            parse_site: site.clone(),
            matched:    Arc::from("alpha"),
            capture:    None,
        });
        c1.captures.insert(Arc::from("N"), Capture::new(Arc::from("alpha")));

        let mut c2 = base_cursor("main", Some("crates/b/Cargo.toml"));
        c2.evidence.push(OpEvidence {
            op_name:    "json",
            parse_site: site.clone(),
            matched:    Arc::from("beta"),
            capture:    None,
        });
        c2.captures.insert(Arc::from("N"), Capture::new(Arc::from("beta")));

        let mut c3 = base_cursor("v2", Some("crates/a/Cargo.toml"));
        c3.evidence.push(OpEvidence {
            op_name:    "json",
            parse_site: site.clone(),
            matched:    Arc::from("gamma"),
            capture:    None,
        });
        c3.captures.insert(Arc::from("N"), Capture::new(Arc::from("gamma")));

        let md = op.hover_capture("N", &[c1, c2, c3]).unwrap();

        assert!(md.contains("### `crates/a/Cargo.toml`"), "missing a/ heading: {md}");
        assert!(md.contains("- `alpha`"), "missing alpha: {md}");
        assert!(md.contains("### `crates/b/Cargo.toml`"), "missing b/ heading: {md}");
        assert!(md.contains("- `beta`"), "missing beta: {md}");
        assert!(md.contains("- `gamma`"), "missing gamma: {md}");
    }

    #[test]
    fn hover_match_groups_by_file_rev() {
        let site = dummy_site();
        let op = make_op(&site);

        let mut c1 = base_cursor("main", Some("package.json"));
        c1.evidence.push(OpEvidence {
            op_name:    "json",
            parse_site: site.clone(),
            matched:    Arc::from("foo"),
            capture:    None,
        });
        let mut c2 = base_cursor("main", Some("sub/package.json"));
        c2.evidence.push(OpEvidence {
            op_name:    "json",
            parse_site: site.clone(),
            matched:    Arc::from("bar"),
            capture:    None,
        });

        let md = op.hover_match(site.as_ref(), &[c1, c2]).unwrap();

        assert!(md.contains("### `package.json`"), "missing package.json heading: {md}");
        assert!(md.contains("- `foo`"), "missing foo: {md}");
        assert!(md.contains("### `sub/package.json`"), "missing sub/package.json heading: {md}");
        assert!(md.contains("- `bar`"), "missing bar: {md}");
    }
}
