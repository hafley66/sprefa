//! `comment(open_re [, close_re])` — narrow `cursor.byte_range` to
//! regions bounded by comment markers. v2 port of `marker` (v2/src/ops/
//! _10_marker.rs), renamed to `comment` for v3 per the author's wish
//! list.
//!
//! Two modes:
//!   * Single-arg: **sequential**. Every comment matching `open_re`
//!     opens a region; the next match (or EOF) closes it. Think bash
//!     `# SECTION: foo` dividers.
//!   * Two-arg: **paired**. Matches nest LIFO. An unpaired open
//!     collapses to the comment line itself so downstream ops still
//!     see a narrowed range.
//!
//! A trailing `$LABEL` capture in the open regex binds the trimmed
//! tail after the match into a SpanBacked `Capture` on the output
//! cursor.
//!
//! Comment lines are detected by line-prefix: `//`, `#`, `--`, `/*`,
//! `*`, `<!--`. The full grammar-driven detection from v2 is parked;
//! line-prefix covers the common comment shapes without dragging a
//! per-language tree-sitter parser into `pipeline`.
//!
//! The paren body is parsed as `regex` or `regex, regex` at op
//! construction time; there is no tree-sitter injection for this op
//! yet. A `comment` sub-grammar can land later without changing this
//! file's public surface.
//!
//! Spec: parse.md §14.5i.

use std::ops::Range;
use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use regex::bytes::Regex;

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::{ArgSpec, TermMode, Value};

const COMMENT_ARG_SPEC: &[ArgSpec] = &[ArgSpec::Str];

#[derive(Debug)]
pub struct CommentOp {
    open: Regex,
    close: Option<Regex>,
    /// Name of the `$LABEL` capture in the open regex, if the author
    /// declared one via `(?P<LABEL>...)` or an ordinary named group.
    /// Captured label bytes land as a SpanBacked capture on the output
    /// cursor.
    label_capture: Option<Arc<str>>,
    /// Name of the unbound term arg, if any. The byte range *between*
    /// the open and close marker lines (exclusive of both) is bound to
    /// this capture, so downstream `write_cursor(${SCOPE?}, :replace)`
    /// has a target. `None` when no term arg was supplied.
    scope_capture: Option<Arc<str>>,
}

impl CommentOp {
    /// Open-comment regex. Test inspection.
    pub fn open_re(&self) -> &Regex { &self.open }

    /// Close-comment regex if the two-arg form was used; `None` for single-arg.
    pub fn close_re(&self) -> Option<&Regex> { self.close.as_ref() }

    /// Construct from lowered `Vec<Value>`. Expects 1 or 2 slots, each
    /// a pattern op exposing a raw regex, or `Value::Atom` / `Value::Str`
    /// (coerced to anchored literal regex).
    pub fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        fn value_to_regex(v: &Value, slot: &str) -> Result<Regex, Vec<PatternDiagnostic>> {
            match v {
                Value::Op(op) if op.try_raw_regex().is_some() => {
                    Ok(op.try_raw_regex().unwrap().clone())
                }
                Value::Atom(s) | Value::Str(s) => {
                    Regex::new(&regex::escape(s)).map_err(|e| {
                        vec![PatternDiagnostic {
                            code: if slot == "open" { "comment/open-syntax" } else { "comment/close-syntax" },
                            message: format!("regex compile failed: {e}"),
                            byte_range: 0..0,
                        }]
                    })
                }
                other => Err(vec![PatternDiagnostic {
                    code: "value/wrong-kind",
                    message: format!(
                        "comment {slot} requires a regex pattern, atom, or string; got {:?}",
                        other
                    ),
                    byte_range: 0..0,
                }]),
            }
        }

        // Sift unbound term args (the `${SCOPE?}` introducer) out of the
        // marker slice. At most one is allowed; multiple is a diagnostic.
        // A bound `${SCOPE}` (Read mode) at this position is also wrong:
        // there is nothing to read against here, the term is naming a
        // *binding site*, not a reference.
        let mut scope_capture: Option<Arc<str>> = None;
        let mut markers: Vec<Value> = Vec::with_capacity(values.len());
        for v in values {
            match v {
                Value::Term { mode: TermMode::Unbound, name } => {
                    if scope_capture.is_some() {
                        return Err(vec![PatternDiagnostic {
                            code: "comment/multiple-scope-terms",
                            message: "comment accepts at most one `${SCOPE?}` term arg".into(),
                            byte_range: 0..0,
                        }]);
                    }
                    scope_capture = Some(name);
                }
                Value::Term { mode: TermMode::Read, name } => {
                    return Err(vec![PatternDiagnostic {
                        code: "comment/scope-term-must-be-unbound",
                        message: format!(
                            "comment scope term must use the unbound form `${{{name}?}}`, not `${{{name}}}`"
                        ),
                        byte_range: 0..0,
                    }]);
                }
                other => markers.push(other),
            }
        }

        match markers.as_slice() {
            [one] => {
                let open = value_to_regex(one, "open")?;
                let label_capture = first_named_capture(&open);
                Ok(CommentOp { open, close: None, label_capture, scope_capture })
            }
            [one, two] => {
                let open  = value_to_regex(one, "open")?;
                let close = value_to_regex(two, "close")?;
                let label_capture = first_named_capture(&open);
                Ok(CommentOp { open, close: Some(close), label_capture, scope_capture })
            }
            [] => Err(vec![PatternDiagnostic {
                code: "comment/missing-arg",
                message: "comment requires a pattern argument (e.g. comment(\"SECTION:\") or comment(\"BEGIN:\", \"END:\"))".into(),
                byte_range: 0..0,
            }]),
            _ => Err(vec![PatternDiagnostic {
                code: "comment/too-many-args",
                message: format!("comment accepts 1 or 2 regex arguments; got {}", markers.len()),
                byte_range: 0..0,
            }]),
        }
    }

    /// Parse the raw paren body. `"open_re"` or `"open_re, close_re"`.
    /// Bare commas inside regex character classes or escape sequences
    /// do not split; we walk the source tracking `[...]` and backslash
    /// escapes.
    pub fn from_source(src: &str) -> Result<Self, Vec<PatternDiagnostic>> {
        let arg = src.trim();
        if arg.is_empty() {
            return Err(vec![PatternDiagnostic {
                code: "comment/missing-arg",
                message:
                    "comment requires a pattern argument (e.g. comment(\"SECTION:\") or comment(\"BEGIN:\", \"END:\"))"
                        .into(),
                byte_range: 0..src.len(),
            }]);
        }

        let parts = split_top_level_comma(arg);
        match parts.as_slice() {
            [one] => {
                let open = compile_body(one, "comment/open-syntax")?;
                let label_capture = first_named_capture(&open);
                Ok(CommentOp { open, close: None, label_capture, scope_capture: None })
            }
            [one, two] => {
                let open = compile_body(one, "comment/open-syntax")?;
                let close = compile_body(two, "comment/close-syntax")?;
                let label_capture = first_named_capture(&open);
                Ok(CommentOp {
                    open,
                    close: Some(close),
                    label_capture,
                    scope_capture: None,
                })
            }
            _ => Err(vec![PatternDiagnostic {
                code: "comment/too-many-args",
                message: format!(
                    "comment accepts 1 or 2 regex arguments; got {}",
                    parts.len()
                ),
                byte_range: 0..src.len(),
            }]),
        }
    }
}

impl OpCtor for CommentOp {
    const NAME: &'static str = "comment";
    const BODY_GRAMMAR: &'static str = "regex(es), comma-separated; quotes optional";
    const DOC:  &'static str = "\
`comment(open_re[, close_re])`

Narrow `cursor.byte_range` to regions bounded by comment markers.

- `comment(\"SECTION:\")` — sequential: each match starts a region; the
  next match (or EOF) ends it.
- `comment(\"BEGIN:\", \"END:\")` — paired: matches nest by LIFO; an
  unpaired open collapses to the comment line itself.

A trailing `$LABEL` in the open regex binds the trimmed post-match
tail as a capture.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        Self::from_values(values)
    }
}

impl Op for CommentOp {
    fn name(&self) -> &'static str { "comment" }

    fn arg_spec(&self) -> &[ArgSpec] { COMMENT_ARG_SPEC }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            let Some(c) = crate::effects::ensure_content_loaded(ctx, c).await else {
                return Vec::new();
            };
            let active = c.active();
            let base = c.byte_range.start;
            let comments = extract_comments(active);
            if comments.is_empty() {
                return Vec::new();
            }
            let regions = match &self.close {
                Some(close) => paired_regions(
                    &comments,
                    &self.open,
                    close,
                    self.label_capture.as_deref(),
                    active.len(),
                ),
                None => sequential_regions(
                    &comments,
                    &self.open,
                    self.label_capture.as_deref(),
                    active.len(),
                ),
            };

            regions
                .into_iter()
                .map(|r| {
                    let mut out = c.narrow((base + r.start)..(base + r.end));
                    if let Some((name, byte_range)) = r.label {
                        out.captures.push(Capture::span_backed(
                            name,
                            (base + byte_range.start)..(base + byte_range.end),
                        ));
                        out.last_bound = Some(out.captures.last().unwrap().name.clone());
                    }
                    if let Some(scope_name) = self.scope_capture.as_ref() {
                        out.captures.push(Capture::span_backed(
                            scope_name.clone(),
                            (base + r.inner_start)..(base + r.inner_end),
                        ));
                        out.last_bound = Some(out.captures.last().unwrap().name.clone());
                    }
                    out
                })
                .collect()
        }))
    }

    fn cache_key(&self, h: &mut blake3::Hasher) -> bool {
        h.update(self.name().as_bytes());
        h.update(&[0u8]);
        h.update(self.open.as_str().as_bytes());
        h.update(&[0u8]);
        if let Some(close) = &self.close {
            h.update(close.as_str().as_bytes());
        }
        h.update(&[0u8]);
        if let Some(name) = &self.label_capture {
            h.update(name.as_bytes());
        }
        h.update(&[0u8]);
        if let Some(name) = &self.scope_capture {
            h.update(name.as_bytes());
        }
        true
    }
}

// --------------------------------------------------------------------
// Parsing: top-level comma split + regex compile with op-coded diag
// --------------------------------------------------------------------

/// Split on commas that are not inside `[...]` character classes and
/// not escaped by a preceding backslash. Returns 1 or 2+ pieces.
fn split_top_level_comma(src: &str) -> Vec<&str> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_class = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'[' if !in_class => {
                in_class = true;
                i += 1;
            }
            b']' if in_class => {
                in_class = false;
                i += 1;
            }
            b',' if !in_class => {
                out.push(src[start..i].trim());
                start = i + 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    out.push(src[start..].trim());
    out
}

fn compile_body(body: &str, code: &'static str) -> Result<Regex, Vec<PatternDiagnostic>> {
    let source = trim_quotes(body);
    Regex::new(source).map_err(|e| {
        vec![PatternDiagnostic {
            code,
            message: format!("regex compile failed: {e}"),
            byte_range: 0..body.len(),
        }]
    })
}

/// Accept quoted literals (v2-style) or raw regex source. `"X"` and
/// `'X'` unwrap; everything else passes through.
fn trim_quotes(s: &str) -> &str {
    let t = s.trim();
    if t.len() >= 2 {
        let b = t.as_bytes();
        if (b[0] == b'"' && b[b.len() - 1] == b'"')
            || (b[0] == b'\'' && b[b.len() - 1] == b'\'')
        {
            return &t[1..t.len() - 1];
        }
    }
    t
}

fn first_named_capture(re: &Regex) -> Option<Arc<str>> {
    re.capture_names().flatten().next().map(Arc::from)
}

// --------------------------------------------------------------------
// Comment extraction: line-prefix detection
// --------------------------------------------------------------------

#[derive(Debug)]
struct CommentLine<'a> {
    /// Trimmed text *after* the comment marker, for regex matching.
    text: &'a str,
    /// Byte offset into the active slice where the comment line starts.
    byte_start: usize,
    /// Byte offset into the active slice where the comment line ends
    /// (exclusive, before the trailing newline).
    byte_end: usize,
    /// Offset from `byte_start` to the first byte of `text`. Used to
    /// re-anchor a regex match into the full source.
    text_offset: usize,
}

fn extract_comments(src: &[u8]) -> Vec<CommentLine<'_>> {
    let mut out = Vec::new();
    let mut line_start = 0usize;
    for (i, &b) in src.iter().enumerate() {
        if b == b'\n' {
            push_if_comment(src, line_start, i, &mut out);
            line_start = i + 1;
        }
    }
    if line_start < src.len() {
        push_if_comment(src, line_start, src.len(), &mut out);
    }
    out
}

fn push_if_comment<'a>(
    src: &'a [u8],
    start: usize,
    end: usize,
    out: &mut Vec<CommentLine<'a>>,
) {
    let line = &src[start..end];
    let lead = leading_ws(line);
    let after_ws = &line[lead..];
    let prefix_len = comment_prefix_len(after_ws);
    if prefix_len == 0 {
        return;
    }
    let text_start_in_line = lead + prefix_len;
    // Skip whitespace between the prefix and the text so `# X` produces
    // `X` as the match subject, matching v2 semantics.
    let text_bytes = &line[text_start_in_line..];
    let extra_ws = leading_ws(text_bytes);
    let text_offset = text_start_in_line + extra_ws;
    let text_slice = &line[text_offset..];
    let Ok(text) = std::str::from_utf8(text_slice) else { return };
    out.push(CommentLine {
        text,
        byte_start: start,
        byte_end: end,
        text_offset,
    });
}

fn leading_ws(line: &[u8]) -> usize {
    line.iter().take_while(|b| **b == b' ' || **b == b'\t').count()
}

/// Return the byte length of the comment marker if `line` opens with
/// one; 0 otherwise. Covers `//`, `#`, `--`, `/*`, `*`, `<!--`.
fn comment_prefix_len(line: &[u8]) -> usize {
    if line.starts_with(b"<!--") { return 4; }
    if line.starts_with(b"//") { return 2; }
    if line.starts_with(b"/*") { return 2; }
    if line.starts_with(b"--") { return 2; }
    if line.starts_with(b"#") { return 1; }
    if line.starts_with(b"*") { return 1; }
    0
}

// --------------------------------------------------------------------
// Region pairing
// --------------------------------------------------------------------

struct Region {
    start: usize,
    end: usize,
    /// Inner range, exclusive of the open marker line and the close
    /// marker line (paired) or of just the open marker line (sequential).
    /// In active-slice coordinates; the op shifts by the cursor's
    /// `byte_range.start` before emitting captures.
    inner_start: usize,
    inner_end: usize,
    label: Option<(Arc<str>, Range<usize>)>,
}

fn sequential_regions(
    comments: &[CommentLine<'_>],
    open: &Regex,
    capture_name: Option<&str>,
    src_len: usize,
) -> Vec<Region> {
    let mut regions = Vec::new();
    // Track open-marker line span (start..end) so inner_start can skip
    // past the marker line itself.
    let mut opens: Vec<(usize, usize, Option<(Arc<str>, Range<usize>)>)> = Vec::new();
    for comment in comments {
        if let Some(m) = open.find(comment.text.as_bytes()) {
            let label = extract_label(comment, m.end(), open, capture_name);
            opens.push((comment.byte_start, comment.byte_end, label));
        }
    }
    for i in 0..opens.len() {
        let (start, marker_end, label) = &opens[i];
        let end = opens
            .get(i + 1)
            .map(|(s, _, _)| *s)
            .unwrap_or(src_len);
        let inner_start = (marker_end + 1).min(src_len);
        let inner_end = end.max(inner_start);
        regions.push(Region {
            start: *start,
            end,
            inner_start,
            inner_end,
            label: label.clone(),
        });
    }
    regions
}

fn paired_regions(
    comments: &[CommentLine<'_>],
    open: &Regex,
    close: &Regex,
    capture_name: Option<&str>,
    src_len: usize,
) -> Vec<Region> {
    let mut regions = Vec::new();
    // Stack frame carries open marker line span (byte_start, byte_end)
    // plus the optional label capture, so inner_start can skip past the
    // open marker line.
    let mut stack: Vec<(usize, usize, Option<(Arc<str>, Range<usize>)>)> = Vec::new();
    for comment in comments {
        if let Some(m) = open.find(comment.text.as_bytes()) {
            let label = extract_label(comment, m.end(), open, capture_name);
            stack.push((comment.byte_start, comment.byte_end, label));
        } else if close.is_match(comment.text.as_bytes()) {
            if let Some((start, marker_end, label)) = stack.pop() {
                let inner_start = (marker_end + 1).min(src_len);
                let inner_end = comment.byte_start.max(inner_start);
                regions.push(Region {
                    start,
                    end: comment.byte_end,
                    inner_start,
                    inner_end,
                    label,
                });
            }
        }
    }
    // Unpaired opens collapse to the next comment line (or EOF).
    for (start, marker_end, label) in stack {
        let next_line_end = comments
            .iter()
            .find(|c| c.byte_start > start)
            .map(|c| c.byte_start)
            .unwrap_or(src_len);
        let inner_start = (marker_end + 1).min(src_len);
        let inner_end = next_line_end.max(inner_start);
        regions.push(Region {
            start,
            end: next_line_end,
            inner_start,
            inner_end,
            label,
        });
    }
    regions
}

/// Extract the label capture from a comment-line match. Returns the
/// byte range relative to the active slice so the op can shift it into
/// the full source at emit time.
fn extract_label(
    comment: &CommentLine<'_>,
    match_end_in_text: usize,
    open: &Regex,
    capture_name: Option<&str>,
) -> Option<(Arc<str>, Range<usize>)> {
    let name = capture_name?;
    // Prefer named-group capture; fall back to the trimmed post-match
    // tail if the regex has no named group (v2-equivalent behavior).
    if let Some(caps) = open.captures(comment.text.as_bytes()) {
        if let Some(g) = caps.name(name) {
            let start = comment.byte_start + comment.text_offset + g.start();
            let end = comment.byte_start + comment.text_offset + g.end();
            return Some((Arc::from(name), start..end));
        }
    }
    // No named group matched; synthesize from the post-match tail.
    let tail = comment.text[match_end_in_text..].trim();
    if tail.is_empty() {
        return None;
    }
    let tail_rel_start = match_end_in_text
        + comment.text[match_end_in_text..]
            .bytes()
            .take_while(|b| *b == b' ' || *b == b'\t')
            .count();
    let start = comment.byte_start + comment.text_offset + tail_rel_start;
    let end = start + tail.len();
    Some((Arc::from(name), start..end))
}

// --------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor_of(bytes: &[u8]) -> Cursor {
        Cursor::new(Arc::from(bytes))
    }

    #[tokio::test]
    async fn sequential_then_paired_then_unpaired_open() {
        let ctx = RtCtx::default();

        // SEQUENTIAL: two `# SECTION: ...` markers; each opens a region
        // that runs until the next marker (last one runs to EOF).
        {
            let src = "# SECTION: A\nline1\nline2\n# SECTION: B\nline3\n";
            let op = CommentOp::from_source("SECTION:").unwrap();
            let out = op.pipe(&ctx, Arc::from(vec![cursor_of(src.as_bytes())])).await;
            assert_eq!(out.len(), 2);
            // First region starts at first `# SECTION: A` and ends at the
            // second marker.
            assert_eq!(out[0].byte_range.start, 0);
            assert_eq!(out[0].byte_range.end, src.find("# SECTION: B").unwrap());
            // Second runs to EOF.
            assert_eq!(out[1].byte_range.end, src.len());
        }

        // PAIRED: BEGIN/END nest; one complete pair yields one region
        // ending at the close line's newline.
        {
            let src = "# BEGIN: outer\nfoo\n# END:\ntail\n";
            let op = CommentOp::from_source("BEGIN:, END:").unwrap();
            let out = op.pipe(&ctx, Arc::from(vec![cursor_of(src.as_bytes())])).await;
            assert_eq!(out.len(), 1);
            let end_line_end = src.find("# END:").unwrap() + "# END:".len();
            assert_eq!(out[0].byte_range, 0..end_line_end);
        }

        // UNPAIRED OPEN collapses to the comment line itself; when
        // another comment follows, the region ends at that comment's
        // start byte (not the comment line length — covered by design
        // to keep the collapse observable as "small region").
        {
            let src = "# BEGIN: leak\nfoo\n# other: x\n";
            let op = CommentOp::from_source("BEGIN:, END:").unwrap();
            let out = op.pipe(&ctx, Arc::from(vec![cursor_of(src.as_bytes())])).await;
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].byte_range.start, 0);
            // Ends at the next comment line's start (not EOF).
            assert_eq!(out[0].byte_range.end, src.find("# other:").unwrap());
        }
    }

    // Label-capture extraction: a named group in the open regex
    // populates a Capture on every emitted cursor.
    #[tokio::test]
    async fn label_capture_from_named_group() {
        let ctx = RtCtx::default();
        let src = "// SECTION: foo\na\nb\n// SECTION: bar\nc\n";
        let op = CommentOp::from_source("SECTION:\\s+(?P<LABEL>\\S+)").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![cursor_of(src.as_bytes())])).await;
        assert_eq!(out.len(), 2);
        let c0 = out[0].capture("LABEL").unwrap();
        assert_eq!(c0.bytes(&out[0].content), b"foo");
        let c1 = out[1].capture("LABEL").unwrap();
        assert_eq!(c1.bytes(&out[1].content), b"bar");
    }

    // Parse-level diagnostics: missing arg, too many args, bad regex.
    #[test]
    fn diagnostics_for_bad_bodies() {
        assert_eq!(
            CommentOp::from_source("").unwrap_err()[0].code,
            "comment/missing-arg"
        );
        assert_eq!(
            CommentOp::from_source("A, B, C").unwrap_err()[0].code,
            "comment/too-many-args"
        );
        assert_eq!(
            CommentOp::from_source("[unclosed").unwrap_err()[0].code,
            "comment/open-syntax"
        );
    }

    // Comma-split must respect character classes so `[a,b]` stays one
    // regex.
    #[test]
    fn comma_in_character_class_stays_together() {
        let parts = split_top_level_comma(r"[a,b]+, END:");
        assert_eq!(parts, vec!["[a,b]+", "END:"]);
    }

    // Term-arg form via from_values: `comment(@begin, ${SCOPE?}, @end)`
    // — paired mode binds the inner-region byte_range to a capture
    // named "SCOPE" exclusive of both marker lines.
    #[tokio::test]
    async fn scope_capture_paired_three_arg_binds_inner_range() {
        let ctx = RtCtx::default();
        let src = "# BEGIN: outer\nINNER1\nINNER2\n# END:\n";
        let op = CommentOp::from_values(vec![
            Value::Atom(Arc::from("BEGIN:")),
            Value::Term { name: Arc::from("SCOPE"), mode: TermMode::Unbound },
            Value::Atom(Arc::from("END:")),
        ]).unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![cursor_of(src.as_bytes())])).await;
        assert_eq!(out.len(), 1);

        let scope = out[0].capture("SCOPE").expect("SCOPE capture present");
        let bytes = scope.bytes(&out[0].content);
        assert_eq!(bytes, b"INNER1\nINNER2\n");

        // Cursor still narrows to the OUTER region (markers included)
        // so subsequent ops see the whole block; capture targets inner.
        let outer = out[0].active();
        assert!(outer.starts_with(b"# BEGIN: outer"));
        assert!(outer.ends_with(b"# END:"));
    }

    // Sequential mode with scope term: `comment(@section, ${SCOPE?})`
    // binds the byte_range from after the marker line up to the next
    // marker (or EOF).
    #[tokio::test]
    async fn scope_capture_sequential_two_arg_binds_after_marker() {
        let ctx = RtCtx::default();
        let src = "# SECTION: A\nfoo\nbar\n# SECTION: B\nbaz\n";
        let op = CommentOp::from_values(vec![
            Value::Atom(Arc::from("SECTION:")),
            Value::Term { name: Arc::from("SCOPE"), mode: TermMode::Unbound },
        ]).unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![cursor_of(src.as_bytes())])).await;
        assert_eq!(out.len(), 2);

        let s0 = out[0].capture("SCOPE").unwrap();
        assert_eq!(s0.bytes(&out[0].content), b"foo\nbar\n");

        let s1 = out[1].capture("SCOPE").unwrap();
        assert_eq!(s1.bytes(&out[1].content), b"baz\n");
    }

    // Bound `${SCOPE}` (Read mode) at the comment scope-arg slot is
    // disallowed: there's nothing to read against; the term *names* a
    // binding, it isn't a reference.
    #[test]
    fn scope_term_must_be_unbound() {
        let err = CommentOp::from_values(vec![
            Value::Atom(Arc::from("BEGIN:")),
            Value::Term { name: Arc::from("SCOPE"), mode: TermMode::Read },
            Value::Atom(Arc::from("END:")),
        ]).unwrap_err();
        assert_eq!(err[0].code, "comment/scope-term-must-be-unbound");
    }

    #[test]
    fn multiple_unbound_terms_rejected() {
        let err = CommentOp::from_values(vec![
            Value::Atom(Arc::from("BEGIN:")),
            Value::Term { name: Arc::from("A"), mode: TermMode::Unbound },
            Value::Term { name: Arc::from("B"), mode: TermMode::Unbound },
            Value::Atom(Arc::from("END:")),
        ]).unwrap_err();
        assert_eq!(err[0].code, "comment/multiple-scope-terms");
    }
}
