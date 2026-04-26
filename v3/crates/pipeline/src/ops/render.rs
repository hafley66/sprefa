//! `render[fmt](template)` — universal output template op.
//!
//! Bracket-arg picks the format (md, ascii, rust, json, sql, …). Paren
//! body is a literal-bytes-with-carveouts template, same shape as `str`.
//! Term reads (`${X}`) substitute capture bytes from the in-flight
//! cursor. The rendered bytes become the output cursor's content;
//! byte_range is reset to 0..rendered_len. Downstream `write_cursor`
//! reads `cursor.active()` for the bytes-to-write and pulls its target
//! byte_range from a separately-bound capture on the same cursor.
//!
//! Format is parsed and validated against a known set so typos in the
//! bracket-arg surface a clean diagnostic, but it does not flow onto
//! the cursor. Future format-specific escaping (markdown table cells,
//! JSON string escaping) will hook off `op.format` at compile time.
//!
//! Per session 20260426 vision: render is the universal output op.
//! lsp[severity] used to live under render[lsp]; that's been split out
//! as its own op since LSP diagnostics carry richer kwargs.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use tree_sitter::Node;

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderFormat {
    Md,
    Ascii,
    Rust,
    Json,
    Sql,
    Plain,
}

impl RenderFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "md" | "markdown"   => Some(RenderFormat::Md),
            "ascii" | "txt"     => Some(RenderFormat::Ascii),
            "rust" | "rs"       => Some(RenderFormat::Rust),
            "json"              => Some(RenderFormat::Json),
            "sql"               => Some(RenderFormat::Sql),
            "plain" | ""        => Some(RenderFormat::Plain),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RenderFormat::Md    => "md",
            RenderFormat::Ascii => "ascii",
            RenderFormat::Rust  => "rust",
            RenderFormat::Json  => "json",
            RenderFormat::Sql   => "sql",
            RenderFormat::Plain => "plain",
        }
    }
}

/// One element of a render template. Same shape as `StrSeg`.
#[derive(Debug, Clone)]
pub enum RenderSeg {
    Lit(Arc<str>),
    Term(Arc<str>),
}

#[derive(Debug)]
pub struct RenderOp {
    pub format:   RenderFormat,
    pub template: Vec<RenderSeg>,
}

impl Op for RenderOp {
    fn name(&self) -> &'static str { "render" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            let mut buf = String::new();
            for seg in &self.template {
                match seg {
                    RenderSeg::Lit(s)  => buf.push_str(s),
                    RenderSeg::Term(n) => {
                        if let Some(cap) = c.capture(n) {
                            if let Ok(s) = std::str::from_utf8(cap.bytes(&c.content)) {
                                buf.push_str(s);
                            }
                        }
                    }
                }
            }
            let bytes: Arc<[u8]> = Arc::from(buf.into_bytes());
            let len = bytes.len();
            vec![c.rebase(bytes, 0..len)]
        }))
    }
}

impl OpCtor for RenderOp {
    const NAME: &'static str = "render";
    const BODY_GRAMMAR: &'static str = "literal bytes + ${NAME} reads";
    const DOC: &'static str = "\
**render**[_format_](_template_)

Universal output template. Format ∈ {md, ascii, rust, json, sql, plain}.
Paren body is literal bytes plus `${NAME}` reads of bound captures.
Result stored in `RenderedValue` slot for the next pipe step.
";

    fn from_values(_: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        Err(vec![PatternDiagnostic {
            code:       "render/value-path-unsupported",
            message:    "render parses paren body via from_paren_node, not values".into(),
            byte_range: 0..0,
        }])
    }

    fn from_paren_node(
        inv_node: Node<'_>,
        src:      &[u8],
    ) -> Option<Result<Self, Vec<PatternDiagnostic>>> {
        if inv_node.kind() != "op_invocation" {
            return None;
        }
        Some(lower_render(inv_node, src))
    }
}

fn lower_render(
    inv_node: Node<'_>,
    src:      &[u8],
) -> Result<RenderOp, Vec<PatternDiagnostic>> {
    let bracket = inv_node.child_by_field_name("bracket").ok_or_else(|| {
        vec![PatternDiagnostic {
            code:       "render/missing-format",
            message:    "render requires bracket-arg format, e.g. render[md](- ${X})".into(),
            byte_range: inv_node.byte_range(),
        }]
    })?;
    let br = bracket.byte_range();
    let bytes = if br.end < br.start + 2 { &[][..] } else { &src[br.start + 1..br.end - 1] };
    let raw = std::str::from_utf8(bytes)
        .map_err(|_| vec![PatternDiagnostic {
            code:       "render/bad-format",
            message:    "render format is not valid UTF-8".into(),
            byte_range: bracket.byte_range(),
        }])?;
    let format = RenderFormat::parse(raw).ok_or_else(|| vec![PatternDiagnostic {
        code:       "render/unknown-format",
        message:    format!(
            "unknown format `{}`; supported: md, ascii, rust, json, sql, plain",
            raw.trim(),
        ),
        byte_range: bracket.byte_range(),
    }])?;

    let template = build_template(inv_node, src)?;
    Ok(RenderOp { format, template })
}

fn build_template(
    inv_node: Node<'_>,
    src:      &[u8],
) -> Result<Vec<RenderSeg>, Vec<PatternDiagnostic>> {
    let Some(paren) = inv_node.child_by_field_name("paren") else {
        return Ok(Vec::new());
    };
    let p_start = paren.start_byte();
    let p_end   = paren.end_byte();
    if p_end < p_start + 2 {
        return Ok(Vec::new());
    }
    let body_start = p_start + 1;
    let body_end   = p_end - 1;

    let mut cuts: Vec<TermCut> = Vec::new();
    let mut walk = paren.walk();
    for child in paren.named_children(&mut walk) {
        match child.kind() {
            "term_ref" => {
                let name = parse_term_name(child, src);
                cuts.push(TermCut { name, range: child.byte_range() });
            }
            "carveout_expr" => {
                let Some(body) = child.child_by_field_name("body") else { continue; };
                match body.kind() {
                    "term_ref" => {
                        let name = parse_term_name(body, src);
                        cuts.push(TermCut { name, range: child.byte_range() });
                    }
                    "pipe" => {
                        // Single bare-ident pipe = term-shorthand read.
                        if let Some(name) = single_bare_ident_pipe(body, src) {
                            cuts.push(TermCut { name, range: child.byte_range() });
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    cuts.sort_by_key(|c| c.range.start);

    let mut template: Vec<RenderSeg> = Vec::new();
    let mut cursor = body_start;
    for cut in cuts {
        if cut.range.start > cursor {
            push_lit(&mut template, &src[cursor..cut.range.start]);
        }
        template.push(RenderSeg::Term(cut.name));
        cursor = cut.range.end;
    }
    if cursor < body_end {
        push_lit(&mut template, &src[cursor..body_end]);
    }
    Ok(template)
}

fn push_lit(template: &mut Vec<RenderSeg>, bytes: &[u8]) {
    if bytes.is_empty() { return; }
    if let Ok(s) = std::str::from_utf8(bytes) {
        template.push(RenderSeg::Lit(Arc::from(s)));
    }
}

struct TermCut {
    name:  Arc<str>,
    range: std::ops::Range<usize>,
}

fn parse_term_name(node: Node<'_>, src: &[u8]) -> Arc<str> {
    let raw = std::str::from_utf8(&src[node.byte_range()]).unwrap_or("");
    let stripped = raw.trim_start_matches(['$', '{']).trim_end_matches('}');
    let no_q = stripped.strip_suffix('?').unwrap_or(stripped);
    Arc::from(no_q)
}

fn single_bare_ident_pipe(pipe: Node<'_>, src: &[u8]) -> Option<Arc<str>> {
    let mut walk = pipe.walk();
    let steps: Vec<Node<'_>> = pipe
        .named_children(&mut walk)
        .filter(|n| n.kind() == "op_invocation")
        .collect();
    if steps.len() != 1 { return None; }
    let inv = steps[0];
    if inv.child_by_field_name("paren").is_some()
        || inv.child_by_field_name("brace").is_some()
        || inv.child_by_field_name("bracket").is_some()
    {
        return None;
    }
    let name = inv.child_by_field_name("name")?;
    let text = std::str::from_utf8(&src[name.byte_range()]).ok()?;
    Some(Arc::from(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_cursor::Capture;
    use tree_sitter::Parser;

    fn host_lang() -> tree_sitter::Language {
        tree_sitter_sprefa::LANGUAGE.into()
    }

    fn lower(src: &str) -> Result<RenderOp, Vec<PatternDiagnostic>> {
        let mut parser = Parser::new();
        parser.set_language(&host_lang()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let inv = find_op_inv(tree.root_node()).expect("no op_invocation");
        lower_render(inv, src.as_bytes())
    }

    fn find_op_inv(n: Node<'_>) -> Option<Node<'_>> {
        if n.kind() == "op_invocation" { return Some(n); }
        let mut walk = n.walk();
        for ch in n.named_children(&mut walk) {
            if let Some(v) = find_op_inv(ch) { return Some(v); }
        }
        None
    }

    #[test]
    fn lower_md_with_term_substitution() {
        let op = lower("render[md](- ${X})").unwrap();
        assert_eq!(op.format, RenderFormat::Md);
        assert_eq!(op.template.len(), 2);
        match &op.template[0] {
            RenderSeg::Lit(s) => assert_eq!(&**s, "- "),
            _ => panic!(),
        }
        match &op.template[1] {
            RenderSeg::Term(n) => assert_eq!(&**n, "X"),
            _ => panic!(),
        }
    }

    #[test]
    fn lower_ascii_literal_only() {
        let op = lower("render[ascii](static line of text)").unwrap();
        assert_eq!(op.format, RenderFormat::Ascii);
        assert_eq!(op.template.len(), 1);
    }

    #[test]
    fn lower_rejects_missing_format() {
        let err = lower("render(- ${X})").unwrap_err();
        assert_eq!(err[0].code, "render/missing-format");
    }

    #[test]
    fn lower_rejects_unknown_format() {
        let err = lower("render[bogus](- ${X})").unwrap_err();
        assert_eq!(err[0].code, "render/unknown-format");
    }

    #[tokio::test]
    async fn pipe_rebases_cursor_with_substituted_bytes() {
        let ctx = RtCtx::default();
        let op = lower("render[md](- ${X})").unwrap();
        let content: Arc<[u8]> = Arc::from(b"alice".as_slice());
        let mut c = Cursor::new(content);
        c.captures.push(Capture::span_backed(Arc::from("X"), 0..5));
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].active(), b"- alice");
        assert_eq!(out[0].byte_range, 0..7);
    }

    #[tokio::test]
    async fn pipe_preserves_fs_repo_rev_through_rebase() {
        let ctx = RtCtx::default();
        let op = lower("render[rust](use crate::${M};)").unwrap();
        let mut c = Cursor::new(Arc::from(b"foobar".as_slice()));
        c.captures.push(Capture::span_backed(Arc::from("M"), 0..6));
        c.repo = Arc::from("acme/widgets");
        c.rev = Arc::from("main");
        c.fs = Some(Arc::<std::path::Path>::from(std::path::PathBuf::from("src/lib.rs")));
        let out_batch = op.pipe(&ctx, Arc::from(vec![c])).await;
        let out = out_batch[0].clone();
        assert_eq!(out.active(), b"use crate::foobar;");
        assert_eq!(&*out.repo, "acme/widgets");
        assert_eq!(&*out.rev, "main");
        assert_eq!(out.fs.as_ref().unwrap().to_str(), Some("src/lib.rs"));
    }
}
