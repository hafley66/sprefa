//! `sh[$POLICY](body)` — run a bash snippet, return stdout as the next
//! cursor's content.
//!
//! Surface:
//!   sh(echo $${X})                           policy = cache (default)
//!   sh[auto](git rev-parse HEAD)             always run
//!   sh[cache](oasdiff diff $${BASE} $${HEAD})  memoized in-batcher
//!   sh[approve](rm -rf $${PATH})             HITL stage + approve (TODO)
//!   sh[dry](mutate-everything)               never run, returns empty
//!
//! Body is a single positional template. Bash variable refs `$X` stay
//! literal; sprf capture reads use `$$X` or `$${X}`. The op walks the
//! paren body's raw bytes and only treats `$$` as the read sigil, so
//! the host-grammar `term_ref` interpretation of `$X` is bypassed.
//!
//! Output cursor:
//!   content    = stdout                       (rebased, captures cleared)
//!   captures   = ${EXIT}     decimal exit code as synthesized bytes
//!                ${STDERR}   raw stderr bytes (UTF-8 lossy)
//!                ${ELAPSED}  elapsed_ms as decimal
//!
//! On subprocess error: cursor is dropped, no diag yet (TODO: lsp[error]).

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use tree_sitter::Node;

use crate::_0_cursor::{Capture, CaptureKind, Cursor};
use crate::_1_op::Op;
use crate::effects::{ShEffect, ShPolicy};
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

#[derive(Debug, Clone)]
pub enum ShSeg {
    Lit(Arc<str>),
    Term(Arc<str>),
}

#[derive(Debug)]
pub struct ShOp {
    pub policy:   ShPolicy,
    pub template: Vec<ShSeg>,
}

impl Op for ShOp {
    fn name(&self) -> &'static str { "sh" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            let mut buf = String::new();
            for seg in &self.template {
                match seg {
                    ShSeg::Lit(s) => buf.push_str(s),
                    ShSeg::Term(n) => {
                        if let Some(cap) = c.capture(n) {
                            if let Ok(s) = std::str::from_utf8(cap.bytes(&c.content)) {
                                buf.push_str(s);
                            }
                        }
                    }
                }
            }
            let cmd_line: Arc<str> = Arc::from(buf);
            let eff = ShEffect {
                cmd_line,
                cwd:      None,
                policy:   self.policy,
                approval: None,
            };
            let resp = match ctx.put(eff).await {
                Ok(r)  => r,
                Err(_) => return Vec::<Cursor>::new(),
            };
            // Materialize upstream captures as Synthesized BEFORE the
            // content swap. Otherwise SpanBacked byte_ranges point at
            // pre-rebase bytes and panic on slice. Synthesized captures
            // already carry inline values and pass through unchanged.
            let preserved: Vec<Capture> = c.captures.iter().map(|cap| {
                match &cap.kind {
                    CaptureKind::Synthesized { .. } => cap.clone(),
                    CaptureKind::SpanBacked => {
                        let v = String::from_utf8_lossy(cap.bytes(&c.content)).into_owned();
                        Capture::synthesized(cap.name.clone(), Arc::from(v))
                    }
                }
            }).collect();
            let bytes = resp.stdout.clone();
            let len = bytes.len();
            let mut out = c.rebase(bytes, 0..len);
            out.captures = preserved;
            out.captures.push(Capture::synthesized(
                Arc::from("EXIT"),
                Arc::from(resp.exit_code.to_string()),
            ));
            out.captures.push(Capture::synthesized(
                Arc::from("STDERR"),
                Arc::from(String::from_utf8_lossy(&resp.stderr).into_owned()),
            ));
            out.captures.push(Capture::synthesized(
                Arc::from("ELAPSED"),
                Arc::from(resp.elapsed_ms.to_string()),
            ));
            vec![out]
        }))
    }
}

impl OpCtor for ShOp {
    const NAME: &'static str = "sh";
    const BODY_GRAMMAR: &'static str = "bash body; $$X reads sprf captures, $X stays literal";
    const DOC: &'static str = "\
`sh[policy](body)`

Run `bash -c <body>`. Body is literal bash with `$$X` or `$${X}` for
sprf capture reads. `$X` stays literal bash. Policy ∈ {auto, cache,
approve, dry}; default cache. Output cursor.content = stdout; captures
EXIT, STDERR, ELAPSED are synthesized.
";

    fn from_values(_: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        Err(vec![PatternDiagnostic {
            code:       "sh/value-path-unsupported",
            message:    "sh parses paren body via from_paren_node, not values".into(),
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
        Some(lower_sh(inv_node, src))
    }
}

fn lower_sh(
    inv_node: Node<'_>,
    src:      &[u8],
) -> Result<ShOp, Vec<PatternDiagnostic>> {
    let policy = match inv_node.child_by_field_name("bracket") {
        None => ShPolicy::Cache,
        Some(bracket) => {
            let br = bracket.byte_range();
            let bytes = if br.end < br.start + 2 { &[][..] } else { &src[br.start + 1..br.end - 1] };
            let raw = std::str::from_utf8(bytes).map_err(|_| vec![PatternDiagnostic {
                code:       "sh/bad-policy",
                message:    "sh policy is not valid UTF-8".into(),
                byte_range: bracket.byte_range(),
            }])?;
            ShPolicy::parse(raw).ok_or_else(|| vec![PatternDiagnostic {
                code:       "sh/unknown-policy",
                message:    format!(
                    "unknown sh policy `{}`; supported: auto, cache, approve, dry",
                    raw.trim(),
                ),
                byte_range: bracket.byte_range(),
            }])?
        }
    };

    let template = match inv_node.child_by_field_name("paren") {
        None => Vec::new(),
        Some(paren) => {
            let p_start = paren.start_byte();
            let p_end   = paren.end_byte();
            if p_end < p_start + 2 {
                Vec::new()
            } else {
                let body = &src[p_start + 1..p_end - 1];
                parse_sh_template(body)
            }
        }
    };

    Ok(ShOp { policy, template })
}

fn parse_sh_template(body: &[u8]) -> Vec<ShSeg> {
    let mut out: Vec<ShSeg> = Vec::new();
    let mut i = 0;
    let mut lit_start = 0;
    while i < body.len() {
        if i + 1 < body.len() && body[i] == b'$' && body[i + 1] == b'$' {
            if i > lit_start {
                push_lit(&mut out, &body[lit_start..i]);
            }
            i += 2;
            let braced = i < body.len() && body[i] == b'{';
            if braced { i += 1; }
            let name_start = i;
            while i < body.len() {
                let b = body[i];
                if b.is_ascii_alphanumeric() || b == b'_' { i += 1; } else { break; }
            }
            let name_end = i;
            if i < body.len() && body[i] == b'?' { i += 1; }
            if braced && i < body.len() && body[i] == b'}' { i += 1; }
            if let Ok(name) = std::str::from_utf8(&body[name_start..name_end]) {
                if !name.is_empty() {
                    out.push(ShSeg::Term(Arc::from(name)));
                }
            }
            lit_start = i;
        } else {
            i += 1;
        }
    }
    if lit_start < body.len() {
        push_lit(&mut out, &body[lit_start..]);
    }
    out
}

fn push_lit(out: &mut Vec<ShSeg>, bytes: &[u8]) {
    if bytes.is_empty() { return; }
    if let Ok(s) = std::str::from_utf8(bytes) {
        out.push(ShSeg::Lit(Arc::from(s)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn host_lang() -> tree_sitter::Language {
        tree_sitter_sprefa::LANGUAGE.into()
    }

    fn lower(src: &str) -> Result<ShOp, Vec<PatternDiagnostic>> {
        let mut parser = Parser::new();
        parser.set_language(&host_lang()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let inv = find_op_inv(tree.root_node()).expect("no op_invocation");
        lower_sh(inv, src.as_bytes())
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
    fn lower_default_policy_is_cache() {
        let op = lower("sh(echo hi)").unwrap();
        assert_eq!(op.policy, ShPolicy::Cache);
    }

    #[test]
    fn lower_explicit_policies() {
        for (s, p) in [
            ("sh[auto](x)",    ShPolicy::Auto),
            ("sh[cache](x)",   ShPolicy::Cache),
            ("sh[approve](x)", ShPolicy::Approve),
            ("sh[dry](x)",     ShPolicy::DryRun),
            ("sh[dry-run](x)", ShPolicy::DryRun),
        ] {
            assert_eq!(lower(s).unwrap().policy, p, "src={s}");
        }
    }

    #[test]
    fn lower_rejects_unknown_policy() {
        let err = lower("sh[bogus](x)").unwrap_err();
        assert_eq!(err[0].code, "sh/unknown-policy");
    }

    #[test]
    fn template_treats_bash_var_as_literal() {
        let op = lower("sh(echo $X)").unwrap();
        assert_eq!(op.template.len(), 1);
        match &op.template[0] {
            ShSeg::Lit(s) => assert_eq!(&**s, "echo $X"),
            _ => panic!("expected single literal segment"),
        }
    }

    #[test]
    fn template_double_dollar_is_term_read() {
        let op = lower("sh(echo $$X)").unwrap();
        assert_eq!(op.template.len(), 2);
        match &op.template[0] {
            ShSeg::Lit(s) => assert_eq!(&**s, "echo "),
            _ => panic!(),
        }
        match &op.template[1] {
            ShSeg::Term(n) => assert_eq!(&**n, "X"),
            _ => panic!(),
        }
    }

    #[test]
    fn template_braced_double_dollar() {
        let op = lower("sh(oasdiff $${BASE} $${HEAD})").unwrap();
        let names: Vec<&str> = op.template.iter().filter_map(|s| match s {
            ShSeg::Term(n) => Some(&**n),
            _ => None,
        }).collect();
        assert_eq!(names, vec!["BASE", "HEAD"]);
    }

    #[test]
    fn template_optional_marker_stripped() {
        let op = lower("sh(echo $${X?})").unwrap();
        let names: Vec<&str> = op.template.iter().filter_map(|s| match s {
            ShSeg::Term(n) => Some(&**n),
            _ => None,
        }).collect();
        assert_eq!(names, vec!["X"]);
    }
}
