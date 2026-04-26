//! `lsp[severity](message=..., code=..., hint=...)` — emit LSP diagnostic.
//!
//! Per session 20260426 vision: replaces `render[lsp](severity=...)`.
//! Bracket-arg discriminator picks one of `error` / `warn` / `hint` /
//! `info`. The op is a side-tap at the tail of a pipe: per incoming
//! cursor, an `LspDiagEffect` is emitted at `cursor.byte_range` against
//! `cursor.fs`. Cursor passes through unchanged so further pipe steps
//! (rare for a tail op, but allowed) still see it.
//!
//! Paren body is parsed as kwargs:
//!   message = "<string>"      required
//!   code    = "<string>"      optional
//!   hint    = "<string>"      optional
//!
//! Strings are quoted literals. Bare-ident or carveout reads (`${X}`)
//! are MVP-out-of-scope; will land with the templating pass.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use tree_sitter::Node;

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::effects::{LspDiagEffect, LspSeverity};
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

#[derive(Debug)]
pub struct LspDiagOp {
    pub severity: LspSeverity,
    pub message:  Arc<str>,
    pub code:     Option<Arc<str>>,
    pub hint:     Option<Arc<str>>,
}

impl Op for LspDiagOp {
    fn name(&self) -> &'static str { "lsp" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            ctx.put(LspDiagEffect {
                file:       c.fs.clone(),
                byte_range: c.byte_range.clone(),
                severity:   self.severity,
                message:    self.message.clone(),
                code:       self.code.clone(),
                hint:       self.hint.clone(),
            }).await;
            vec![c]
        }))
    }
}

impl OpCtor for LspDiagOp {
    const NAME: &'static str = "lsp";
    const BODY_GRAMMAR: &'static str = "kwargs: message=\"...\" [code=\"...\"] [hint=\"...\"]";
    const DOC: &'static str = "\
**lsp**[_severity_](message=_str_, [code=_str_], [hint=_str_])

Emit one LSP diagnostic per incoming cursor at `cursor.byte_range`
against `cursor.fs`. Severity is one of `error`, `warn`, `hint`, `info`.

The cursor flows through unchanged. Tail-of-pipe op in practice.
";

    fn from_values(_: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        Err(vec![PatternDiagnostic {
            code:       "lsp/value-path-unsupported",
            message:    "lsp parses paren body via from_paren_node, not values".into(),
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
        Some(lower_lsp(inv_node, src))
    }
}

fn lower_lsp(
    inv_node: Node<'_>,
    src:      &[u8],
) -> Result<LspDiagOp, Vec<PatternDiagnostic>> {
    let severity = parse_severity(inv_node, src)?;
    let kwargs = parse_kwargs(inv_node, src)?;

    let mut message: Option<Arc<str>> = None;
    let mut code:    Option<Arc<str>> = None;
    let mut hint:    Option<Arc<str>> = None;
    for (k, v) in kwargs {
        match k.as_ref() {
            "message" => message = Some(v),
            "code"    => code = Some(v),
            "hint"    => hint = Some(v),
            other => return Err(vec![PatternDiagnostic {
                code:       "lsp/unknown-kwarg",
                message:    format!("unknown kwarg `{other}`; allowed: message, code, hint"),
                byte_range: inv_node.byte_range(),
            }]),
        }
    }

    let Some(message) = message else {
        return Err(vec![PatternDiagnostic {
            code:       "lsp/missing-message",
            message:    "lsp requires `message=\"...\"`".into(),
            byte_range: inv_node.byte_range(),
        }]);
    };

    Ok(LspDiagOp { severity, message, code, hint })
}

fn parse_severity(
    inv_node: Node<'_>,
    src:      &[u8],
) -> Result<LspSeverity, Vec<PatternDiagnostic>> {
    let bracket = inv_node.child_by_field_name("bracket").ok_or_else(|| {
        vec![PatternDiagnostic {
            code:       "lsp/missing-severity",
            message:    "lsp requires bracket-arg severity, e.g. lsp[warn](message=\"...\")".into(),
            byte_range: inv_node.byte_range(),
        }]
    })?;
    let br = bracket.byte_range();
    let bytes = if br.end < br.start + 2 { &[][..] } else { &src[br.start + 1..br.end - 1] };
    let raw = std::str::from_utf8(bytes)
        .map_err(|_| vec![PatternDiagnostic {
            code:       "lsp/bad-severity",
            message:    "lsp severity is not valid UTF-8".into(),
            byte_range: bracket.byte_range(),
        }])?
        .trim();
    match raw {
        "error" | "err"          => Ok(LspSeverity::Error),
        "warn"  | "warning"      => Ok(LspSeverity::Warning),
        "hint"                   => Ok(LspSeverity::Hint),
        "info"  | "information"  => Ok(LspSeverity::Info),
        other => Err(vec![PatternDiagnostic {
            code:       "lsp/unknown-severity",
            message:    format!(
                "unknown severity `{other}`; allowed: error, warn, hint, info"
            ),
            byte_range: bracket.byte_range(),
        }]),
    }
}

/// Walk the paren_slot for `IDENT = "STRING"` pairs separated by commas.
/// Returns kwargs in source order.
fn parse_kwargs(
    inv_node: Node<'_>,
    src:      &[u8],
) -> Result<Vec<(Arc<str>, Arc<str>)>, Vec<PatternDiagnostic>> {
    let Some(paren) = inv_node.child_by_field_name("paren") else {
        return Ok(Vec::new());
    };
    let p_start = paren.start_byte();
    let p_end   = paren.end_byte();
    if p_end < p_start + 2 {
        return Ok(Vec::new());
    }
    let body = std::str::from_utf8(&src[p_start + 1..p_end - 1])
        .map_err(|_| vec![PatternDiagnostic {
            code:       "lsp/bad-utf8",
            message:    "lsp paren body is not valid UTF-8".into(),
            byte_range: paren.byte_range(),
        }])?;
    let mut out = Vec::new();
    for entry in split_top_level_commas(body) {
        let entry = entry.trim();
        if entry.is_empty() { continue; }
        let Some(eq_pos) = entry.find('=') else {
            return Err(vec![PatternDiagnostic {
                code:       "lsp/bad-kwarg",
                message:    format!("expected `IDENT = \"STRING\"`, got `{entry}`"),
                byte_range: paren.byte_range(),
            }]);
        };
        let key = entry[..eq_pos].trim();
        let val = entry[eq_pos + 1..].trim();
        if !is_ident(key) {
            return Err(vec![PatternDiagnostic {
                code:       "lsp/bad-kwarg",
                message:    format!("kwarg key `{key}` is not an identifier"),
                byte_range: paren.byte_range(),
            }]);
        }
        let stripped = strip_quotes(val).ok_or_else(|| vec![PatternDiagnostic {
            code:       "lsp/bad-kwarg",
            message:    format!("kwarg `{key}` value must be a quoted string"),
            byte_range: paren.byte_range(),
        }])?;
        out.push((Arc::from(key), Arc::from(stripped)));
    }
    Ok(out)
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut in_string = false;
    let mut escape    = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else {
            match b {
                b'"' => in_string = true,
                b',' => {
                    out.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    if start <= s.len() {
        out.push(&s[start..]);
    }
    out
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false; };
    if !(first.is_ascii_alphabetic() || first == '_') { return false; }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn strip_quotes(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if b.len() < 2 || b[0] != b'"' || b[b.len() - 1] != b'"' { return None; }
    let inner = &s[1..s.len() - 1];
    // Minimal escape handling: \" -> ", \\ -> \, \n -> newline.
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"')  => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n')  => out.push('\n'),
                Some('t')  => out.push('\t'),
                Some(other) => { out.push('\\'); out.push(other); }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::{LspDiagBatcher, LspDiagEffect};
    use effect_runtime::RtCtxBuilder;
    use tree_sitter::Parser;

    fn host_lang() -> tree_sitter::Language {
        tree_sitter_sprefa::LANGUAGE.into()
    }

    fn lower(src: &str) -> Result<LspDiagOp, Vec<PatternDiagnostic>> {
        let mut parser = Parser::new();
        parser.set_language(&host_lang()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let inv = find_lsp_inv(tree.root_node()).expect("no lsp op_invocation");
        lower_lsp(inv, src.as_bytes())
    }

    fn find_lsp_inv(n: Node<'_>) -> Option<Node<'_>> {
        if n.kind() == "op_invocation" { return Some(n); }
        let mut walk = n.walk();
        for ch in n.named_children(&mut walk) {
            if let Some(v) = find_lsp_inv(ch) { return Some(v); }
        }
        None
    }

    #[test]
    fn lower_warn_with_message() {
        let op = lower(r#"lsp[warn](message = "hello")"#).unwrap();
        assert_eq!(op.severity, LspSeverity::Warning);
        assert_eq!(&*op.message, "hello");
        assert!(op.code.is_none());
        assert!(op.hint.is_none());
    }

    #[test]
    fn lower_error_with_all_kwargs() {
        let op = lower(r#"lsp[error](message = "boom", code = "X1", hint = "fix it")"#).unwrap();
        assert_eq!(op.severity, LspSeverity::Error);
        assert_eq!(&*op.message, "boom");
        assert_eq!(op.code.as_deref(), Some("X1"));
        assert_eq!(op.hint.as_deref(), Some("fix it"));
    }

    #[test]
    fn lower_rejects_missing_message() {
        let err = lower(r#"lsp[hint](code = "x")"#).unwrap_err();
        assert_eq!(err[0].code, "lsp/missing-message");
    }

    #[test]
    fn lower_rejects_unknown_severity() {
        let err = lower(r#"lsp[bogus](message = "x")"#).unwrap_err();
        assert_eq!(err[0].code, "lsp/unknown-severity");
    }

    #[test]
    fn lower_rejects_missing_severity() {
        let err = lower(r#"lsp(message = "x")"#).unwrap_err();
        assert_eq!(err[0].code, "lsp/missing-severity");
    }

    #[test]
    fn lower_rejects_unknown_kwarg() {
        let err = lower(r#"lsp[warn](message = "x", oops = "y")"#).unwrap_err();
        assert_eq!(err[0].code, "lsp/unknown-kwarg");
    }

    #[test]
    fn lower_rejects_unquoted_value() {
        let err = lower(r#"lsp[warn](message = bare)"#).unwrap_err();
        assert_eq!(err[0].code, "lsp/bad-kwarg");
    }

    #[tokio::test]
    async fn pipe_emits_diag_effect_per_cursor() {
        let (batcher, buf) = LspDiagBatcher::buffer();
        let rt = RtCtxBuilder::new().register::<LspDiagEffect, _>(batcher).build();
        let op = LspDiagOp {
            severity: LspSeverity::Warning,
            message:  Arc::from("nope"),
            code:     Some(Arc::from("X1")),
            hint:     None,
        };
        let mut c = Cursor::new(Arc::from(b"hello world".as_slice()));
        c.byte_range = 6..11;
        c.fs = Some(Arc::<std::path::Path>::from(std::path::PathBuf::from("a.rs")));
        let out = op.pipe(&rt, Arc::from(vec![c])).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].byte_range, 6..11);
        let diags = buf.lock().unwrap().clone();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, LspSeverity::Warning);
        assert_eq!(&*diags[0].message, "nope");
        assert_eq!(diags[0].byte_range, 6..11);
    }

    #[test]
    fn split_commas_respects_strings() {
        let parts = split_top_level_commas(r#"a = "b, c", d = "e""#);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].trim(), r#"a = "b, c""#);
        assert_eq!(parts[1].trim(), r#"d = "e""#);
    }
}
