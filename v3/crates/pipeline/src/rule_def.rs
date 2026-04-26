//! Rule-definition index. Walks a parsed host source for top-level
//! `rule(:name, ${X?}, ...) { body }` invocations and returns metadata
//! consumers need without re-implementing the walk in each caller.
//!
//! Two consumers today:
//!   * `server::DocSession`: rule hover / completion.
//!   * `sprefa-run` pass-1: build runtime body Pipelines + params.
//!
//! This module returns metadata only; consumers that need the body
//! Pipeline lower it themselves from `RuleDef::brace_range`.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use sprefa_parse::{host_parse_with_injections, ParseError, ParseErrorKind, ParsedSource};
use tree_sitter::Node;

use crate::op_languages;
use crate::registry::{lower_paren_slot, Registry};
use crate::value::{TermMode, Value};

/// Metadata for one `rule(:name, ${A?}, ...) { body }` definition.
#[derive(Debug, Clone)]
pub struct RuleDef {
    pub name:            Arc<str>,
    pub params:          Vec<Arc<str>>,
    /// Host-source byte range of the def's full op_invocation
    /// (`rule(:name, ...) { ... }`).
    pub byte_range:      Range<usize>,
    /// 1-based line number of the def's start.
    pub line:            usize,
    /// Number of top-level pipes inside the brace body.
    pub body_pipe_count: usize,
    /// Host-source byte range of the brace body interior (between the
    /// `{` and `}`, exclusive). `None` when the def has no brace body.
    pub brace_range:     Option<Range<usize>>,
    /// Flat list of op_invocation names appearing in the brace body.
    /// Order = body source order. Includes every `name(...)` head; the
    /// caller filters against the known-rules set to derive cross-rule
    /// edges for `rule_hash::compute`.
    pub body_op_names:   Vec<Arc<str>>,
    /// Host-source byte ranges of body op_invocations whose head name
    /// equals `name` (self-recursion sites). Forbidden in v0; consumers
    /// surface these as `ParseErrorKind::RecursionForbidden`.
    pub body_self_calls: Vec<Range<usize>>,
}

/// Walk every top-level pipe whose head op is `rule`. Returns a name →
/// metadata map. Definitions with malformed first args (non-atom) or
/// non-introducer params are skipped silently; callers that want
/// diagnostics should produce them at lower time.
pub fn collect(
    parsed:   &ParsedSource,
    src:      &[u8],
    registry: &Registry,
    file:     &Arc<Path>,
) -> HashMap<Arc<str>, RuleDef> {
    let mut out: HashMap<Arc<str>, RuleDef> = HashMap::new();
    for pipe in &parsed.pipes {
        let Some(head) = pipe.ops.first() else { continue };
        if &*head.name != "rule" { continue; }
        let node: Node<'_> = head.node();
        let Some(paren) = node.child_by_field_name("paren") else { continue };
        let mut diags = Vec::new();
        let values = lower_paren_slot(paren, src, registry, &mut diags);
        let mut iter = values.into_iter();
        let name = match iter.next() {
            Some(Value::Atom(s)) => s,
            _ => continue,
        };
        let mut params: Vec<Arc<str>> = Vec::new();
        for v in iter {
            if let Value::Term { name: pname, mode: TermMode::Unbound } = v {
                params.push(pname);
            }
        }
        let byte_range = head.parse_site.byte_range.clone();
        let line = line_at_byte(src, byte_range.start);
        let brace_range = brace_interior(node);
        let (body_pipe_count, body_op_names, body_self_calls) = brace_range
            .as_ref()
            .map(|r| walk_body(src, r.clone(), file, &name))
            .unwrap_or((0, Vec::new(), Vec::new()));
        out.insert(
            name.clone(),
            RuleDef {
                name, params, byte_range, line,
                body_pipe_count, brace_range, body_op_names, body_self_calls,
            },
        );
    }
    out
}

/// 1-based line number for `offset` in `src`.
pub fn line_at_byte(src: &[u8], offset: usize) -> usize {
    let upto = offset.min(src.len());
    1 + src[..upto].iter().filter(|b| **b == b'\n').count()
}

fn brace_interior(inv: Node<'_>) -> Option<Range<usize>> {
    let brace = inv.child_by_field_name("brace")?;
    let start = brace.start_byte() + 1;
    let end = brace.end_byte().saturating_sub(1);
    if start > end { return None; }
    Some(start..end)
}

fn walk_body(
    src:       &[u8],
    range:     Range<usize>,
    file:      &Arc<Path>,
    rule_name: &Arc<str>,
) -> (usize, Vec<Arc<str>>, Vec<Range<usize>>) {
    if range.end > src.len() { return (0, Vec::new(), Vec::new()); }
    let body_offset = range.start;
    let body = match std::str::from_utf8(&src[range]) {
        Ok(s) => s,
        Err(_) => return (0, Vec::new(), Vec::new()),
    };
    let (sub, _errs) = host_parse_with_injections(
        body,
        file.clone(),
        &op_languages::language_of,
    );
    let mut names: Vec<Arc<str>> = Vec::new();
    let mut self_calls: Vec<Range<usize>> = Vec::new();
    for pipe in &sub.pipes {
        for inv in &pipe.ops {
            names.push(inv.name.clone());
            if &inv.name == rule_name {
                let r = &inv.parse_site.byte_range;
                self_calls.push((r.start + body_offset)..(r.end + body_offset));
            }
        }
    }
    (sub.pipes.len(), names, self_calls)
}

/// Build `ParseError`s for every self-recursion site in `defs`. Empty
/// vec when no rule body calls itself. Consumers (sprefa-run pass-1,
/// `DocSession`) append these onto their respective error lists so the
/// LSP underlines the offending invocation and the CLI prints a parse
/// diagnostic before lowering.
pub fn self_recursion_errors(
    defs: &HashMap<Arc<str>, RuleDef>,
) -> Vec<ParseError> {
    let mut out = Vec::new();
    for def in defs.values() {
        for r in &def.body_self_calls {
            let msg = format!("rule {:?} calls itself; self-recursion is forbidden", def.name);
            out.push(ParseError {
                kind:       ParseErrorKind::RecursionForbidden { rule: def.name.clone() },
                byte_range: r.clone(),
                message:    Arc::from(msg),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprefa_parse::host_parse_with_injections;
    use crate::op_languages;
    use crate::registry::Registry;
    use std::path::Path;

    fn collect_for(src: &str) -> HashMap<Arc<str>, RuleDef> {
        let registry = Registry::with_stdlib();
        let file: Arc<Path> = Arc::from(Path::new("<test>"));
        let (parsed, _errs) = host_parse_with_injections(
            src, file.clone(), &op_languages::language_of,
        );
        collect(&parsed, src.as_bytes(), &registry, &file)
    }

    #[test]
    fn self_recursion_site_is_recorded_and_surfaced_as_parse_error() {
        let src = "rule(:foo, ${X?}) {\n  > foo(${X?})\n}\n";
        let defs = collect_for(src);
        let foo = defs.get("foo").expect("rule foo collected");
        // The body contains exactly one self-call.
        assert_eq!(foo.body_self_calls.len(), 1);
        let r = &foo.body_self_calls[0];
        // Range maps to the offending invocation in host coords.
        assert_eq!(&src[r.clone()], "foo(${X?})");

        let errs = self_recursion_errors(&defs);
        assert_eq!(errs.len(), 1);
        match &errs[0].kind {
            ParseErrorKind::RecursionForbidden { rule } => assert_eq!(&**rule, "foo"),
            other => panic!("unexpected kind: {other:?}"),
        }
        assert_eq!(errs[0].byte_range, *r);
    }

    #[test]
    fn non_recursive_rule_emits_no_self_recursion_error() {
        let src = "rule(:bar, ${X?}) {\n  > tag(:t, ${X?})\n}\n";
        let defs = collect_for(src);
        let bar = defs.get("bar").expect("rule bar collected");
        assert!(bar.body_self_calls.is_empty());
        assert!(self_recursion_errors(&defs).is_empty());
    }
}
