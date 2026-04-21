//! md — match markdown structure (headings, lists, links, code blocks, tables, blockquotes).
//!
//! Surface syntax dispatches by leading token of the paren body:
//!   md(## $SECTION)       heading-scope: narrows cursor.byte_range to region
//!   md(- $ITEM)           list item fan-out
//!   md([$TEXT]($URL))     link extraction
//!   md(```$LANG)          fenced code block
//!   md(| $ROW |)          table row
//!   md(> $TEXT)           blockquote

use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream::{self, StreamExt};
use regex::Regex;
use tracing::{info_span, Instrument};

use crate::types::{Capture, Cursor, FilePath, OpEvidence, ParseSite, Severity};
use crate::diagnostic::{Diagnostic, Renderer};
use crate::op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation,
    Operator, Pipeline, ProgramCtx,
};

// ---------------------------------------------------------------------------
// Local MdPattern (mirrors v1 crates/rules/src/types.rs:385-432)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum MdPattern {
    /// Heading-scope or heading-element match.
    /// When text/capture are derived from a sole `$VAR` token, we use heading-scope mode.
    Heading {
        level:   u8,
        text:    Option<Arc<str>>,   // None = any heading; Some = literal or regex filter
        capture: Option<Arc<str>>,   // explicit capture name (sole $VAR case)
    },
    ListItem   { capture: Option<Arc<str>> },
    Link       { text_capture: Option<Arc<str>>, url_capture: Option<Arc<str>> },
    CodeBlock  { lang_capture: Option<Arc<str>> },
    TableRow   { capture: Option<Arc<str>> },
    Blockquote { capture: Option<Arc<str>> },
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct MdFactory;

impl Operator for MdFactory {
    fn name(&self) -> &'static str { "md" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("md-arg")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "md".to_string(),
            detail: "md(## $SECTION | - $ITEM | [$T]($U) | ```$LANG | | $R | | > $Q)".to_string(),
            doc:    "# md\n\nMatch markdown structure. Heading patterns scope the byte_range; element patterns fan-out to one cursor per match.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(MdDiag::MissingArg { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>]
        })?;
        let arg = paren.src.trim();
        if arg.is_empty() {
            return Err(vec![Box::new(MdDiag::MissingArg {
                site: (*inv.parse_site).clone(),
            }) as Box<dyn Diagnostic>]);
        }
        let pattern = parse_md_body(arg).map_err(|e| {
            vec![Box::new(MdDiag::BadArg {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from(e.as_str()),
            }) as Box<dyn Diagnostic>]
        })?;
        Ok(Pipeline::Op(Arc::new(MdOp {
            pattern:    Arc::new(pattern),
            parse_site: inv.parse_site.clone(),
        }).into()))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct MdOp {
    pattern:    Arc<MdPattern>,
    parse_site: Arc<ParseSite>,
}

impl Op for MdOp {
    fn name(&self) -> &'static str { "md" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn binds_captures(&self) -> Vec<Arc<str>> {
        let mut out: Vec<Arc<str>> = Vec::new();
        let mut push = |n: &str| {
            if !out.iter().any(|s: &Arc<str>| s.as_ref() == n) {
                out.push(Arc::from(n));
            }
        };
        match self.pattern.as_ref() {
            MdPattern::Heading { text, capture, .. } => {
                // Extract embedded $VAR names from the text filter (regex sugar).
                if let Some(t) = text {
                    for name in extract_dollar_names(t) { push(&name); }
                }
                if let Some(c) = capture { push(c); }
            }
            MdPattern::ListItem       { capture }                   => { if let Some(c) = capture { push(c); } }
            MdPattern::Link           { text_capture, url_capture } => {
                if let Some(c) = text_capture { push(c); }
                if let Some(c) = url_capture  { push(c); }
            }
            MdPattern::CodeBlock      { lang_capture }              => { if let Some(c) = lang_capture { push(c); } }
            MdPattern::TableRow       { capture }                   => { if let Some(c) = capture { push(c); } }
            MdPattern::Blockquote     { capture }                   => { if let Some(c) = capture { push(c); } }
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
        let header = format!("`${}` (from md)", cap);
        hover_render_grouped(&header, &entries)
    }

    fn pipe(&self, input: BoxStream<'static, Arc<[Cursor]>>, ctx: OpCtx)
        -> BoxStream<'static, Arc<[Cursor]>>
    {
        let _pipe_span = info_span!("op.md.pipe").entered();
        let pattern = self.pattern.clone();
        let site    = self.parse_site.clone();
        let reader  = ctx.reader.clone();
        let diags   = ctx.diags.clone();

        input.flat_map(move |batch| {
            let batch_span = info_span!("op.md.batch", n = batch.len());
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
                                diags.0(Box::new(MdDiag::ReadFailed {
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

                    let per_cursor = run_pattern(
                        sliced, &pattern, c, &content_arc, base_offset, &fp, &site,
                    );

                    if per_cursor.is_empty() {
                        diags.0(Box::new(MdDiag::NoMatch {
                            site: (*site).clone(),
                            path: fp.0.to_string_lossy().into_owned(),
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
// Runtime matching
// ---------------------------------------------------------------------------

fn run_pattern(
    sliced:      &[u8],
    pattern:     &MdPattern,
    c:           &Cursor,
    content_arc: &Arc<Bytes>,
    base_offset: usize,
    fp:          &FilePath,
    site:        &Arc<ParseSite>,
) -> Vec<Cursor> {
    match pattern {
        MdPattern::Heading { level, text, capture } => {
            run_heading(sliced, *level, text.as_deref(), capture.as_deref(),
                        c, content_arc, base_offset, fp, site)
        }
        MdPattern::ListItem { capture } =>
            run_list_items(sliced, capture.as_deref(), c, content_arc, base_offset, fp, site),
        MdPattern::Link { text_capture, url_capture } =>
            run_links(sliced, text_capture.as_deref(), url_capture.as_deref(),
                      c, content_arc, base_offset, fp, site),
        MdPattern::CodeBlock { lang_capture } =>
            run_code_blocks(sliced, lang_capture.as_deref(), c, content_arc, base_offset, fp, site),
        MdPattern::TableRow { capture } =>
            run_table_rows(sliced, capture.as_deref(), c, content_arc, base_offset, fp, site),
        MdPattern::Blockquote { capture } =>
            run_blockquotes(sliced, capture.as_deref(), c, content_arc, base_offset, fp, site),
    }
}

// Heading-scope: one cursor per matching heading, byte_range narrowed to region.
fn run_heading(
    sliced:      &[u8],
    level:       u8,
    text:        Option<&str>,
    capture:     Option<&str>,
    c:           &Cursor,
    content_arc: &Arc<Bytes>,
    base_offset: usize,
    fp:          &FilePath,
    site:        &Arc<ParseSite>,
) -> Vec<Cursor> {
    let src = match std::str::from_utf8(sliced) { Ok(s) => s, Err(_) => return vec![] };

    let text_re = text.and_then(|t| {
        let pat = capture_pattern_to_regex(t);
        Regex::new(&pat).ok()
    });

    struct Region {
        start:   usize,
        end:     usize,
        heading: String,         // heading line text (for evidence.matched)
        caps:    Vec<(Arc<str>, Arc<str>)>,
    }

    let mut regions: Vec<Region> = vec![];
    // open region = (region_start, heading_matched_text, captures)
    let mut current: Option<(usize, String, Vec<(Arc<str>, Arc<str>)>)> = None;

    let mut offset = 0usize;
    for line in src.split('\n') {
        let line_start = offset;
        let line_end   = offset + line.len();

        if let Some((h_level, h_text)) = parse_heading_line(line) {
            if let Some((start, heading_text, caps)) = current.take() {
                if h_level <= level {
                    regions.push(Region { start, end: line_start, heading: heading_text, caps });
                } else {
                    current = Some((start, heading_text, caps));
                }
            }
            if h_level == level {
                let matched = match &text_re {
                    Some(re) => re.is_match(h_text),
                    None     => text.map_or(true, |t| !t.contains('$') && h_text.trim() == t.trim()),
                };
                if matched {
                    let mut caps: Vec<(Arc<str>, Arc<str>)> = vec![];
                    if let Some(re) = &text_re {
                        if let Some(m) = re.captures(h_text) {
                            for name in re.capture_names().flatten() {
                                if let Some(val) = m.name(name) {
                                    caps.push((Arc::from(name), Arc::from(val.as_str())));
                                }
                            }
                        }
                    }
                    if let Some(cap) = capture {
                        if !caps.iter().any(|(n, _)| n.as_ref() == cap) {
                            caps.push((Arc::from(cap), Arc::from(h_text.trim())));
                        }
                    }
                    current = Some((line_start, line.to_string(), caps));
                }
            }
        }
        offset = line_end + 1;
    }
    if let Some((start, heading_text, caps)) = current {
        regions.push(Region { start, end: sliced.len(), heading: heading_text, caps });
    }

    regions.into_iter().map(|r| {
        let mut c2 = c.clone();
        c2.fs        = Some(fp.clone());
        c2.content   = Some(content_arc.clone());
        c2.byte_range = Some((base_offset + r.start)..(base_offset + r.end));
        for (name, val) in r.caps {
            c2.captures.insert(name, Capture::new(val));
        }
        c2.evidence.push(OpEvidence {
            op_name:    "md",
            parse_site: site.clone(),
            matched:    Arc::from(r.heading.as_str()),
            capture:    None,
        });
        c2
    }).collect()
}

fn run_list_items(
    sliced:      &[u8],
    capture:     Option<&str>,
    c:           &Cursor,
    content_arc: &Arc<Bytes>,
    base_offset: usize,
    fp:          &FilePath,
    site:        &Arc<ParseSite>,
) -> Vec<Cursor> {
    let src = match std::str::from_utf8(sliced) { Ok(s) => s, Err(_) => return vec![] };
    let re = Regex::new(r"^[ \t]*(?:[-*+]|\d+[.)]) (.+)").unwrap();
    let mut out = vec![];
    let mut offset = 0usize;
    for line in src.split('\n') {
        let line_start = offset;
        let line_end   = offset + line.len();
        if let Some(m) = re.captures(line) {
            let text = m.get(1).unwrap().as_str();
            let mut c2 = c.clone();
            c2.fs        = Some(fp.clone());
            c2.content   = Some(content_arc.clone());
            c2.byte_range = Some((base_offset + line_start)..(base_offset + line_end));
            if let Some(cap) = capture {
                c2.captures.insert(Arc::from(cap), Capture::new(Arc::from(text)));
            }
            c2.evidence.push(OpEvidence {
                op_name:    "md",
                parse_site: site.clone(),
                matched:    Arc::from(line),
                capture:    None,
            });
            out.push(c2);
        }
        offset = line_end + 1;
    }
    out
}

fn run_links(
    sliced:       &[u8],
    text_capture: Option<&str>,
    url_capture:  Option<&str>,
    c:            &Cursor,
    content_arc:  &Arc<Bytes>,
    base_offset:  usize,
    fp:           &FilePath,
    site:         &Arc<ParseSite>,
) -> Vec<Cursor> {
    let src = match std::str::from_utf8(sliced) { Ok(s) => s, Err(_) => return vec![] };
    let re = Regex::new(r"\[([^\]]*)\]\(([^)]*)\)").unwrap();
    let mut out = vec![];
    let mut offset = 0usize;
    for line in src.split('\n') {
        let line_start = offset;
        let line_end   = offset + line.len();
        for m in re.captures_iter(line) {
            let text = m.get(1).unwrap().as_str();
            let url  = m.get(2).unwrap().as_str();
            let mut c2 = c.clone();
            c2.fs        = Some(fp.clone());
            c2.content   = Some(content_arc.clone());
            c2.byte_range = Some((base_offset + line_start)..(base_offset + line_end));
            if let Some(cap) = text_capture {
                c2.captures.insert(Arc::from(cap), Capture::new(Arc::from(text)));
            }
            if let Some(cap) = url_capture {
                c2.captures.insert(Arc::from(cap), Capture::new(Arc::from(url)));
            }
            c2.evidence.push(OpEvidence {
                op_name:    "md",
                parse_site: site.clone(),
                matched:    Arc::from(line),
                capture:    None,
            });
            out.push(c2);
        }
        offset = line_end + 1;
    }
    out
}

fn run_code_blocks(
    sliced:        &[u8],
    lang_capture:  Option<&str>,
    c:             &Cursor,
    content_arc:   &Arc<Bytes>,
    _base_offset:  usize,
    fp:            &FilePath,
    site:          &Arc<ParseSite>,
) -> Vec<Cursor> {
    let src = match std::str::from_utf8(sliced) { Ok(s) => s, Err(_) => return vec![] };
    let mut out = vec![];
    let mut in_block = false;
    let mut block_lang = String::new();
    let mut fence_line = String::new();
    let mut offset = 0usize;
    for line in src.split('\n') {
        let line_end = offset + line.len();
        if !in_block {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                in_block   = true;
                block_lang = trimmed[3..].trim().to_string();
                fence_line = line.to_string();
            }
        } else if line.trim_start().starts_with("```") {
            let mut c2 = c.clone();
            c2.fs        = Some(fp.clone());
            c2.content   = Some(content_arc.clone());
            // byte_range set to fence open line (evidence matches the open fence).
            // No sub-range narrowing for code blocks in this port.
            if let Some(cap) = lang_capture {
                c2.captures.insert(Arc::from(cap), Capture::new(Arc::from(block_lang.as_str())));
            }
            c2.evidence.push(OpEvidence {
                op_name:    "md",
                parse_site: site.clone(),
                matched:    Arc::from(fence_line.as_str()),
                capture:    None,
            });
            out.push(c2);
            in_block = false;
        }
        offset = line_end + 1;
    }
    out
}

fn run_table_rows(
    sliced:   &[u8],
    capture:  Option<&str>,
    c:        &Cursor,
    content_arc: &Arc<Bytes>,
    base_offset: usize,
    fp:       &FilePath,
    site:     &Arc<ParseSite>,
) -> Vec<Cursor> {
    let src = match std::str::from_utf8(sliced) { Ok(s) => s, Err(_) => return vec![] };
    let mut out = vec![];
    let mut offset = 0usize;
    for line in src.split('\n') {
        let line_start = offset;
        let line_end   = offset + line.len();
        let trimmed = line.trim();
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let inner = &trimmed[1..trimmed.len() - 1];
            if inner.chars().all(|c| c == '-' || c == '|' || c == ' ' || c == ':') {
                offset = line_end + 1;
                continue;
            }
            let mut c2 = c.clone();
            c2.fs        = Some(fp.clone());
            c2.content   = Some(content_arc.clone());
            c2.byte_range = Some((base_offset + line_start)..(base_offset + line_end));
            if let Some(cap) = capture {
                c2.captures.insert(Arc::from(cap), Capture::new(Arc::from(trimmed)));
            }
            c2.evidence.push(OpEvidence {
                op_name:    "md",
                parse_site: site.clone(),
                matched:    Arc::from(trimmed),
                capture:    None,
            });
            out.push(c2);
        }
        offset = line_end + 1;
    }
    out
}

fn run_blockquotes(
    sliced:   &[u8],
    capture:  Option<&str>,
    c:        &Cursor,
    content_arc: &Arc<Bytes>,
    base_offset: usize,
    fp:       &FilePath,
    site:     &Arc<ParseSite>,
) -> Vec<Cursor> {
    let src = match std::str::from_utf8(sliced) { Ok(s) => s, Err(_) => return vec![] };
    let mut out = vec![];
    let mut offset = 0usize;
    for line in src.split('\n') {
        let line_start = offset;
        let line_end   = offset + line.len();
        let trimmed = line.trim_start();
        if trimmed.starts_with("> ") || trimmed == ">" {
            let text = if trimmed.len() > 2 { &trimmed[2..] } else { "" };
            let mut c2 = c.clone();
            c2.fs        = Some(fp.clone());
            c2.content   = Some(content_arc.clone());
            c2.byte_range = Some((base_offset + line_start)..(base_offset + line_end));
            if let Some(cap) = capture {
                c2.captures.insert(Arc::from(cap), Capture::new(Arc::from(text)));
            }
            c2.evidence.push(OpEvidence {
                op_name:    "md",
                parse_site: site.clone(),
                matched:    Arc::from(trimmed),
                capture:    None,
            });
            out.push(c2);
        }
        offset = line_end + 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Body parser (ported from _3_lower.rs:738)
// ---------------------------------------------------------------------------

fn parse_md_body(body: &str) -> Result<MdPattern, String> {
    let body = body.trim();

    if body.starts_with('#') {
        let hashes = body.bytes().take_while(|&b| b == b'#').count();
        if hashes > 6 {
            return Err(format!("md() heading level must be 1-6, got {}", hashes));
        }
        let rest = body[hashes..].trim();
        let (text, capture) = parse_md_text_and_capture(rest);
        return Ok(MdPattern::Heading {
            level:   hashes as u8,
            text:    text.map(|s| Arc::from(s.as_str())),
            capture: capture.map(|s| Arc::from(s.as_str())),
        });
    }

    if body.starts_with("- ") || body.starts_with("* ") || body.starts_with("+ ") {
        let rest = body[2..].trim();
        let capture = extract_sole_capture(rest).map(|s| Arc::from(s.as_str()));
        return Ok(MdPattern::ListItem { capture });
    }
    if body.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        if let Some(idx) = body.find(". ").or_else(|| body.find(") ")) {
            let rest = body[idx + 2..].trim();
            let capture = extract_sole_capture(rest).map(|s| Arc::from(s.as_str()));
            return Ok(MdPattern::ListItem { capture });
        }
    }

    if body.starts_with('[') && body.contains("](") && body.ends_with(')') {
        let bracket_end = body.find("](").unwrap();
        let text_part   = &body[1..bracket_end];
        let url_part    = &body[bracket_end + 2..body.len() - 1];
        let text_capture = extract_sole_capture(text_part).map(|s| Arc::from(s.as_str()));
        let url_capture  = extract_sole_capture(url_part).map(|s| Arc::from(s.as_str()));
        return Ok(MdPattern::Link { text_capture, url_capture });
    }

    if body.starts_with("```") {
        let rest = body[3..].trim();
        let lang_capture = extract_sole_capture(rest).map(|s| Arc::from(s.as_str()));
        return Ok(MdPattern::CodeBlock { lang_capture });
    }

    if body.starts_with('|') && body.ends_with('|') {
        let capture = extract_sole_capture(body).map(|s| Arc::from(s.as_str()));
        return Ok(MdPattern::TableRow { capture });
    }

    if body.starts_with("> ") || body == ">" {
        let rest = if body.len() > 2 { body[2..].trim() } else { "" };
        let capture = extract_sole_capture(rest).map(|s| Arc::from(s.as_str()));
        return Ok(MdPattern::Blockquote { capture });
    }

    Err(format!(
        "md() pattern not recognized: {:?}. Expected heading (#), list (- ), link ([]()), \
         code block (```), table (| |), or blockquote (>)",
        body
    ))
}

/// Sole-capture check: body is exactly a `$VAR` with no surrounding text → return var name.
fn extract_sole_capture(body: &str) -> Option<String> {
    let body = body.trim();
    if body.starts_with('$') && !body.starts_with("$$$") && !body.contains(' ') {
        let var = &body[1..];
        if !var.is_empty() && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Some(var.to_string());
        }
    }
    None
}

/// Parse heading text: returns (text_filter, capture_name).
/// Sole `$VAR` → (Some("$VAR"), Some("VAR")); literal → (Some(lit), None); embedded → (Some(text), None).
fn parse_md_text_and_capture(text: &str) -> (Option<String>, Option<String>) {
    if text.is_empty() { return (None, None); }

    if text.starts_with('$')
        && !text.starts_with("$$$")
        && !text.contains('/')
        && !text.contains(' ')
    {
        let var = &text[1..];
        if !var.is_empty()
            && var.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        {
            return (Some(text.to_string()), Some(var.to_string()));
        }
    }

    if text.contains('$') {
        return (Some(text.to_string()), None);
    }

    (Some(text.to_string()), None)
}

/// Convert a capture-sugar heading text like `$NAME-service` to a full regex.
/// `$VAR` → `(?P<VAR>\S+)`; `$$$VAR` → `(?P<VAR>.+)`; literal chars are regex-escaped.
fn capture_pattern_to_regex(pattern: &str) -> String {
    if !pattern.contains('$') {
        return regex::escape(pattern.trim());
    }
    let mut out = String::from('^');
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            let multi = i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'$';
            if multi { i += 2; }
            if i < bytes.len() && bytes[i] == b'_' {
                out.push_str(if multi { ".+" } else { "\\S+" });
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i > start {
                let name = &pattern[start..i];
                out.push_str(&format!("(?P<{}>{})", name, if multi { ".+" } else { "\\S+" }));
            }
        } else {
            out.push_str(&regex::escape(&pattern[i..i + 1]));
            i += 1;
        }
    }
    out.push('$');
    out
}

/// Parse a line as a markdown heading. Returns (level, heading_text_after_space).
fn parse_heading_line(line: &str) -> Option<(u8, &str)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') { return None; }
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 { return None; }
    let rest = &trimmed[hashes..];
    if rest.is_empty() { return Some((hashes as u8, "")); }
    if !rest.starts_with(' ') { return None; }
    Some((hashes as u8, &rest[1..]))
}

/// Extract all `$VAR` names from a text-filter string (for binds_captures).
fn extract_dollar_names(text: &str) -> Vec<String> {
    let mut out = vec![];
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            // Skip $$$ (greedy sugar counts as a capture var too)
            let multi = i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'$';
            if multi { i += 2; }
            if i < bytes.len() && bytes[i] == b'_' { i += 1; continue; }
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i > start {
                out.push(text[start..i].to_string());
            }
        } else {
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum MdDiag {
    MissingArg { site: ParseSite },
    BadArg     { site: ParseSite, msg: Arc<str> },
    ReadFailed { site: ParseSite, path: String },
    NoMatch    { site: ParseSite, path: String },
}

impl Diagnostic for MdDiag {
    fn code(&self) -> &str {
        match self {
            MdDiag::MissingArg { .. } => "md/missing-arg",
            MdDiag::BadArg     { .. } => "md/bad-arg",
            MdDiag::ReadFailed { .. } => "md/read-failed",
            MdDiag::NoMatch    { .. } => "md/no-match",
        }
    }
    fn severity(&self) -> Severity { Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            MdDiag::MissingArg { site }    => site,
            MdDiag::BadArg     { site, .. } => site,
            MdDiag::ReadFailed { site, .. } => site,
            MdDiag::NoMatch    { site, .. } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            MdDiag::MissingArg { site } => {
                out.header(self.code(), self.severity(),
                    "md requires a pattern argument (e.g. md(## $SECTION) or md(- $ITEM))");
                out.primary(site);
            }
            MdDiag::BadArg { site, msg } => {
                out.header(self.code(), self.severity(),
                    &format!("md pattern is invalid: {msg}"));
                out.primary(site);
            }
            MdDiag::ReadFailed { site, path } => {
                out.header(self.code(), self.severity(),
                    &format!("md failed to read `{path}`"));
                out.primary(site);
            }
            MdDiag::NoMatch { site, path } => {
                out.header(self.code(), self.severity(),
                    &format!("md pattern matched nothing in `{path}`"));
                out.primary(site);
            }
        }
    }
}
