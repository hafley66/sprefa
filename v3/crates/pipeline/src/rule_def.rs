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

use sprefa_parse::{host_parse_with_injections, ParsedSource};
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
        let body_pipe_count = brace_range
            .as_ref()
            .map(|r| count_pipes(src, r.clone(), file))
            .unwrap_or(0);
        out.insert(
            name.clone(),
            RuleDef { name, params, byte_range, line, body_pipe_count, brace_range },
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

fn count_pipes(src: &[u8], range: Range<usize>, file: &Arc<Path>) -> usize {
    if range.end > src.len() { return 0; }
    let body = match std::str::from_utf8(&src[range]) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let (sub, _errs) = host_parse_with_injections(
        body,
        file.clone(),
        &op_languages::language_of,
    );
    sub.pipes.len()
}
