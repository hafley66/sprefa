//! Op + Operator traits, OpCtx, OpInvocation, Pipeline.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex};

use futures_core::stream::BoxStream;

use crate::_0_types::{Capture, Cursor, OpEvidence, OpId, ParseSite, PathSeg, RunEvent, RunId, Severity, SprfPath, Tri};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_2_config::Config;
use crate::_3_reader::Reader;
use crate::_4_writer::Writer;
use crate::_12_result_store::ResultStore;

// ---------------------------------------------------------------------------
// OpInvocation — host parser output, pre-lower
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OpInvocation {
    pub name:       Arc<str>,
    pub brackets:   Vec<BracketSlot>,
    pub paren_src:  Option<ParenSlot>,
    pub brace_src:  Option<BraceSlot>,
    pub parse_site: Arc<ParseSite>,
    /// Cross-refs (`${rule.$VAR}`) discovered in paren/bracket tokens at
    /// host-parse time. Feeds the rule DAG build. Walker-embedded cross-refs
    /// inside brace bodies are gathered separately at lower time.
    pub crossrefs:  Vec<CrossRefOccurrence>,
}

/// One `${rule.$VAR}` cross-ref token discovered at host-parse time.
///
/// `rule` and `var` are the parsed components inside the braces.
/// `byte_range` is the absolute byte span of the entire `${...}` token in
/// the original source — used to anchor `xref/empty-join` and friends so
/// the diagnostic underlines the exact ref that caused the problem.
///
/// Populated by `_8_parse.rs::scan_slot_crossrefs` over paren, bracket,
/// and brace slots. Consumed by:
///   - `ops/_0_rule.rs::parse` to build `RuleHandle.depends_on` (DAG edges)
///   - `_5_op.rs::expand_xrefs` to look up source rows in `ResultStore`
#[derive(Debug, Clone)]
pub struct CrossRefOccurrence {
    pub rule:       Arc<str>,
    pub var:        Arc<str>,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct BracketSlot { pub src: Arc<str>, pub byte_range: Range<usize> }
#[derive(Debug, Clone)]
pub struct ParenSlot   { pub src: Arc<str>, pub byte_range: Range<usize> }
#[derive(Debug, Clone)]
pub struct BraceSlot   { pub src: Arc<str>, pub byte_range: Range<usize> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BraceMode { DefaultFork, CustomSprf, WalkerPattern }

#[derive(Debug, Clone)]
pub struct GrammarRef(pub Arc<str>);

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

pub enum Pipeline {
    Op     (LoweredOp),
    Seq    (Vec<Pipeline>),
    Fork   (Vec<ForkBranch>),
    Switch { on: ChannelSelector, arms: Vec<(Arc<str>, Pipeline)> },
}

/// An op together with its compile-time cross-ref bindings.
///
/// Wraps `Arc<dyn Op>` so the framework can splice cross-ref expansion
/// (`expand_xrefs`) ahead of the op's `pipe` call without changing the
/// `Op` trait. This keeps op authors out of the cross-ref machinery
/// entirely — they write `pipe(input, ctx)` and the framework guarantees
/// `input` already has source-row captures seeded.
///
/// `xrefs` is the list of `${target.$VAR}` tokens that appeared anywhere
/// in this op's slots (paren / bracket / brace). Empty for the common
/// no-xref case, in which `expand_xrefs` short-circuits to zero alloc.
///
/// Constructed at lower time. `lower_chain` and `RuleFactory::parse`
/// build it from `OpInvocation.crossrefs`. The `From<Arc<T>>` and
/// `From<Arc<dyn Op>>` impls below let existing op factories keep using
/// `Arc::new(MyOp { ... }).into()` instead of explicit wrapping.
#[derive(Clone)]
pub struct LoweredOp {
    pub op:    Arc<dyn Op>,
    pub xrefs: Arc<[CrossRefOccurrence]>,
}

impl LoweredOp {
    pub fn new(op: Arc<dyn Op>, xrefs: Arc<[CrossRefOccurrence]>) -> Self {
        Self { op, xrefs }
    }

    /// Bare-op constructor for the empty-xref hot path.
    pub fn bare(op: Arc<dyn Op>) -> Self {
        Self { op, xrefs: Arc::from(Vec::<CrossRefOccurrence>::new().into_boxed_slice()) }
    }
}

/// Ergonomics: any concrete op `Arc::new(MyOp { ... })` flows into a
/// `Pipeline::Op(_)` slot via `Arc::new(...).into()`. Existing op factories
/// only need a `.into()` rather than wrapping in `LoweredOp::bare(...)`.
impl<T: Op + 'static> From<Arc<T>> for LoweredOp {
    fn from(op: Arc<T>) -> Self { LoweredOp::bare(op as Arc<dyn Op>) }
}

impl From<Arc<dyn Op>> for LoweredOp {
    fn from(op: Arc<dyn Op>) -> Self { LoweredOp::bare(op) }
}

/// One arm of a Fork. Carries its parse_site so framework path-tagging can
/// emit `PathSeg::ForkArm { index, parse_site }` per cursor that flows through.
pub struct ForkBranch {
    pub parse_site: Arc<ParseSite>,
    pub pipeline:   Pipeline,
}

impl Pipeline {
    /// Rx-style fold. Framework owns SprfPath population:
    ///   - Op  → push `PathSeg::Op { name, parse_site, step }` per emitted cursor
    ///   - Seq → pass step index to children; no seg of its own
    ///   - Fork→ push `PathSeg::ForkArm { index, parse_site }` per arm's output
    pub fn run(
        &self,
        input: BoxStream<'static, Cursor>,
        ctx:   OpCtx,
    ) -> BoxStream<'static, Cursor> {
        self.run_with_step(input, ctx, 0)
    }

    fn run_with_step(
        &self,
        input: BoxStream<'static, Cursor>,
        ctx:   OpCtx,
        step:  u16,
    ) -> BoxStream<'static, Cursor> {
        use futures_util::stream::{self, StreamExt};
        match self {
            Pipeline::Op(lop) => {
                let op  = &lop.op;
                let name = Arc::<str>::from(op.name());
                let ps   = op.parse_site().clone();
                let collect = ctx.config.runtime.collect_witnesses;
                let op2 = op.clone();
                let limit = ctx.config.runtime.xref_cartesian_limit;
                let input = expand_xrefs(input, lop.xrefs.clone(), ps.clone(), ctx.clone(), limit);
                op.pipe(input, ctx).map(move |mut c| {
                    if collect {
                        if let Some(v) = op2.witness(&c) {
                            c.evidence.push(OpEvidence {
                                op_name:    op2.name(),
                                parse_site: op2.parse_site().clone(),
                                matched:    v,
                                capture:    op2.capture_name(),
                            });
                        }
                    }
                    push_path(c, PathSeg::Op {
                        name:       name.clone(),
                        parse_site: ps.clone(),
                        step,
                    })
                }).boxed()
            }
            Pipeline::Seq(children) => children
                .iter()
                .enumerate()
                .fold(input, |acc, (i, child)| child.run_with_step(acc, ctx.clone(), i as u16)),
            Pipeline::Fork(arms) => {
                // Distribute parent: buffer, replay to each arm, tag with ForkArm, union.
                // TODO: real shareReplay(1) for daemon mode.
                use futures::executor::block_on;
                let buffered: Vec<Cursor> = block_on(input.collect());
                let mut merged: Vec<Cursor> = Vec::new();
                for (i, arm) in arms.iter().enumerate() {
                    let s = stream::iter(buffered.clone()).boxed();
                    let out: Vec<Cursor> = block_on(arm.pipeline.run(s, ctx.clone()).collect());
                    let ps = arm.parse_site.clone();
                    let idx = i as u16;
                    merged.extend(out.into_iter().map(|c| {
                        push_path(c, PathSeg::ForkArm { index: idx, parse_site: ps.clone() })
                    }));
                }
                stream::iter(merged).boxed()
            }
            Pipeline::Switch { .. } => unimplemented!("Pipeline::Switch"),
        }
    }
}

fn push_path(mut c: Cursor, seg: PathSeg) -> Cursor {
    let mut v: Vec<PathSeg> = c.path.0.iter().cloned().collect();
    v.push(seg);
    c.path = SprfPath(Arc::from(v.into_boxed_slice()));
    c
}

#[derive(Debug, Clone)]
pub enum ChannelSelector {
    Capture(Arc<str>),
    Provenance(Arc<str>),
    PathSegment,
}

// ---------------------------------------------------------------------------
// ProgramCtx
// ---------------------------------------------------------------------------

pub struct ProgramCtx {
    pub rules:     HashMap<Arc<str>, RuleHandle>,
    pub constants: HashMap<Arc<str>, Capture>,
    pub config:    Arc<Config>,
    pub registry:  Arc<crate::_10_registry::OperatorRegistry>,
}

#[derive(Clone)]
pub struct RuleHandle {
    pub name:       Arc<str>,
    pub parse_site: Arc<ParseSite>,
    pub captures:   Vec<Arc<str>>,
    /// Rule names this rule cross-refs (`${other.$VAR}`). Populated during
    /// `rule.parse` by scanning body invocations' `crossrefs`. Used by the
    /// Rule DAG (Kahn topo sort, cycle detection, runner scheduling).
    /// Walker-embedded cross-refs inside brace bodies (e.g. `json({ k: ${r.$V} })`)
    /// are NOT yet surfaced here — a follow-up op-level hook will add them.
    pub depends_on: Vec<Arc<str>>,
}

// ---------------------------------------------------------------------------
// OpCtx
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct OpCtx {
    pub run_id: RunId,
    pub op_id:  OpId,
    pub reader: Arc<dyn Reader>,
    pub writer: Arc<dyn Writer>,
    pub config: Arc<Config>,
    pub diags:  DiagSink,
    pub events: EventSink,
    /// Shared per-run row store. The Layer 2 adapter `expand_xrefs` reads
    /// it via `rows_of(rule)` to look up source rows for `${rule.$VAR}`
    /// tokens. Runner writes to it as cursors emit and calls
    /// `mark_complete(rule)` on stream end (Layer 5, not yet wired).
    /// All `OpCtx` clones across one run share the same `Arc<ResultStore>`.
    /// Sites with no cross-refs in scope (analysis snapshots, unit tests,
    /// CLI smoke) get a fresh empty one via `OpCtx::fresh_xref_state`.
    pub result_store: Arc<ResultStore>,
    /// Per-run dedup set so cross-ref diagnostics fire at most once per
    /// site instead of once per cursor that flows through. Key encoding:
    ///   - `(op_id, byte_range.start)` for a per-site empty-join Hint
    ///   - `(op_id, usize::MAX)`       for a once-per-op cartesian-limit Warn
    /// Shared with the same lifetime as `result_store`.
    pub xref_seen: Arc<Mutex<HashSet<(OpId, usize)>>>,
}

impl OpCtx {
    /// Allocate fresh empty result_store + xref_seen. For test/analysis
    /// sites that don't need cross-rule plumbing.
    pub fn fresh_xref_state() -> (Arc<ResultStore>, Arc<Mutex<HashSet<(OpId, usize)>>>) {
        (Arc::new(ResultStore::new()), Arc::new(Mutex::new(HashSet::new())))
    }
}

#[derive(Clone)]
pub struct DiagSink(pub Arc<dyn Fn(Box<dyn Diagnostic>) + Send + Sync>);

#[derive(Clone)]
pub struct EventSink(pub Arc<dyn Fn(RunEvent) + Send + Sync>);

// ---------------------------------------------------------------------------
// Op + Operator
// ---------------------------------------------------------------------------

pub trait Op: Send + Sync {
    fn pipe(
        &self,
        input: BoxStream<'static, Cursor>,
        ctx:   OpCtx,
    ) -> BoxStream<'static, Cursor>;

    fn name(&self) -> &'static str;
    fn step(&self) -> u16;
    fn parse_site(&self) -> &Arc<ParseSite>;
    fn tokens(&self) -> &'static [TokenSpan] { &[] }
    fn hover_at(&self, _byte: usize) -> Option<HoverInfo> { None }

    /// What concrete value satisfied this op for the given cursor.
    /// Framework calls on every cursor the op emits and pushes the result
    /// into `cursor.evidence` for LSP telemetry. Default `None` = opt out.
    fn witness(&self, _c: &Cursor) -> Option<Arc<str>> { None }

    /// If this op is binding a capture, its name. Paired with `witness`
    /// in the evidence record. Default `None` = filter / non-binding mode.
    fn capture_name(&self) -> Option<Arc<str>> { None }

    // -------------------------------------------------------------------
    // LSP hover surface — pure, no I/O.
    // -------------------------------------------------------------------

    /// Markdown for hovering directly on the op name token.
    fn hover_self(&self) -> String { self.name().to_string() }

    /// Markdown for hovering a capture variable produced by this op.
    /// `cap` is the bare name (no `$`). `cursors` is the current result
    /// set the LSP has in scope (may be empty; implementors must tolerate).
    fn hover_capture(&self, _cap: &str, _cursors: &[Cursor]) -> Option<String> { None }

    /// Markdown for hovering a non-captured match token whose compile-time
    /// location is `site`. `cursors` is the current LSP result set.
    fn hover_match(&self, _site: &ParseSite, _cursors: &[Cursor]) -> Option<String> { None }

    /// If this op owns a sub-pipeline (e.g. RuleOp's brace body), return a
    /// reference to it. Used by tree-walk resolution in DocSession to reach
    /// inner ops without re-lowering.
    fn body_pipeline(&self) -> Option<&Pipeline> { None }
}

// ---------------------------------------------------------------------------
// CompletionItem
// ---------------------------------------------------------------------------

pub struct CompletionItem {
    pub label:  String,
    pub detail: String,
    pub doc:    String,
}

// ---------------------------------------------------------------------------
// ScanPointer — op-declared `$$sigil` provenance token
// ---------------------------------------------------------------------------

pub struct ScanPointer {
    pub sigil: &'static str,
    pub read:  fn(&Cursor) -> Option<Arc<str>>,
}

pub trait Operator: Send + Sync {
    fn name(&self) -> &'static str;
    fn aliases(&self) -> &[&'static str] { &[] }

    fn bracket_grammar(&self) -> Option<GrammarRef> { None }
    fn paren_grammar  (&self) -> GrammarRef;
    fn brace_mode     (&self) -> BraceMode { BraceMode::DefaultFork }

    fn pre_register(&self, _inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<(), Vec<Box<dyn Diagnostic>>> { Ok(()) }

    fn parse(&self, inv: &OpInvocation, pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>;

    /// `$$sigil` tokens this op owns. Registry folds them into a dispatch
    /// map at `register()` time. Defaults to empty; ops opt in.
    fn scan_pointers(&self) -> &'static [ScanPointer] { &[] }

    /// One-line completion entry for this operator.
    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  self.name().to_string(),
            detail: self.paren_grammar().0.to_string(),
            doc:    self.name().to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// LSP-adjacent metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct TokenSpan { pub start: u32, pub end: u32, pub kind: TokenKind }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword, Capture, Provenance, CrossRef,
    PatternKey, PatternString, Punctuation,
}

#[derive(Debug, Clone)]
pub struct HoverInfo {
    pub title:    Arc<str>,
    pub markdown: Arc<str>,
    pub range:    Range<u32>,
}

// ---------------------------------------------------------------------------
// Hover grouping utility
// ---------------------------------------------------------------------------

/// Group `(fs_path, rev, value)` tuples into grouped markdown under `header`.
///
/// Rules:
/// - If all `fs_path` are `None`, returns a flat bullet list (no `###` headings).
/// - Otherwise groups by `(fs_path, rev)` with `### \`path\`  (rev: rev)` headings.
///   If all tuples share one rev, the `(rev: …)` suffix is omitted.
/// - Deduplicates `(fs_path, rev, value)` tuples.
/// - Caps total values at 20 across all groups.
///
/// `header` is rendered verbatim as the first line if provided (non-empty).
/// Returns `None` if `entries` is empty.
///
/// # TODO: reference tail needs rule name threaded from DocSession
pub fn hover_render_grouped(
    header:  &str,
    entries: &[(Option<String>, String, String)],  // (fs, rev, value)
) -> Option<String> {
    if entries.is_empty() { return None; }

    let all_fs_none = entries.iter().all(|(fs, _, _)| fs.is_none());

    if all_fs_none {
        // Flat bullet list — original behavior.
        let mut seen: Vec<&str> = Vec::new();
        let mut lines: Vec<String> = Vec::new();
        for (_, _, val) in entries {
            let v = val.as_str();
            if seen.contains(&v) { continue; }
            seen.push(v);
            lines.push(format!("- `{}`", v));
            if lines.len() >= 20 { break; }
        }
        if lines.is_empty() { return None; }
        let body = lines.join("\n");
        return Some(if header.is_empty() {
            body
        } else {
            format!("{}\n\n{}", header, body)
        });
    }

    // Determine if all revs are the same → omit (rev: …) suffix.
    let single_rev = {
        let mut it = entries.iter().map(|(_, rev, _)| rev.as_str());
        let first = it.next().unwrap();
        if it.all(|r| r == first) { Some(first) } else { None }
    };

    // Build ordered group key list + value sets, respecting 20-value cap.
    let mut order: Vec<(Option<String>, String)> = Vec::new();  // (fs, rev)
    let mut groups: std::collections::HashMap<(Option<String>, String), Vec<String>> =
        std::collections::HashMap::new();
    let mut total = 0usize;

    'outer: for (fs, rev, val) in entries {
        let key = (fs.clone(), rev.clone());
        let bucket = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Vec::new()
        });
        let v = val.as_str();
        if bucket.iter().any(|s| s.as_str() == v) { continue; }
        bucket.push(val.clone());
        total += 1;
        if total >= 20 { break 'outer; }
    }

    let mut sections: Vec<String> = Vec::new();
    for key in &order {
        let vals = &groups[key];
        let (fs_opt, rev) = key;
        let heading = match fs_opt {
            Some(path) => {
                if single_rev.is_some() {
                    format!("### `{}`", path)
                } else {
                    format!("### `{}`  (rev: {})", path, rev)
                }
            }
            None => {
                // Mixed: some cursors have fs, some don't. Group under rev.
                if single_rev.is_some() {
                    format!("### (no file)")
                } else {
                    format!("### (no file, rev: {})", rev)
                }
            }
        };
        let bullets: Vec<String> = vals.iter().map(|v| format!("- `{}`", v)).collect();
        sections.push(format!("{}\n{}", heading, bullets.join("\n")));
    }

    let body = sections.join("\n\n");
    Some(if header.is_empty() {
        body
    } else {
        format!("{}\n\n{}", header, body)
    })
}

// ---------------------------------------------------------------------------
// expand_xrefs — combineLatest-style adapter (Layer 2)
// ---------------------------------------------------------------------------

/// Splice between input cursors and `op.pipe`. For every cursor, expand it
/// over the cartesian product of source rows from each xref's target rule.
///
/// - Empty `xrefs` → input passes through unchanged (zero alloc).
/// - Empty target rule (no rows or pending) → cursor drops, one
///   `xref/empty-join` Hint per op-site (deduped via `ctx.xref_seen`).
/// - Distinct seeded tuples beyond `limit` → cursor drops, one
///   `xref/cartesian-limit` Warn per op (sentinel key `(op_id, usize::MAX)`).
/// - Per-input dedup: tuples of `(var, value)` in xref declaration order
///   are hashed so duplicate target rows collapse.
pub(crate) fn expand_xrefs(
    input:   BoxStream<'static, Cursor>,
    xrefs:   Arc<[CrossRefOccurrence]>,
    op_site: Arc<ParseSite>,
    ctx:     OpCtx,
    limit:   usize,
) -> BoxStream<'static, Cursor> {
    use futures_util::stream::{self, StreamExt};

    if xrefs.is_empty() {
        return input;
    }

    input
        .flat_map(move |cursor| {
            let outs = expand_one(&cursor, &xrefs, &op_site, &ctx, limit);
            stream::iter(outs)
        })
        .boxed()
}

fn expand_one(
    cursor:  &Cursor,
    xrefs:   &Arc<[CrossRefOccurrence]>,
    op_site: &Arc<ParseSite>,
    ctx:     &OpCtx,
    limit:   usize,
) -> Vec<Cursor> {
    // Pull rows for every xref. Empty/missing → empty-join, drop.
    let mut row_lists: Vec<Vec<crate::_12_result_store::CaptureMap>> =
        Vec::with_capacity(xrefs.len());
    for xr in xrefs.iter() {
        let rows = ctx.result_store.rows_of(&xr.rule).unwrap_or_default();
        if rows.is_empty() {
            emit_empty_join(ctx, op_site, xr);
            return Vec::new();
        }
        row_lists.push(rows);
    }

    // Cartesian over row indices.
    let counts: Vec<usize> = row_lists.iter().map(|v| v.len()).collect();
    let total: usize = counts.iter().product();

    // Output dedup key: tuple of (var, value) in xref declaration order.
    let mut seen_tuples: HashSet<Vec<(Arc<str>, Arc<str>)>> = HashSet::new();
    let mut out: Vec<Cursor> = Vec::new();

    let mut idx = vec![0usize; xrefs.len()];
    'combos: for _ in 0..total {
        // Build seed pairs for this combination.
        let mut seeds: Vec<(Arc<str>, Capture)> = Vec::with_capacity(xrefs.len());
        let mut key:   Vec<(Arc<str>, Arc<str>)> = Vec::with_capacity(xrefs.len());
        let mut skip = false;
        for (i, xr) in xrefs.iter().enumerate() {
            let row = &row_lists[i][idx[i]];
            let Some(cap) = row.get(&xr.var) else { skip = true; break; };
            key.push((xr.var.clone(), cap.value.clone()));
            seeds.push((xr.var.clone(), cap.clone()));
        }

        if !skip {
            if !seen_tuples.contains(&key) {
                if seen_tuples.len() >= limit {
                    // Count how many distinct tuples we'd actually have produced
                    // by walking the rest. For diagnostic clarity, keep going to
                    // tally distinct, but stop emitting cursors.
                    let distinct = count_distinct_tuples(xrefs, &row_lists);
                    emit_cartesian_limit(ctx, op_site, xrefs, &counts, distinct, limit);
                    return Vec::new();
                }
                seen_tuples.insert(key);

                let mut c = cursor.clone();
                for (var, cap) in seeds {
                    c.captures.insert(var, cap);
                }
                out.push(c);
            }
        }
        let _ = skip;

        // Increment mixed-radix counter.
        let mut k = xrefs.len();
        while k > 0 {
            k -= 1;
            idx[k] += 1;
            if idx[k] < counts[k] { continue 'combos; }
            idx[k] = 0;
        }
        break;
    }

    out
}

fn count_distinct_tuples(
    xrefs:     &Arc<[CrossRefOccurrence]>,
    row_lists: &[Vec<crate::_12_result_store::CaptureMap>],
) -> usize {
    let counts: Vec<usize> = row_lists.iter().map(|v| v.len()).collect();
    let total: usize = counts.iter().product();
    let mut seen: HashSet<Vec<(Arc<str>, Arc<str>)>> = HashSet::new();
    let mut idx = vec![0usize; xrefs.len()];
    'combos: for _ in 0..total {
        let mut key: Vec<(Arc<str>, Arc<str>)> = Vec::with_capacity(xrefs.len());
        let mut ok = true;
        for (i, xr) in xrefs.iter().enumerate() {
            let row = &row_lists[i][idx[i]];
            match row.get(&xr.var) {
                Some(cap) => key.push((xr.var.clone(), cap.value.clone())),
                None => { ok = false; break; }
            }
        }
        if ok { seen.insert(key); }
        let mut k = xrefs.len();
        while k > 0 {
            k -= 1;
            idx[k] += 1;
            if idx[k] < counts[k] { continue 'combos; }
            idx[k] = 0;
        }
        break;
    }
    seen.len()
}

fn emit_empty_join(ctx: &OpCtx, _op_site: &Arc<ParseSite>, xr: &CrossRefOccurrence) {
    let key = (ctx.op_id, xr.byte_range.start);
    {
        let mut g = ctx.xref_seen.lock().unwrap();
        if g.contains(&key) { return; }
        g.insert(key);
    }
    let mut site = (**_op_site).clone();
    site.byte_range = xr.byte_range.clone();
    let diag = XrefEmptyJoin { site, target_rule: xr.rule.clone() };
    (ctx.diags.0)(Box::new(diag));
}

fn emit_cartesian_limit(
    ctx:        &OpCtx,
    op_site:    &Arc<ParseSite>,
    xrefs:      &Arc<[CrossRefOccurrence]>,
    counts:     &[usize],
    distinct:   usize,
    limit:      usize,
) {
    let key = (ctx.op_id, usize::MAX);
    {
        let mut g = ctx.xref_seen.lock().unwrap();
        if g.contains(&key) { return; }
        g.insert(key);
    }
    let rule_counts: Vec<(Arc<str>, usize)> = xrefs
        .iter()
        .zip(counts.iter())
        .map(|(xr, n)| (xr.rule.clone(), *n))
        .collect();
    let diag = XrefCartesianLimit {
        site: (**op_site).clone(),
        distinct, limit, rule_counts,
    };
    (ctx.diags.0)(Box::new(diag));
}

// ---------------------------------------------------------------------------
// Cross-ref runtime diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct XrefEmptyJoin {
    pub site:        ParseSite,
    pub target_rule: Arc<str>,
}

impl Diagnostic for XrefEmptyJoin {
    fn code(&self) -> &str { "xref/empty-join" }
    fn severity(&self) -> Severity { Severity::Hint }
    fn primary(&self) -> &ParseSite { &self.site }
    fn render(&self, out: &mut dyn Renderer) {
        out.header(self.code(), self.severity(),
            &format!("cross-ref to rule `{}` produced zero rows; cursor dropped",
                self.target_rule));
        out.primary(&self.site);
    }
}

#[derive(Debug)]
pub struct XrefCartesianLimit {
    pub site:        ParseSite,
    pub distinct:    usize,
    pub limit:       usize,
    pub rule_counts: Vec<(Arc<str>, usize)>,
}

impl Diagnostic for XrefCartesianLimit {
    fn code(&self) -> &str { "xref/cartesian-limit" }
    fn severity(&self) -> Severity { Severity::Warn }
    fn primary(&self) -> &ParseSite { &self.site }
    fn render(&self, out: &mut dyn Renderer) {
        let counts: Vec<String> = self.rule_counts.iter()
            .map(|(r, n)| format!("{r}={n}")).collect();
        out.header(self.code(), self.severity(),
            &format!("cross-ref expansion exceeded cartesian limit (distinct={} limit={}; rules: {})",
                self.distinct, self.limit, counts.join(", ")));
        out.primary(&self.site);
    }
}

// ---------------------------------------------------------------------------
// expand_xrefs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod xref_tests {
    use super::*;
    use crate::_0_types::{Capture, Cursor, FileId, OpId, ParseSite, RunId, SprfPath};
    use crate::_1_diagnostic::Diagnostic;
    use crate::_12_result_store::{CaptureMap, ResultStore};
    use crate::{Config, RuntimeConfig};
    use crate::readers::MemReader;
    use crate::writers::MemWriter;
    use futures_util::stream::{self, StreamExt};
    use std::path::Path;
    use std::sync::Mutex;

    fn site() -> Arc<ParseSite> {
        Arc::new(ParseSite {
            file:       Arc::from(Path::new("test.sprf")),
            path:       Arc::from(Vec::<crate::_0_types::ParseSeg>::new().into_boxed_slice()),
            byte_range: 100..110,
        })
    }

    fn make_config() -> Arc<Config> {
        Arc::new(Config {
            repos: vec![], revs: vec![], fs_exclude: vec![],
            sprf_files: vec![], shell_allow: vec![],
            runtime: RuntimeConfig {
                worker_threads: 1, buffer_size: 64,
                flush_interval_ms: 100, collect_witnesses: false,
                xref_cartesian_limit: 10_000,
            },
            content_hash: 0,
        })
    }

    fn cap(v: &str) -> Capture { Capture::new(Arc::from(v)) }

    fn row(pairs: &[(&str, &str)]) -> CaptureMap {
        pairs.iter().map(|(k, v)| (Arc::<str>::from(*k), cap(v))).collect()
    }

    fn cursor() -> Cursor {
        Cursor {
            run_id:   RunId(0),
            repo:     Arc::from("repo"),
            rev:      Arc::from("HEAD"),
            fs:       None,
            captures: std::collections::HashMap::new(),
            fks:      std::collections::HashMap::new(),
            path:     SprfPath(Arc::from(Vec::<crate::_0_types::PathSeg>::new().into_boxed_slice())),
            evidence: Vec::new(),
            content:  None,
        }
    }

    fn cursor_with(cap_pairs: &[(&str, &str)]) -> Cursor {
        let mut c = cursor();
        for (k, v) in cap_pairs {
            c.captures.insert(Arc::<str>::from(*k), cap(v));
        }
        c
    }

    struct TestCtx {
        ctx:   OpCtx,
        store: Arc<ResultStore>,
        diags: Arc<Mutex<Vec<(String, String)>>>,  // (code, _rendered placeholder unused here)
    }

    fn build_ctx() -> TestCtx {
        let config = make_config();
        let reader: Arc<dyn crate::_3_reader::Reader + Send + Sync> =
            Arc::new(MemReader::new(config.clone()));
        let writer: Arc<dyn crate::_4_writer::Writer + Send + Sync> =
            Arc::new(MemWriter::new());
        let fired: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(vec![]));
        let fired2 = fired.clone();
        let diags = DiagSink(Arc::new(move |d: Box<dyn Diagnostic>| {
            fired2.lock().unwrap().push((d.code().to_string(), String::new()));
        }));
        let events = EventSink(Arc::new(|_| {}));
        let store = Arc::new(ResultStore::new());
        let xref_seen = Arc::new(Mutex::new(std::collections::HashSet::new()));
        let ctx = OpCtx {
            run_id: RunId(0),
            op_id:  OpId(42),
            reader, writer, config, diags, events,
            result_store: store.clone(),
            xref_seen,
        };
        TestCtx { ctx, store, diags: fired }
    }

    fn xref(rule: &str, var: &str, start: usize) -> CrossRefOccurrence {
        CrossRefOccurrence {
            rule: Arc::from(rule),
            var:  Arc::from(var),
            byte_range: start..(start + 4),
        }
    }

    fn drain(s: BoxStream<'static, Cursor>) -> Vec<Cursor> {
        futures::executor::block_on(s.collect())
    }

    fn run(xrefs: Vec<CrossRefOccurrence>, inputs: Vec<Cursor>, tctx: &TestCtx, limit: usize)
        -> Vec<Cursor>
    {
        let s = stream::iter(inputs).boxed();
        let arc: Arc<[CrossRefOccurrence]> = Arc::from(xrefs.into_boxed_slice());
        drain(expand_xrefs(s, arc, site(), tctx.ctx.clone(), limit))
    }

    #[test]
    fn no_xrefs_passthrough() {
        let t = build_ctx();
        let out = run(vec![], vec![cursor(), cursor()], &t, 10_000);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn single_xref_single_row() {
        let t = build_ctx();
        let rule: Arc<str> = Arc::from("R");
        t.store.append(&rule, row(&[("TAG", "v1")]));
        t.store.mark_complete(&rule);

        let out = run(vec![xref("R", "TAG", 0)], vec![cursor()], &t, 10_000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].captures.get(&Arc::<str>::from("TAG")).unwrap().value.as_ref(), "v1");
    }

    #[test]
    fn single_xref_multiple_rows() {
        let t = build_ctx();
        let rule: Arc<str> = Arc::from("R");
        for v in &["v1", "v2", "v3"] {
            t.store.append(&rule, row(&[("TAG", v)]));
        }
        t.store.mark_complete(&rule);

        let out = run(vec![xref("R", "TAG", 0)], vec![cursor()], &t, 10_000);
        let mut vals: Vec<String> = out.iter()
            .map(|c| c.captures[&Arc::<str>::from("TAG")].value.to_string())
            .collect();
        vals.sort();
        assert_eq!(vals, vec!["v1", "v2", "v3"]);
    }

    #[test]
    fn single_xref_duplicate_values_deduped() {
        let t = build_ctx();
        let rule: Arc<str> = Arc::from("R");
        for _ in 0..5 { t.store.append(&rule, row(&[("TAG", "v1")])); }
        t.store.mark_complete(&rule);

        let out = run(vec![xref("R", "TAG", 0)], vec![cursor()], &t, 10_000);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn two_xrefs_cartesian() {
        let t = build_ctx();
        let a: Arc<str> = Arc::from("A");
        let b: Arc<str> = Arc::from("B");
        for v in &["a1", "a2"] { t.store.append(&a, row(&[("X", v)])); }
        for v in &["b1", "b2", "b3"] { t.store.append(&b, row(&[("Y", v)])); }
        t.store.mark_complete(&a);
        t.store.mark_complete(&b);

        let out = run(
            vec![xref("A", "X", 0), xref("B", "Y", 10)],
            vec![cursor()],
            &t, 10_000,
        );
        assert_eq!(out.len(), 6);
    }

    #[test]
    fn two_xrefs_partial_dedup() {
        let t = build_ctx();
        let a: Arc<str> = Arc::from("A");
        let b: Arc<str> = Arc::from("B");
        for v in &["a1", "a2"] { t.store.append(&a, row(&[("X", v)])); }
        for v in &["b1", "b1", "b2"] { t.store.append(&b, row(&[("Y", v)])); }
        t.store.mark_complete(&a);
        t.store.mark_complete(&b);

        let out = run(
            vec![xref("A", "X", 0), xref("B", "Y", 10)],
            vec![cursor()],
            &t, 10_000,
        );
        // distinct tuples: 2 * 2 = 4
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn empty_join_drops_and_diags() {
        let t = build_ctx();
        let rule: Arc<str> = Arc::from("R");
        t.store.mark_complete(&rule);  // no rows

        let out = run(vec![xref("R", "TAG", 0)], vec![cursor()], &t, 10_000);
        assert!(out.is_empty());
        let d = t.diags.lock().unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, "xref/empty-join");
    }

    #[test]
    fn empty_join_dedup_across_cursors() {
        let t = build_ctx();
        let rule: Arc<str> = Arc::from("R");
        t.store.mark_complete(&rule);

        let out = run(
            vec![xref("R", "TAG", 0)],
            vec![cursor(), cursor(), cursor()],
            &t, 10_000,
        );
        assert!(out.is_empty());
        assert_eq!(t.diags.lock().unwrap().len(), 1);
    }

    #[test]
    fn cartesian_limit_exceeded() {
        let t = build_ctx();
        let a: Arc<str> = Arc::from("A");
        let b: Arc<str> = Arc::from("B");
        for i in 0..100 { t.store.append(&a, row(&[("X", &format!("a{}", i))])); }
        for i in 0..200 { t.store.append(&b, row(&[("Y", &format!("b{}", i))])); }
        t.store.mark_complete(&a);
        t.store.mark_complete(&b);

        let out = run(
            vec![xref("A", "X", 0), xref("B", "Y", 10)],
            vec![cursor()],
            &t, 10_000,
        );
        assert!(out.is_empty());
        let d = t.diags.lock().unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, "xref/cartesian-limit");
    }

    #[test]
    fn cartesian_limit_diag_once_per_op() {
        let t = build_ctx();
        let a: Arc<str> = Arc::from("A");
        let b: Arc<str> = Arc::from("B");
        for i in 0..100 { t.store.append(&a, row(&[("X", &format!("a{}", i))])); }
        for i in 0..200 { t.store.append(&b, row(&[("Y", &format!("b{}", i))])); }
        t.store.mark_complete(&a);
        t.store.mark_complete(&b);

        let out = run(
            vec![xref("A", "X", 0), xref("B", "Y", 10)],
            vec![cursor(), cursor()],
            &t, 10_000,
        );
        assert!(out.is_empty());
        assert_eq!(t.diags.lock().unwrap().len(), 1);
    }

    #[test]
    fn unknown_target_rule_treated_as_empty_join() {
        let t = build_ctx();
        let out = run(vec![xref("GhostRule", "TAG", 0)], vec![cursor()], &t, 10_000);
        assert!(out.is_empty());
        let d = t.diags.lock().unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, "xref/empty-join");
    }

    #[test]
    fn seed_overwrites_existing_capture_with_same_key() {
        let t = build_ctx();
        let rule: Arc<str> = Arc::from("R");
        t.store.append(&rule, row(&[("TAG", "new")]));
        t.store.mark_complete(&rule);

        let out = run(
            vec![xref("R", "TAG", 0)],
            vec![cursor_with(&[("TAG", "old")])],
            &t, 10_000,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].captures[&Arc::<str>::from("TAG")].value.as_ref(), "new");
    }

    #[test]
    fn non_seeded_captures_preserved() {
        let t = build_ctx();
        let rule: Arc<str> = Arc::from("R");
        t.store.append(&rule, row(&[("TAG", "v1")]));
        t.store.mark_complete(&rule);

        let out = run(
            vec![xref("R", "TAG", 0)],
            vec![cursor_with(&[("OTHER", "x")])],
            &t, 10_000,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].captures[&Arc::<str>::from("OTHER")].value.as_ref(), "x");
        assert_eq!(out[0].captures[&Arc::<str>::from("TAG")].value.as_ref(), "v1");
    }

    // Suppress unused-import warnings for FileId.
    #[allow(dead_code)]
    fn _silence_unused(_: FileId) {}

    // Task 1: default scan_pointers() is empty for any Operator that has not
    // opted in. Updated in Task 3 when RepoFactory registers repo/repo_norm.
    #[test]
    fn default_scan_pointers_is_empty() {
        // RuleFactory declares none; verifies the trait default stays empty.
        let factory = crate::ops::RuleFactory;
        assert!(Operator::scan_pointers(&factory).is_empty());
    }
}
