//! marker — narrow cursor.byte_range to a comment-bounded region.
//!
//! Surface syntax:
//!   marker("OPEN")                 sequential: each open starts a region, ends
//!                                  at next open or EOF.
//!   marker("OPEN", "CLOSE")        paired: open/close nestable, unpaired open
//!                                  collapses to the comment line itself.
//!   marker("OPEN", "CLOSE", $NAME) paired, binds the trimmed label text after
//!                                  the open-regex match to `$NAME`.
//!
//! OPEN / CLOSE are regexes matched against comment-line text. v2 port uses
//! line-prefix comment detection only (`//`, `#`, `--`, `/*`, `*`, `<!--`).

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream::{self, StreamExt};
use regex::Regex;
use tracing::{info_span, Instrument};

use crate::_0_types::{Capture, Cursor, FilePath, OpEvidence, ParseSite, Severity};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation,
    Operator, Pipeline, ProgramCtx,
};

// ---------------------------------------------------------------------------
// Local scope (mirrors v1 MarkerScope from crates/rules/src/types.rs:154)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct MarkerScope {
    open:    Arc<str>,
    close:   Option<Arc<str>>,
    capture: Option<Arc<str>>,
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct MarkerFactory;

impl Operator for MarkerFactory {
    fn name(&self) -> &'static str { "marker" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("marker-arg")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "marker".to_string(),
            detail: "marker(\"OPEN\" [, \"CLOSE\" [, $NAME]])".to_string(),
            doc:    "# marker\n\nNarrow cursor.byte_range to regions bounded by comment markers. One regex = sequential regions (next marker or EOF ends the region). Two regexes = paired open/close (nestable). A trailing `$NAME` binds the trimmed label text after the open match.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(MarkerDiag::MissingArg { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>]
        })?;
        let body = paren.src.trim();
        if body.is_empty() {
            return Err(vec![Box::new(MarkerDiag::MissingArg {
                site: (*inv.parse_site).clone(),
            }) as Box<dyn Diagnostic>]);
        }
        let args = split_args(body).map_err(|e| {
            vec![Box::new(MarkerDiag::BadArg {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from(e.as_str()),
            }) as Box<dyn Diagnostic>]
        })?;

        let mut regexes: Vec<String> = Vec::new();
        let mut capture: Option<String> = None;
        for a in &args {
            match a {
                MarkerArg::Regex(r) => {
                    if capture.is_some() {
                        return Err(vec![Box::new(MarkerDiag::BadArg {
                            site: (*inv.parse_site).clone(),
                            msg:  Arc::from("regex args must precede the $NAME capture"),
                        }) as Box<dyn Diagnostic>]);
                    }
                    regexes.push(r.clone());
                }
                MarkerArg::Capture(name) => {
                    if capture.is_some() {
                        return Err(vec![Box::new(MarkerDiag::BadArg {
                            site: (*inv.parse_site).clone(),
                            msg:  Arc::from("marker() accepts at most one $NAME capture"),
                        }) as Box<dyn Diagnostic>]);
                    }
                    capture = Some(name.clone());
                }
            }
        }
        let (open, close) = match regexes.len() {
            1 => (regexes[0].clone(), None),
            2 => (regexes[0].clone(), Some(regexes[1].clone())),
            n => return Err(vec![Box::new(MarkerDiag::BadArg {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from(format!("marker() takes 1 or 2 regex args, got {n}").as_str()),
            }) as Box<dyn Diagnostic>]),
        };

        if let Err(e) = Regex::new(&open) {
            return Err(vec![Box::new(MarkerDiag::BadArg {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from(format!("open regex invalid: {e}").as_str()),
            }) as Box<dyn Diagnostic>]);
        }
        if let Some(c) = close.as_ref() {
            if let Err(e) = Regex::new(c) {
                return Err(vec![Box::new(MarkerDiag::BadArg {
                    site: (*inv.parse_site).clone(),
                    msg:  Arc::from(format!("close regex invalid: {e}").as_str()),
                }) as Box<dyn Diagnostic>]);
            }
        }

        let scope = MarkerScope {
            open:    Arc::from(open.as_str()),
            close:   close.map(|s| Arc::from(s.as_str())),
            capture: capture.map(|s| Arc::from(s.as_str())),
        };
        Ok(Pipeline::Op(Arc::new(MarkerOp {
            scope:      Arc::new(scope),
            parse_site: inv.parse_site.clone(),
        }).into()))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct MarkerOp {
    scope:      Arc<MarkerScope>,
    parse_site: Arc<ParseSite>,
}

impl Op for MarkerOp {
    fn name(&self) -> &'static str { "marker" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn binds_captures(&self) -> Vec<Arc<str>> {
        match self.scope.capture.as_ref() {
            Some(name) => vec![name.clone()],
            None       => Vec::new(),
        }
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
        let header = format!("`${}` (from marker)", cap);
        hover_render_grouped(&header, &entries)
    }

    fn pipe(&self, input: BoxStream<'static, Arc<[Cursor]>>, ctx: OpCtx)
        -> BoxStream<'static, Arc<[Cursor]>>
    {
        let _pipe_span = info_span!("op.marker.pipe").entered();
        let scope  = self.scope.clone();
        let site   = self.parse_site.clone();
        let reader = ctx.reader.clone();
        let diags  = ctx.diags.clone();

        input.flat_map(move |batch| {
            let batch_span = info_span!("op.marker.batch", n = batch.len());
            let scope  = scope.clone();
            let site   = site.clone();
            let reader = reader.clone();
            let diags  = diags.clone();

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
                                diags.0(Box::new(MarkerDiag::ReadFailed {
                                    site: (*site).clone(),
                                    path: fp.0.to_string_lossy().into_owned(),
                                }));
                                continue;
                            }
                        };
                        (Arc::new(raw), 0usize, fp.clone())
                    } else {
                        continue;
                    };

                    let sliced: &[u8] = match &c.byte_range {
                        Some(r) if c.content.is_some() => &content_arc[r.clone()],
                        _                              => &content_arc[..],
                    };

                    let per_cursor = run_scope(
                        sliced, &scope, c, &content_arc, base_offset, &fp, &site,
                    );

                    if per_cursor.is_empty() {
                        diags.0(Box::new(MarkerDiag::NoMatch {
                            site:    (*site).clone(),
                            path:    fp.0.to_string_lossy().into_owned(),
                            pattern: scope.open.to_string(),
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
// Runtime: scan comments, pair into regions, stamp cursors
// ---------------------------------------------------------------------------

struct CommentNode {
    text:       String,
    byte_start: usize,
    byte_end:   usize,
}

struct Region {
    start:    usize,
    end:      usize,
    evidence: String,   // the opening comment line text
    caps:     HashMap<Arc<str>, Arc<str>>,
}

fn run_scope(
    sliced:      &[u8],
    scope:       &MarkerScope,
    c:           &Cursor,
    content_arc: &Arc<Bytes>,
    base_offset: usize,
    fp:          &FilePath,
    site:        &Arc<ParseSite>,
) -> Vec<Cursor> {
    let src = match std::str::from_utf8(sliced) { Ok(s) => s, Err(_) => return vec![] };
    let open_re = match Regex::new(&scope.open) { Ok(r) => r, Err(_) => return vec![] };
    let close_re = scope.close.as_ref().and_then(|p| Regex::new(p).ok());

    let comments = extract_comments_line_prefix(src);
    if comments.is_empty() { return vec![]; }

    let regions = match &close_re {
        Some(close) => paired_regions(&comments, &open_re, close, scope.capture.as_deref(), sliced.len()),
        None        => sequential_regions(&comments, &open_re, scope.capture.as_deref(), sliced.len()),
    };

    regions.into_iter().map(|r| {
        let mut c2 = c.clone();
        c2.fs         = Some(fp.clone());
        c2.content    = Some(content_arc.clone());
        c2.byte_range = Some((base_offset + r.start)..(base_offset + r.end));
        for (name, val) in r.caps {
            c2.captures.insert(name, Capture::new(val));
        }
        c2.evidence.push(OpEvidence {
            op_name:    "marker",
            parse_site: site.clone(),
            matched:    Arc::from(r.evidence.as_str()),
            capture:    None,
        });
        c2
    }).collect()
}

fn extract_comments_line_prefix(src: &str) -> Vec<CommentNode> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in src.split('\n') {
        let trimmed = line.trim_start();
        let is_comment = trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("--")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("<!--");
        if is_comment {
            out.push(CommentNode {
                text:       line.to_string(),
                byte_start: offset,
                byte_end:   offset + line.len(),
            });
        }
        offset += line.len() + 1; // +1 for '\n' separator (last line: overshoots harmlessly)
    }
    out
}

fn paired_regions(
    comments:     &[CommentNode],
    open_re:      &Regex,
    close_re:     &Regex,
    capture_name: Option<&str>,
    _src_len:     usize,
) -> Vec<Region> {
    let mut regions: Vec<Region> = Vec::new();
    // Stack: (open_byte_start, opening_comment_line_text, captures)
    let mut stack: Vec<(usize, String, HashMap<Arc<str>, Arc<str>>)> = Vec::new();

    for comment in comments {
        if let Some(m) = open_re.find(&comment.text) {
            let caps = extract_label_capture(&comment.text, m.end(), capture_name);
            stack.push((comment.byte_start, comment.text.clone(), caps));
        } else if close_re.is_match(&comment.text) {
            if let Some((start, evidence, caps)) = stack.pop() {
                regions.push(Region { start, end: comment.byte_end, evidence, caps });
            }
        }
    }

    // Unpaired opens: collapse to the comment line itself.
    for (start, evidence, caps) in stack {
        let point_end = comments.iter()
            .find(|cc| cc.byte_start == start)
            .map(|cc| cc.byte_end)
            .unwrap_or(start);
        regions.push(Region { start, end: point_end, evidence, caps });
    }

    regions.sort_by_key(|r| r.start);
    regions
}

fn sequential_regions(
    comments:     &[CommentNode],
    open_re:      &Regex,
    capture_name: Option<&str>,
    src_len:      usize,
) -> Vec<Region> {
    let mut opens: Vec<(usize, String, HashMap<Arc<str>, Arc<str>>)> = Vec::new();
    for comment in comments {
        if let Some(m) = open_re.find(&comment.text) {
            let caps = extract_label_capture(&comment.text, m.end(), capture_name);
            opens.push((comment.byte_start, comment.text.clone(), caps));
        }
    }
    if opens.is_empty() { return Vec::new(); }

    let mut regions: Vec<Region> = Vec::with_capacity(opens.len());
    for i in 0..opens.len() {
        let (start, ref evidence, ref caps) = opens[i];
        let end = if i + 1 < opens.len() { opens[i + 1].0 } else { src_len };
        regions.push(Region {
            start,
            end,
            evidence: evidence.clone(),
            caps:     caps.clone(),
        });
    }
    regions
}

fn extract_label_capture(
    comment_text: &str,
    match_end:    usize,
    capture_name: Option<&str>,
) -> HashMap<Arc<str>, Arc<str>> {
    let mut caps = HashMap::new();
    if let Some(name) = capture_name {
        let label = comment_text.get(match_end..).unwrap_or("").trim();
        if !label.is_empty() {
            caps.insert(Arc::from(name), Arc::from(label));
        }
    }
    caps
}

// ---------------------------------------------------------------------------
// Body parser — ported from crates/sprf/src/_3_lower.rs::split_comment_args
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum MarkerArg {
    Regex(String),
    Capture(String),
}

fn split_args(body: &str) -> Result<Vec<MarkerArg>, String> {
    let mut args = Vec::new();
    let mut rest = body.trim();
    loop {
        if rest.is_empty() { break; }
        if rest.starts_with('"') {
            let end = rest[1..].find('"')
                .ok_or_else(|| "unterminated string".to_string())?;
            args.push(MarkerArg::Regex(rest[1..=end].to_string()));
            rest = rest[end + 2..].trim();
            if rest.starts_with(',') { rest = rest[1..].trim(); }
        } else if rest.starts_with('$') {
            let after = &rest[1..];
            // Support both `$NAME` and `${NAME}` to match sprefa capture syntax.
            let (name, consumed) = if let Some(inner) = after.strip_prefix('{') {
                let close = inner.find('}').ok_or_else(|| "unterminated ${".to_string())?;
                (&inner[..close], 1 + 1 + close + 1)
            } else {
                let end = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(after.len());
                (&after[..end], 1 + end)
            };
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!("expected $NAME capture, got `{rest}`"));
            }
            args.push(MarkerArg::Capture(name.to_string()));
            rest = rest[consumed..].trim();
            if rest.starts_with(',') { rest = rest[1..].trim(); }
        } else {
            return Err(format!("expected quoted regex or $NAME, got `{rest}`"));
        }
    }
    if args.is_empty() {
        return Err("marker() requires at least 1 argument".to_string());
    }
    Ok(args)
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum MarkerDiag {
    MissingArg { site: ParseSite },
    BadArg     { site: ParseSite, msg: Arc<str> },
    ReadFailed { site: ParseSite, path: String },
    NoMatch    { site: ParseSite, path: String, pattern: String },
}

impl Diagnostic for MarkerDiag {
    fn code(&self) -> &str {
        match self {
            MarkerDiag::MissingArg { .. } => "marker/missing-arg",
            MarkerDiag::BadArg     { .. } => "marker/bad-arg",
            MarkerDiag::ReadFailed { .. } => "marker/read-failed",
            MarkerDiag::NoMatch    { .. } => "marker/no-match",
        }
    }
    fn severity(&self) -> Severity { Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            MarkerDiag::MissingArg { site }    => site,
            MarkerDiag::BadArg     { site, .. } => site,
            MarkerDiag::ReadFailed { site, .. } => site,
            MarkerDiag::NoMatch    { site, .. } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            MarkerDiag::MissingArg { site } => {
                out.header(self.code(), self.severity(),
                    "marker requires a pattern argument (e.g. marker(\"SECTION:\") or marker(\"BEGIN:\", \"END:\"))");
                out.primary(site);
            }
            MarkerDiag::BadArg { site, msg } => {
                out.header(self.code(), self.severity(),
                    &format!("marker argument is invalid: {msg}"));
                out.primary(site);
            }
            MarkerDiag::ReadFailed { site, path } => {
                out.header(self.code(), self.severity(),
                    &format!("marker failed to read `{path}`"));
                out.primary(site);
            }
            MarkerDiag::NoMatch { site, path, pattern } => {
                out.header(self.code(), self.severity(),
                    &format!("marker(`{pattern}`) matched nothing in `{path}`"));
                out.primary(site);
            }
        }
    }
}
