//! line — scan content line-by-line, match pattern against each line.
//!
//! Examples:
//!   line(re:TODO(.*))              regex with named groups
//!   line($ORG/$REPO)               segment capture
//!   json({code:$C}) > &.$C > line(re:TODO)   re-parse sub-content

use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream::{self, StreamExt};
use tracing::{info_span, Instrument};

use crate::_0_types::{Capture, Cursor, FilePath, OpEvidence, ParseSite, Severity};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation,
    Operator, Pipeline, ProgramCtx,
};
use crate::_16_pattern::{CompiledPattern, PatternMatcher, Segment};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct LineFactory;

impl Operator for LineFactory {
    fn name(&self) -> &'static str { "line" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("line-arg")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "line".to_string(),
            detail: "line(pattern | re:REGEX)".to_string(),
            doc:    "# line\n\nScan content line-by-line. Emit one cursor per matching line with captures bound.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(LineDiag::MissingArg { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>]
        })?;
        let arg = paren.src.trim();
        if arg.is_empty() {
            return Err(vec![Box::new(LineDiag::MissingArg {
                site: (*inv.parse_site).clone(),
            }) as Box<dyn Diagnostic>]);
        }
        let pattern = CompiledPattern::compile(arg).map_err(|e| {
            vec![Box::new(LineDiag::BadArg {
                site: (*inv.parse_site).clone(),
                got:  Arc::from(arg),
                msg:  Arc::from(e.to_string().as_str()),
            }) as Box<dyn Diagnostic>]
        })?;
        Ok(Pipeline::Op(Arc::new(LineOp {
            pattern:    Arc::new(pattern),
            parse_site: inv.parse_site.clone(),
        }).into()))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct LineOp {
    pattern:    Arc<CompiledPattern>,
    parse_site: Arc<ParseSite>,
}

impl Op for LineOp {
    fn name(&self) -> &'static str { "line" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn binds_captures(&self) -> Vec<Arc<str>> {
        let mut out: Vec<Arc<str>> = Vec::new();
        let mut push = |n: &str| {
            if !out.iter().any(|s: &Arc<str>| s.as_ref() == n) {
                out.push(Arc::from(n));
            }
        };
        for m in &self.pattern.matchers {
            match m {
                PatternMatcher::Regex(r) => {
                    for name in r.capture_names().flatten() {
                        push(name);
                    }
                }
                PatternMatcher::SegmentCapture(segs) => {
                    for seg in segs {
                        match seg {
                            Segment::Capture(n) | Segment::MultiCapture(n) => push(n),
                            _ => {}
                        }
                    }
                }
                PatternMatcher::Glob(_) => {}
            }
        }
        out
    }

    fn hover_capture(&self, cap: &str, cursors: &[Cursor]) -> Option<String> {
        if !self.binds_captures().iter().any(|n| n.as_ref() == cap) { return None; }
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .filter_map(|c| c.captures.get(cap).map(|cv| (
                c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                c.rev.to_string(),
                cv.value.to_string(),
            )))
            .collect();
        let header = format!("`${}` (from line)", cap);
        hover_render_grouped(&header, &entries)
    }

    fn pipe(&self, input: BoxStream<'static, Arc<[Cursor]>>, ctx: OpCtx)
        -> BoxStream<'static, Arc<[Cursor]>>
    {
        let _pipe_span = info_span!("op.line.pipe").entered();
        let pattern = self.pattern.clone();
        let site    = self.parse_site.clone();
        let reader  = ctx.reader.clone();
        let diags   = ctx.diags.clone();

        input.flat_map(move |batch| {
            let batch_span = info_span!("op.line.batch", n = batch.len());
            let pattern = pattern.clone();
            let site    = site.clone();
            let reader  = reader.clone();
            let diags   = diags.clone();

            let per_batch = async move {
                let mut out: Vec<Arc<[Cursor]>> = Vec::new();
                for c in batch.iter() {
                    // PATH B — content priority
                    let (content_arc, base_offset, fp) = if let Some(content) = c.content.as_ref() {
                        let base = c.byte_range.as_ref().map(|r| r.start).unwrap_or(0);
                        let fp = c.fs.clone().unwrap_or_else(|| {
                            FilePath(Arc::from(std::path::Path::new("<rebased>")))
                        });
                        (content.clone(), base, fp)
                    } else if let Some(fp) = c.fs.as_ref() {
                        // PATH C — reader fallback
                        let mut s = reader.bytes(&c.repo, &c.rev, fp);
                        let raw = match s.next().await {
                            Some(b) => b,
                            None    => {
                                diags.0(Box::new(LineDiag::ReadFailed {
                                    site: (*site).clone(),
                                    path: fp.0.to_string_lossy().into_owned(),
                                }));
                                continue;
                            }
                        };
                        (Arc::new(raw), 0usize, fp.clone())
                    } else {
                        // no content and no fs: line doesn't enumerate files
                        continue;
                    };

                    let sliced: &[u8] = match &c.byte_range {
                        Some(r) if c.content.is_some() => &content_arc[r.clone()],
                        _                              => &content_arc[..],
                    };

                    let mut per_cursor: Vec<Cursor> = Vec::new();

                    let mut start = 0usize;
                    let process_line = |line_bytes: &[u8], line_start: usize, line_end: usize,
                                        pattern: &CompiledPattern, c: &Cursor,
                                        content_arc: &Arc<Bytes>, base_offset: usize,
                                        fp: &FilePath, site: &Arc<ParseSite>,
                                        per_cursor: &mut Vec<Cursor>|
                    {
                        let s = match std::str::from_utf8(line_bytes) {
                            Ok(s) => s,
                            Err(_) => return,
                        };
                        for m in &pattern.matchers {
                            if let Some(caps_map) = m.captures(s) {
                                let mut c2 = c.clone();
                                c2.content   = Some(content_arc.clone());
                                c2.fs        = Some(fp.clone());
                                c2.byte_range = Some(
                                    (base_offset + line_start)..(base_offset + line_end)
                                );
                                for (name, value_str) in caps_map {
                                    c2.captures.insert(
                                        Arc::from(name.as_str()),
                                        Capture::new(Arc::from(value_str.as_str())),
                                    );
                                }
                                c2.evidence.push(OpEvidence {
                                    op_name:    "line",
                                    parse_site: site.clone(),
                                    matched:    Arc::from(s),
                                    capture:    None,
                                });
                                per_cursor.push(c2);
                                return; // first matcher wins
                            } else if m.is_match(s) {
                                // glob match: no captures but still emit
                                let mut c2 = c.clone();
                                c2.content    = Some(content_arc.clone());
                                c2.fs         = Some(fp.clone());
                                c2.byte_range = Some(
                                    (base_offset + line_start)..(base_offset + line_end)
                                );
                                c2.evidence.push(OpEvidence {
                                    op_name:    "line",
                                    parse_site: site.clone(),
                                    matched:    Arc::from(s),
                                    capture:    None,
                                });
                                per_cursor.push(c2);
                                return;
                            }
                        }
                    };

                    for (i, b) in sliced.iter().enumerate() {
                        if *b == b'\n' {
                            process_line(
                                &sliced[start..i], start, i,
                                &pattern, c, &content_arc, base_offset, &fp, &site,
                                &mut per_cursor,
                            );
                            start = i + 1;
                        }
                    }
                    if start < sliced.len() {
                        let end = sliced.len();
                        process_line(
                            &sliced[start..end], start, end,
                            &pattern, c, &content_arc, base_offset, &fp, &site,
                            &mut per_cursor,
                        );
                    }

                    if per_cursor.is_empty() {
                        diags.0(Box::new(LineDiag::NoMatch {
                            site: (*site).clone(),
                            path: fp.0.to_string_lossy().into_owned(),
                            pattern: pattern.src.as_ref().to_string(),
                        }));
                    } else {
                        out.push(Arc::from(per_cursor.into_boxed_slice()));
                    }
                }
                out
            };
            stream::once(per_batch.instrument(batch_span)).flat_map(|bs| stream::iter(bs))
        }).boxed()
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum LineDiag {
    MissingArg { site: ParseSite },
    BadArg     { site: ParseSite, got: Arc<str>, msg: Arc<str> },
    ReadFailed { site: ParseSite, path: String },
    NoMatch    { site: ParseSite, path: String, pattern: String },
}

impl Diagnostic for LineDiag {
    fn code(&self) -> &str {
        match self {
            LineDiag::MissingArg { .. } => "line/missing-arg",
            LineDiag::BadArg     { .. } => "line/bad-arg",
            LineDiag::ReadFailed { .. } => "line/read-failed",
            LineDiag::NoMatch    { .. } => "line/no-match",
        }
    }
    fn severity(&self) -> Severity { Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            LineDiag::MissingArg { site }    => site,
            LineDiag::BadArg     { site, .. } => site,
            LineDiag::ReadFailed { site, .. } => site,
            LineDiag::NoMatch    { site, .. } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            LineDiag::MissingArg { site } => {
                out.header(self.code(), self.severity(),
                    "line requires a pattern argument (e.g. line(re:TODO) or line($USER:$PWD))");
                out.primary(site);
            }
            LineDiag::BadArg { site, got, msg } => {
                out.header(self.code(), self.severity(),
                    &format!("line pattern `{got}` is invalid: {msg}"));
                out.primary(site);
            }
            LineDiag::ReadFailed { site, path } => {
                out.header(self.code(), self.severity(),
                    &format!("line failed to read `{path}`"));
                out.primary(site);
            }
            LineDiag::NoMatch { site, path, pattern } => {
                out.header(self.code(), self.severity(),
                    &format!("line(`{pattern}`) matched nothing in `{path}`"));
                out.primary(site);
            }
        }
    }
}
