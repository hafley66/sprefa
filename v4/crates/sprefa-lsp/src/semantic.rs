//! Per-DSL semantic tokens for backtick bodies.
//!
//! Pipeline:
//!   1. host_parse(text) -> (Vec<PipeAst>, _) — discard diags here; the
//!      diag flow has its own publish pass.
//!   2. Walk PipeAst.steps recursively (incl. nested blocks). For each
//!      OpCall whose name maps to a DSL with a `DslBodyLsp` impl, run
//!      `dsl.semantic_tokens(raw_body_bytes)`.
//!   3. Each cst::SemanticToken carries a body-relative byte_range and a
//!      token_type index into `Legend::standard()`. Shift the range by
//!      the dsl_body span offset to get host-absolute bytes.
//!   4. Convert host bytes -> (line, utf-16 col); sort by (line, col);
//!      encode as relative `tower_lsp::lsp_types::SemanticToken`s.
//!
//! Legend on the wire: same string-typed names that
//! `cst::lsp::highlights::Legend::standard()` uses, in the same order.
//! Modifiers: DEFAULT_LIBRARY, READONLY (matches Legend::standard()).

use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType,
};

use v4::compile::ast::{OpCall, PipeAst};
use v4::compile::parse::host_parse;
use v4::cst::dsls::{glob::GlobDsl, markdown::MarkdownDsl, re::ReDsl};
use v4::cst::dsl::Dsl;

/// Token-type legend. Order MUST match `cst::lsp::highlights::Legend::standard()`
/// indices: cst-side tokens carry raw `u32` indices into that order.
pub fn legend_token_types() -> Vec<SemanticTokenType> {
    vec![
        SemanticTokenType::FUNCTION,
        SemanticTokenType::VARIABLE,
        SemanticTokenType::PARAMETER,
        SemanticTokenType::KEYWORD,
        SemanticTokenType::STRING,
        SemanticTokenType::NUMBER,
        SemanticTokenType::COMMENT,
        SemanticTokenType::OPERATOR,
        SemanticTokenType::TYPE,
        SemanticTokenType::PROPERTY,
        SemanticTokenType::NAMESPACE,
        SemanticTokenType::REGEXP,
        SemanticTokenType::MACRO,
        SemanticTokenType::CLASS,
        SemanticTokenType::ENUM_MEMBER,
    ]
}

/// Modifier legend. Order MUST match cst Legend::standard() modifier order.
pub fn legend_token_modifiers() -> Vec<SemanticTokenModifier> {
    vec![
        SemanticTokenModifier::DEFAULT_LIBRARY,
        SemanticTokenModifier::READONLY,
    ]
}

/// Top-level: take .sprf source, return relative-encoded semantic tokens.
pub fn tokens_for(text: &str) -> Vec<SemanticToken> {
    let (pipes, _diags) = host_parse(text);
    let mut absolute: Vec<AbsToken> = Vec::new();
    for p in &pipes {
        collect_pipe(p, text, &mut absolute);
    }
    encode(text, absolute)
}

/// Body-absolute token: host-byte range, type/modifier indices into the legend.
struct AbsToken {
    lo:    usize,
    hi:    usize,
    ttype: u32,
    mods:  u32,
}

fn collect_pipe(p: &PipeAst, src: &str, out: &mut Vec<AbsToken>) {
    for step in &p.steps {
        collect_step(step, src, out);
    }
}

fn collect_step(call: &OpCall, src: &str, out: &mut Vec<AbsToken>) {
    if let Some(dsl) = &call.dsl {
        if let Some(toks) = dsl_tokens_for(&call.name, dsl.raw.as_bytes()) {
            let off = dsl.span.lo as usize;
            for t in toks {
                let lo = off + t.byte_range.start;
                let hi = off + t.byte_range.end;
                if hi > lo && hi <= src.len() {
                    out.push(AbsToken {
                        lo,
                        hi,
                        ttype: t.token_type,
                        mods:  t.token_modifiers,
                    });
                }
            }
        }
    }
    if let Some(block) = &call.block {
        collect_pipe(block, src, out);
    }
}

/// Op-name → DSL provider. Returns `None` for unknown names or DSLs without
/// a body-LSP provider. `json` and `ast` lack mature highlight queries, so
/// they're deferred (returning `None` keeps host highlighting in charge).
fn dsl_tokens_for(op_name: &str, body: &[u8]) -> Option<Vec<v4::cst::SemanticToken>> {
    match op_name {
        "re"   => Some(ReDsl::new().lsp()?.semantic_tokens(body)),
        "glob" => Some(GlobDsl::new().lsp()?.semantic_tokens(body)),
        "render" => Some(MarkdownDsl::new().lsp()?.semantic_tokens(body)),
        // "ast" — ast-grep DSL has no DslBodyLsp impl yet (no metavar/keyword
        // node kinds exposed); revisit when the v4 ast grammar lands queries.
        // "json" — no `json` op in v4 op registry yet.
        _ => None,
    }
}

// ── absolute → relative LSP encoding ──────────────────────────────────

fn encode(src: &str, mut toks: Vec<AbsToken>) -> Vec<SemanticToken> {
    // Convert each absolute byte range to (line, utf16-col, length-utf16).
    // Multi-line bodies: split a token at newline boundaries so each emitted
    // SemanticToken stays on one line (LSP spec requirement).
    let mut rows: Vec<RowTok> = Vec::new();
    for t in toks.drain(..) {
        push_split_lines(src, &t, &mut rows);
    }
    rows.sort_by_key(|r| (r.line, r.start_col));

    let mut out: Vec<SemanticToken> = Vec::with_capacity(rows.len());
    let mut prev_line: u32 = 0;
    let mut prev_col:  u32 = 0;
    for r in rows {
        let delta_line = r.line - prev_line;
        let delta_col  = if delta_line == 0 { r.start_col - prev_col } else { r.start_col };
        out.push(SemanticToken {
            delta_line,
            delta_start: delta_col,
            length: r.length,
            token_type: r.ttype,
            token_modifiers_bitset: r.mods,
        });
        prev_line = r.line;
        prev_col  = r.start_col;
    }
    out
}

struct RowTok {
    line:      u32,
    start_col: u32,
    length:    u32,
    ttype:     u32,
    mods:      u32,
}

fn push_split_lines(src: &str, t: &AbsToken, out: &mut Vec<RowTok>) {
    let bytes = src.as_bytes();
    let lo = t.lo.min(src.len());
    let hi = t.hi.min(src.len());
    if hi <= lo { return; }
    let mut cur = lo;
    while cur < hi {
        // find next newline before hi
        let mut end = cur;
        while end < hi && bytes[end] != b'\n' {
            end += 1;
        }
        let (line, start_col) = byte_to_lc(src, cur);
        let length = utf16_len(&src[cur..end]);
        if length > 0 {
            out.push(RowTok {
                line,
                start_col,
                length,
                ttype: t.ttype,
                mods:  t.mods,
            });
        }
        if end == hi { break; }
        cur = end + 1; // skip the newline byte
    }
}

fn byte_to_lc(src: &str, off: usize) -> (u32, u32) {
    let off = off.min(src.len());
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    let bytes = src.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i == off { break; }
        if *b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let col = utf16_len(&src[line_start..off]);
    (line, col)
}

fn utf16_len(s: &str) -> u32 {
    if s.is_ascii() {
        s.len() as u32
    } else {
        s.chars().map(|c| c.len_utf16() as u32).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_body_emits_tokens_within_body_range() {
        // step 1 of pipe: `glob`src/**/*.rs``.
        let src = "glob`src/**/*.rs`";
        let toks = tokens_for(src);
        assert!(!toks.is_empty(), "expected glob tokens, got {:?}", toks);
        // dsl_body inner offset is 5 (after "glob`"), inner ends at 16.
        // Confirm first token's absolute column lands inside [5, 16].
        // Reconstruct absolute col of first token from delta encoding (single
        // line, deltas chain from 0).
        let first = &toks[0];
        assert_eq!(first.delta_line, 0);
        let col = first.delta_start;
        assert!(col >= 5 && col + first.length <= 16,
            "first token col={} length={} not in [5,16]", col, first.length);
    }

    #[test]
    fn unknown_op_emits_nothing() {
        let toks = tokens_for("str `hello world`");
        assert!(toks.is_empty(), "str body should not emit dsl tokens, got {:?}", toks);
    }

    #[test]
    fn render_markdown_body_emits_tokens() {
        let toks = tokens_for("render(:markdown)`## ${TITLE}\n| A | B |`");
        assert!(!toks.is_empty(), "expected markdown tokens, got {:?}", toks);
    }

    #[test]
    fn legends_match_cst_standard_order() {
        // Cross-check: legend types' string names in order match cst Legend.
        let cst_legend = v4::cst::lsp::highlights::Legend::standard();
        let lsp_types = legend_token_types();
        assert_eq!(cst_legend.types.len(), lsp_types.len());
        for (a, b) in cst_legend.types.iter().zip(lsp_types.iter()) {
            assert_eq!(a.as_str(), b.as_str(),
                "legend mismatch: cst={:?} lsp={:?}", a, b);
        }
    }
}
