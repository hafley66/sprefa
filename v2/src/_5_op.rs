//! Op + Operator traits, OpCtx, OpInvocation, Pipeline.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use futures_core::stream::BoxStream;

use crate::_0_types::{Capture, Cursor, OpEvidence, OpId, ParseSite, PathSeg, RunEvent, RunId, SprfPath};
use crate::_1_diagnostic::Diagnostic;
use crate::_2_config::Config;
use crate::_3_reader::Reader;
use crate::_4_writer::Writer;

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
    Op     (Arc<dyn Op>),
    Seq    (Vec<Pipeline>),
    Fork   (Vec<ForkBranch>),
    Switch { on: ChannelSelector, arms: Vec<(Arc<str>, Pipeline)> },
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
            Pipeline::Op(op) => {
                let name = Arc::<str>::from(op.name());
                let ps   = op.parse_site().clone();
                let collect = ctx.config.runtime.collect_witnesses;
                let op2 = op.clone();
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
