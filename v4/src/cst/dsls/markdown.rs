//! `markdown` — render-template body support for `render_markdown`.
//!
//! This DSL is body-LSP first. Runtime rendering still lives in the
//! `render_markdown` op; this provider gives VS Code enough nested
//! language behavior for headings, lists, tables, fences, and `${TERM}`
//! interpolation holes.

use lsp_types::{
    CompletionItem, Hover, HoverContents, MarkedString, SemanticTokenModifier, SemanticTokenType,
};

use crate::cst::diag::{Diag, DiagSink};
use crate::cst::dsl::{CaptureSink, Compiled, Dsl};
use crate::cst::lsp::highlights::Legend;
use crate::cst::lsp::providers::{DslBodyLsp, SemanticToken};

#[derive(Clone, Default)]
pub struct MarkdownDsl;

impl MarkdownDsl {
    pub fn new() -> Self {
        Self
    }
}

impl Dsl for MarkdownDsl {
    fn id(&self) -> &'static str {
        "markdown"
    }

    fn compile(&self, _body: &[u8], _diags: &dyn DiagSink) -> Result<Box<dyn Compiled>, Diag> {
        Ok(Box::new(MarkdownCompiled))
    }

    fn lsp(&self) -> Option<&dyn DslBodyLsp> {
        Some(self)
    }
}

struct MarkdownCompiled;

impl Compiled for MarkdownCompiled {
    fn match_into(&self, _target: &[u8], _target_off: usize, _sink: &mut dyn CaptureSink) {}
}

impl DslBodyLsp for MarkdownDsl {
    fn semantic_tokens(&self, body: &[u8]) -> Vec<SemanticToken> {
        let legend = Legend::standard();
        let t_keyword = legend.type_index(&SemanticTokenType::KEYWORD).unwrap_or(3);
        let t_operator = legend.type_index(&SemanticTokenType::OPERATOR).unwrap_or(7);
        let t_string = legend.type_index(&SemanticTokenType::STRING).unwrap_or(4);
        let t_parameter = legend
            .type_index(&SemanticTokenType::PARAMETER)
            .unwrap_or(2);
        let t_macro = legend.type_index(&SemanticTokenType::MACRO).unwrap_or(12);
        let readonly = legend.modifier_bits(&[SemanticTokenModifier::READONLY]);

        let mut out = Vec::new();
        let mut line_start = 0usize;
        let mut in_fence = false;
        while line_start <= body.len() {
            let line_end = body[line_start..]
                .iter()
                .position(|b| *b == b'\n')
                .map(|i| line_start + i)
                .unwrap_or(body.len());
            let line = &body[line_start..line_end];

            if line.starts_with(b"```") || line.starts_with(b"~~~") {
                out.push(tok(line_start..line_start + 3, t_macro, readonly));
                in_fence = !in_fence;
            } else if in_fence {
                if line_end > line_start {
                    out.push(tok(line_start..line_end, t_string, 0));
                }
            } else {
                push_markdown_line_tokens(line, line_start, &mut out, t_keyword, t_operator);
            }
            push_interpolation_tokens(line, line_start, &mut out, t_parameter);

            if line_end == body.len() {
                break;
            }
            line_start = line_end + 1;
        }
        out
    }

    fn hover(&self, body: &[u8], byte: usize) -> Option<Hover> {
        if let Some(hole) = interpolation_at(body, byte) {
            return Some(hover(format!(
                "render interpolation\nreads `{}` from the cursor terms or focal fields",
                hole
            )));
        }

        let (line, col) = line_at(body, byte)?;
        let trimmed = trim_ascii_start(line);
        let msg = if trimmed.starts_with(b"#") {
            "markdown heading"
        } else if trimmed.starts_with(b"- ") || trimmed.starts_with(b"* ") {
            "markdown list item"
        } else if trimmed.starts_with(b"```") || trimmed.starts_with(b"~~~") {
            "markdown fenced code boundary"
        } else if line.contains(&b'|') {
            "markdown table row"
        } else if col <= line.len() {
            "markdown paragraph text"
        } else {
            return None;
        };
        Some(hover(msg.to_string()))
    }

    fn completions(&self, _body: &[u8], _byte: usize) -> Vec<CompletionItem> {
        ["${&.value}", "${FS}", "${LO}", "${HI}", "${MATCH}"]
            .into_iter()
            .map(|label| CompletionItem {
                label: label.into(),
                detail: Some("render_markdown interpolation".into()),
                ..Default::default()
            })
            .collect()
    }
}

fn push_markdown_line_tokens(
    line: &[u8],
    line_start: usize,
    out: &mut Vec<SemanticToken>,
    t_keyword: u32,
    t_operator: u32,
) {
    let indent = line.iter().take_while(|b| b.is_ascii_whitespace()).count();
    let rest = &line[indent..];
    if rest.starts_with(b"#") {
        let n = rest.iter().take_while(|b| **b == b'#').count();
        out.push(tok(
            line_start + indent..line_start + indent + n,
            t_keyword,
            0,
        ));
    } else if rest.starts_with(b"- ") || rest.starts_with(b"* ") {
        out.push(tok(
            line_start + indent..line_start + indent + 1,
            t_operator,
            0,
        ));
    }

    for (i, b) in line.iter().enumerate() {
        if *b == b'|' {
            out.push(tok(line_start + i..line_start + i + 1, t_operator, 0));
        }
    }
}

fn push_interpolation_tokens(
    line: &[u8],
    line_start: usize,
    out: &mut Vec<SemanticToken>,
    t_parameter: u32,
) {
    let mut i = 0usize;
    while i + 3 <= line.len() {
        if line[i] == b'$' && line.get(i + 1) == Some(&b'{') {
            if let Some(rel_end) = line[i + 2..].iter().position(|b| *b == b'}') {
                let end = i + 2 + rel_end + 1;
                out.push(tok(line_start + i..line_start + end, t_parameter, 0));
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

fn interpolation_at(body: &[u8], byte: usize) -> Option<String> {
    let byte = byte.min(body.len());
    let start = body[..byte].iter().rposition(|b| *b == b'$')?;
    if body.get(start + 1) != Some(&b'{') {
        return None;
    }
    let end = body[start + 2..].iter().position(|b| *b == b'}')? + start + 2;
    if byte > end + 1 {
        return None;
    }
    Some(String::from_utf8_lossy(&body[start..=end]).into_owned())
}

fn line_at(body: &[u8], byte: usize) -> Option<(&[u8], usize)> {
    let byte = byte.min(body.len());
    let start = body[..byte]
        .iter()
        .rposition(|b| *b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = body[byte..]
        .iter()
        .position(|b| *b == b'\n')
        .map(|i| byte + i)
        .unwrap_or(body.len());
    Some((&body[start..end], byte - start))
}

fn trim_ascii_start(bytes: &[u8]) -> &[u8] {
    let n = bytes.iter().take_while(|b| b.is_ascii_whitespace()).count();
    &bytes[n..]
}

fn hover(value: String) -> Hover {
    Hover {
        contents: HoverContents::Scalar(MarkedString::String(value)),
        range: None,
    }
}

fn tok(range: std::ops::Range<usize>, token_type: u32, token_modifiers: u32) -> SemanticToken {
    SemanticToken {
        byte_range: range,
        token_type,
        token_modifiers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_semantic_tokens_cover_shapes_and_interps() {
        let dsl = MarkdownDsl::new();
        let tokens = dsl.semantic_tokens(b"## Title\n- ${NAME}\n| A | B |\n");
        assert!(
            tokens.iter().any(|t| t.byte_range == (0..2)),
            "expected heading marker token, got {tokens:?}",
        );
        assert!(
            tokens.iter().any(|t| t.byte_range == (11..18)),
            "expected interpolation token, got {tokens:?}",
        );
        assert!(
            tokens.iter().any(|t| t.byte_range == (19..20)),
            "expected table pipe token, got {tokens:?}",
        );
    }

    #[test]
    fn markdown_hover_classifies_interpolation_and_table() {
        let dsl = MarkdownDsl::new();
        let h = dsl.hover(b"- ${NAME}\n", 4).expect("interp hover");
        match h.contents {
            HoverContents::Scalar(MarkedString::String(s)) => {
                assert!(s.contains("${NAME}"));
            }
            other => panic!("unexpected hover {other:?}"),
        }

        let h = dsl.hover(b"| A | B |\n", 3).expect("table hover");
        match h.contents {
            HoverContents::Scalar(MarkedString::String(s)) => {
                assert_eq!(s, "markdown table row");
            }
            other => panic!("unexpected hover {other:?}"),
        }
    }
}
