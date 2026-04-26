//! `str(...)` — definite (bound-only) string template op.
//!
//! Paren body is the DSL itself, NO quoting. `str(hello ${X} world)`
//! tokenizes as `Lit("hello ") + Term("X") + Lit(" world")`. Bare
//! identifiers, punctuation, whitespace, atom-looking sequences — all
//! literal bytes. The only structural carveouts are `${NAME}` /
//! `${NAME?}` (carveout_expr) and bare `$NAME` / `$NAME?` (term_ref).
//!
//! Polarity inversion vs `re` / `glob` / `comment`: those ops *bind*
//! terms; `str` is a pure *consumer*. Every term ref inside `str(...)`
//! must be `TermMode::Read`. `${X?}` introducers and bare `$X?` are
//! rejected at lower time with `term/unbound-not-allowed-in-str`.
//!
//! Lowering takes the host CST `op_invocation` node directly (via
//! `OpCtor::from_paren_node`) and slices the paren body's literal-byte
//! gaps around the term-ref / carveout child ranges. This bypasses
//! `lower_paren_slot` and its bare-ident diagnostics.
//!
//! Runtime substitutes Read terms from `cursor.captures` and stashes
//! the assembled bytes into `StrValue` for downstream source ops.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use tree_sitter::Node;

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::{TermMode, Value};

/// One element of a str template. `Lit` is verbatim bytes; `Term` is
/// the name of a Read-mode capture to be substituted at runtime.
#[derive(Debug, Clone)]
pub enum StrSeg {
    Lit(Arc<str>),
    Term(Arc<str>),
}

/// Compiled call-site for `str(...)`. The template is a flat sequence
/// of literal/term segments produced by the lowerer.
#[derive(Debug)]
pub struct StrOp {
    pub template: Vec<StrSeg>,
}

impl StrOp {
    pub fn new(body: impl Into<Arc<str>>) -> Self {
        Self {
            template: vec![StrSeg::Lit(body.into())],
        }
    }
}

/// Typed slot key for the value produced by `str(...)`. Downstream ops
/// read it via `cursor.get_slot::<StrValue>()`.
#[derive(Clone)]
pub struct StrValue(pub Arc<str>);

impl OpCtor for StrOp {
    const NAME: &'static str = "str";
    const BODY_GRAMMAR: &'static str = "literal bytes + ${NAME} reads";
    const DOC:  &'static str = "\
**str**(_dsl-body_)

Definite (bound-only) string template. Paren body is a raw DSL: literal
bytes between `${NAME}` carveouts. No quoting. `str(hello ${X} world)`
yields `\"hello <bound-X> world\"`. `${NAME?}` introducers are rejected
— str is a *consumer* of bindings, never a producer (inverse polarity
of `re` / `glob` / `comment`).
";

    fn from_values(_values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        // Defensive: the registry routes str through `from_paren_node`,
        // not the value path. If a caller bypasses the node path and
        // hits this, return an empty literal so static-known callers
        // (StrOp::new) still compose. The lowerer always uses the node
        // path.
        Ok(StrOp { template: Vec::new() })
    }

    fn from_paren_node(
        inv_node: Node<'_>,
        src:      &[u8],
    ) -> Option<Result<Self, Vec<PatternDiagnostic>>> {
        Some(lower_str_paren(inv_node, src))
    }
}

impl Op for StrOp {
    fn name(&self) -> &'static str { "str" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            let mut buf = String::new();
            for seg in &self.template {
                match seg {
                    StrSeg::Lit(s) => buf.push_str(s),
                    StrSeg::Term(name) => {
                        if let Some(cap) = c.capture(name) {
                            if let Ok(s) = std::str::from_utf8(cap.bytes(&c.content)) {
                                buf.push_str(s);
                            }
                        }
                    }
                }
            }
            let mut out = c;
            out.put_slot(StrValue(Arc::from(buf)));
            vec![out]
        }))
    }
}

// ---------------------------------------------------------------------------
// CST → template lowering
// ---------------------------------------------------------------------------

/// Walk the host CST `op_invocation` paren slot and slice it into a
/// template of literal-byte runs alternating with Read term refs.
/// Bare-ident / punctuation / whitespace are all literal bytes.
fn lower_str_paren(
    inv_node: Node<'_>,
    src:      &[u8],
) -> Result<StrOp, Vec<PatternDiagnostic>> {
    let Some(paren) = inv_node.child_by_field_name("paren") else {
        return Ok(StrOp { template: Vec::new() });
    };
    // Body byte range: skip leading `(` and trailing `)`.
    let p_start = paren.start_byte();
    let p_end = paren.end_byte();
    if p_end < p_start + 2 {
        return Ok(StrOp { template: Vec::new() });
    }
    let body_start = p_start + 1;
    let body_end = p_end - 1;

    // Collect term-ref / carveout-expr child ranges in source order.
    // These are the cuts; everything between is literal bytes.
    let mut cuts: Vec<TermCut> = Vec::new();
    let mut errors: Vec<PatternDiagnostic> = Vec::new();
    let mut walk = paren.walk();
    for child in paren.named_children(&mut walk) {
        match child.kind() {
            "term_ref" => {
                let (name, mode) = parse_term_ref(child, src);
                if let Err(d) = check_read(&name, mode, child.byte_range()) {
                    errors.push(d);
                    continue;
                }
                cuts.push(TermCut {
                    name,
                    range: child.byte_range(),
                });
            }
            "carveout_expr" => {
                let Some(body) = child.child_by_field_name("body") else {
                    continue;
                };
                match body.kind() {
                    // `${X?}` — body is the unbound-shorthand token aliased
                    // as term_ref. Always Unbound by construction; reject.
                    "term_ref" => {
                        let (name, mode) = parse_term_ref(body, src);
                        if let Err(d) = check_read(&name, mode, child.byte_range()) {
                            errors.push(d);
                            continue;
                        }
                        cuts.push(TermCut {
                            name,
                            range: child.byte_range(),
                        });
                    }
                    // `${X}` — body is a pipe with a single bare-ident
                    // op_invocation (term-shorthand Read).
                    "pipe" => {
                        if let Some(name) = single_bare_ident_pipe(body, src) {
                            cuts.push(TermCut {
                                name,
                                range: child.byte_range(),
                            });
                        } else {
                            errors.push(PatternDiagnostic {
                                code:    "str/no-pipe-carveout",
                                message: "str body accepts `${NAME}` reads only; \
                                          sub-pipe carveouts (`${ op() }`) not allowed"
                                    .into(),
                                byte_range: child.byte_range(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                // Any other named child is treated as literal bytes —
                // its range will be covered by the gap between cuts.
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    cuts.sort_by_key(|c| c.range.start);

    // Build template: alternate gap literals and term cuts.
    let mut template: Vec<StrSeg> = Vec::new();
    let mut cursor = body_start;
    for cut in cuts {
        if cut.range.start > cursor {
            push_lit(&mut template, &src[cursor..cut.range.start]);
        }
        template.push(StrSeg::Term(cut.name));
        cursor = cut.range.end;
    }
    if cursor < body_end {
        push_lit(&mut template, &src[cursor..body_end]);
    }

    Ok(StrOp { template })
}

fn push_lit(template: &mut Vec<StrSeg>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        template.push(StrSeg::Lit(Arc::from(s)));
    }
}

struct TermCut {
    name:  Arc<str>,
    range: std::ops::Range<usize>,
}

/// `${X}` carveout body parses as a pipe with one bare-ident op_invocation.
/// Recognize that shape and return the ident as a Read-mode capture name.
fn single_bare_ident_pipe(pipe: Node<'_>, src: &[u8]) -> Option<Arc<str>> {
    let mut walk = pipe.walk();
    let steps: Vec<Node<'_>> = pipe
        .named_children(&mut walk)
        .filter(|n| n.kind() == "op_invocation")
        .collect();
    if steps.len() != 1 {
        return None;
    }
    let inv = steps[0];
    let has_paren = inv.child_by_field_name("paren").is_some();
    let has_brace = inv.child_by_field_name("brace").is_some();
    let has_bracket = inv.child_by_field_name("bracket").is_some();
    if has_paren || has_brace || has_bracket {
        return None;
    }
    let name_node = inv.child_by_field_name("name")?;
    let text = std::str::from_utf8(&src[name_node.byte_range()]).ok()?;
    Some(Arc::from(text))
}

fn parse_term_ref(node: Node<'_>, src: &[u8]) -> (Arc<str>, TermMode) {
    let raw = std::str::from_utf8(&src[node.byte_range()]).unwrap_or("");
    let stripped = raw.trim_start_matches(['$', '{']).trim_end_matches('}');
    if let Some(name) = stripped.strip_suffix('?') {
        (Arc::from(name), TermMode::Unbound)
    } else {
        (Arc::from(stripped), TermMode::Read)
    }
}

fn check_read(
    name:  &Arc<str>,
    mode:  TermMode,
    range: std::ops::Range<usize>,
) -> Result<(), PatternDiagnostic> {
    match mode {
        TermMode::Read => Ok(()),
        TermMode::Unbound => Err(PatternDiagnostic {
            code:    "term/unbound-not-allowed-in-str",
            message: format!(
                "`${{{name}?}}` introducer not allowed in str(); \
                 str reads bound terms only — use re/glob/comment to introduce"
            ),
            byte_range: range,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_cursor::Capture;
    use tree_sitter::Parser;

    fn host_lang() -> tree_sitter::Language {
        tree_sitter_sprefa::LANGUAGE.into()
    }

    fn lower(src: &str) -> Result<StrOp, Vec<PatternDiagnostic>> {
        let mut parser = Parser::new();
        parser.set_language(&host_lang()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        // Find the first op_invocation under the root.
        let root = tree.root_node();
        let inv = find_op_invocation(root).expect("no op_invocation in source");
        lower_str_paren(inv, src.as_bytes())
    }

    fn find_op_invocation(n: Node<'_>) -> Option<Node<'_>> {
        if n.kind() == "op_invocation" {
            return Some(n);
        }
        let mut walk = n.walk();
        for ch in n.named_children(&mut walk) {
            if let Some(v) = find_op_invocation(ch) {
                return Some(v);
            }
        }
        None
    }

    #[tokio::test]
    async fn pipe_stores_lit_only_template() {
        let ctx = RtCtx::default();
        let c = Cursor::new(Arc::from(b"x".as_slice()));
        let op = StrOp {
            template: vec![StrSeg::Lit(Arc::from("hello world"))],
        };
        let out_batch = op.pipe(&ctx, Arc::from(vec![c])).await;
        let v = out_batch[0].get_slot::<StrValue>().unwrap();
        assert_eq!(&*v.0, "hello world");
    }

    #[test]
    fn lower_literal_only() {
        let op = lower("str(hello world)").unwrap();
        assert_eq!(op.template.len(), 1);
        match &op.template[0] {
            StrSeg::Lit(s) => assert_eq!(&**s, "hello world"),
            _ => panic!("expected lit"),
        }
    }

    #[test]
    fn lower_with_carveout_term() {
        let op = lower("str(hello ${X} world)").unwrap();
        // Lit("hello ") + Term("X") + Lit(" world")
        assert_eq!(op.template.len(), 3);
        match &op.template[0] {
            StrSeg::Lit(s) => assert_eq!(&**s, "hello "),
            _ => panic!(),
        }
        match &op.template[1] {
            StrSeg::Term(n) => assert_eq!(&**n, "X"),
            _ => panic!(),
        }
        match &op.template[2] {
            StrSeg::Lit(s) => assert_eq!(&**s, " world"),
            _ => panic!(),
        }
    }

    #[test]
    fn lower_rejects_unbound_term() {
        let err = lower("str(hello ${X?})").unwrap_err();
        assert_eq!(err[0].code, "term/unbound-not-allowed-in-str");
    }

    #[test]
    fn lower_empty_body() {
        let op = lower("str()").unwrap();
        assert!(op.template.is_empty());
    }

    #[tokio::test]
    async fn substitution_runtime() {
        let ctx = RtCtx::default();
        let op = lower("str(out/${P}/file.bin)").unwrap();
        let content: Arc<[u8]> = Arc::from(b"PATH=alice".as_slice());
        let mut c = Cursor::new(content);
        c.captures.push(Capture::span_backed(Arc::from("P"), 5..10));
        let out_batch = op.pipe(&ctx, Arc::from(vec![c])).await;
        let v = out_batch[0].get_slot::<StrValue>().unwrap();
        assert_eq!(&*v.0, "out/alice/file.bin");
    }
}
