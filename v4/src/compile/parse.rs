//! `host_parse` — source bytes in, value-typed AST out.
//!
//! Wraps `tree-sitter-sprefa` with a single entry point. Always returns
//! the (possibly partial) AST plus a vec of diagnostics; tree-sitter
//! never refuses a tree, so strict vs tolerant collapses to "look at
//! the diag vec or don't."
//!
//! No tree handles, no lifetimes, no per-op typing. Slot bodies are
//! kept as raw text + span — the walker classifies and re-parses at
//! lower-time.

use std::sync::Arc;

use effect_runtime::v2::{ByteRange, Diag};
use tree_sitter::{Node, Parser, TreeCursor};

use super::ast::{DslText, OpCall, PipeAst, SlotText};

/// Parse a .sprf source. Always returns one `PipeAst` per top-level
/// `pipe` node in the source. Diagnostics carry tree-sitter ERROR /
/// MISSING node ranges; an empty diag vec means a clean parse.
pub fn host_parse(src: &str) -> (Vec<PipeAst>, Vec<Diag>) {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_sprefa::LANGUAGE.into())
        .expect("tree-sitter-sprefa language loads");
    let tree = parser
        .parse(src, None)
        .expect("tree-sitter parser always returns a tree on str input");

    let mut diags = Vec::new();
    let mut pipes = Vec::new();
    {
        let root = tree.root_node();
        let mut walker = root.walk();
        collect_diags(&mut walker, &mut diags);
        let mut walker = root.walk();
        for child in root.named_children(&mut walker) {
            if child.kind() != "pipe" {
                continue;
            }
            if let Some(p) = lower_pipe(child, src) {
                pipes.push(p);
            }
        }
    }

    (pipes, diags)
}

/// Build a `PipeAst` from a `pipe` node. Returns `None` if the pipe has
/// no recognizable steps (e.g. an entirely error region).
fn lower_pipe(pipe_node: Node<'_>, src: &str) -> Option<PipeAst> {
    let mut steps: Vec<OpCall> = Vec::new();
    let mut walker = pipe_node.walk();
    for step in pipe_node.named_children(&mut walker) {
        match step.kind() {
            "op_invocation" => {
                if let Some(call) = lower_op_invocation(step, src) {
                    steps.push(call);
                }
            }
            "dsl_body" => {
                // Naked backtick at pipe-step position: lower as `str`
                // op call with no slots, dsl set to the body.
                steps.push(OpCall {
                    name: Arc::<str>::from("str"),
                    force: false,
                    predicate: false,
                    apply: false,
                    span: node_range(step),
                    flow: None,
                    args: Vec::new(),
                    dsl: Some(dsl_text(step, src)),
                    block: None,
                });
            }
            "parenthesized" => {
                // `( a > b > c )` flattens its inner pipe steps into the
                // outer pipe at the position the group occupied.
                let inner_pipe = step.named_child(0).filter(|c| c.kind() == "pipe");
                if let Some(inner_pipe) = inner_pipe {
                    if let Some(inner) = lower_pipe(inner_pipe, src) {
                        steps.extend(inner.steps);
                    }
                }
            }
            _ => continue, // line_comment, ERROR, etc.
        }
    }
    Some(PipeAst {
        steps,
        span: node_range(pipe_node),
    })
}

fn lower_op_invocation(node: Node<'_>, src: &str) -> Option<OpCall> {
    let name_node = node.child_by_field_name("name")?;
    let name = Arc::<str>::from(&src[name_node.byte_range()]);
    let force = node.child_by_field_name("force").is_some();
    let predicate = node.child_by_field_name("predicate").is_some();
    let apply = node.child_by_field_name("apply").is_some();

    // bracket: take the first bracket_slot if any (multiple are tolerated
    // by the grammar's `repeat(field('bracket', …))`; v4 lowers the first).
    let flow = first_field(node, "bracket").map(|n| slot_text_from_delimited(n, src));

    // paren: comma-split args at top level.
    let args = node
        .child_by_field_name("paren")
        .map(|n| split_paren_args(n, src))
        .unwrap_or_default();

    let dsl = node.child_by_field_name("dsl").map(|n| dsl_text(n, src));

    let block = node
        .child_by_field_name("brace")
        .and_then(|n| lower_brace_block(n, src));

    Some(OpCall {
        name,
        force,
        predicate,
        apply,
        span: node_range(node),
        flow,
        args,
        dsl,
        block,
    })
}

/// Lower a `brace_slot` into a `PipeAst`. The grammar says `brace_slot`
/// holds an opaque `_slot_body` (not a `pipe`), so for v4 we re-parse
/// the brace body as a sprefa source fragment to get the inner pipe.
/// Single-pipe block only; fork/multi-pipe blocks are TODO.
fn lower_brace_block(brace: Node<'_>, src: &str) -> Option<PipeAst> {
    // brace_slot covers `{ ... }`. Strip the braces.
    let r = brace.byte_range();
    if r.end < r.start + 2 {
        return None;
    }
    let inner_lo = r.start + 1;
    let inner_hi = r.end - 1;
    let inner_owned = {
        // The grammar requires top-level statements to be `;`-terminated.
        // Brace bodies in source omit it; synthesize one so the inner
        // re-parse produces a clean `pipe`. The synthetic `;` sits past
        // the original body's bytes; downstream rebase still uses
        // `inner_lo` for the original body's source coords.
        let mut s = String::with_capacity((inner_hi - inner_lo) + 1);
        s.push_str(&src[inner_lo..inner_hi]);
        s.push(';');
        s
    };
    let inner: &str = &inner_owned;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_sprefa::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(inner, None)?;
    let root = tree.root_node();
    let mut walker = root.walk();
    let pipe_node = root
        .named_children(&mut walker)
        .find(|c| c.kind() == "pipe")?;

    // Re-walk steps, but rebase byte ranges into the outer source.
    let mut steps: Vec<OpCall> = Vec::new();
    let mut w2 = pipe_node.walk();
    for step in pipe_node.named_children(&mut w2) {
        match step.kind() {
            "op_invocation" => {
                if let Some(mut call) = lower_op_invocation(step, inner) {
                    rebase_op_call(&mut call, inner_lo);
                    steps.push(call);
                }
            }
            "dsl_body" => {
                let mut span = node_range(step);
                shift_range(&mut span, inner_lo);
                // Strip the leading/trailing backtick fences. dsl_body is a
                // single token covering `\`...\``; raw should be just the body.
                let r = step.byte_range();
                let body_lo = r.start.saturating_add(1);
                let body_hi = r.end.saturating_sub(1).max(body_lo);
                let mut body_span = ByteRange {
                    lo: body_lo as u32,
                    hi: body_hi as u32,
                };
                shift_range(&mut body_span, inner_lo);
                steps.push(OpCall {
                    name: Arc::<str>::from("str"),
                    force: false,
                    predicate: false,
                    apply: false,
                    span,
                    flow: None,
                    args: Vec::new(),
                    dsl: Some(DslText {
                        raw: Arc::<str>::from(&inner[body_lo..body_hi]),
                        span: body_span,
                    }),
                    block: None,
                });
            }
            "parenthesized" => {
                let inner_pipe_node = step.named_child(0).filter(|c| c.kind() == "pipe");
                if let Some(inner_pipe) = inner_pipe_node {
                    let mut wq = inner_pipe.walk();
                    for sub in inner_pipe.named_children(&mut wq) {
                        if sub.kind() == "op_invocation" {
                            if let Some(mut call) = lower_op_invocation(sub, inner) {
                                rebase_op_call(&mut call, inner_lo);
                                steps.push(call);
                            }
                        }
                    }
                }
            }
            _ => continue,
        }
    }

    let mut span = node_range(pipe_node);
    shift_range(&mut span, inner_lo);
    Some(PipeAst { steps, span })
}

fn rebase_op_call(call: &mut OpCall, offset: usize) {
    shift_range(&mut call.span, offset);
    if let Some(s) = call.flow.as_mut() {
        shift_range(&mut s.span, offset);
    }
    for a in call.args.iter_mut() {
        shift_range(&mut a.span, offset);
    }
    if let Some(d) = call.dsl.as_mut() {
        shift_range(&mut d.span, offset);
    }
    if let Some(b) = call.block.as_mut() {
        shift_range(&mut b.span, offset);
        for s in b.steps.iter_mut() {
            rebase_op_call(s, offset);
        }
    }
}

fn shift_range(r: &mut ByteRange, offset: usize) {
    r.lo = r.lo.saturating_add(offset as u32);
    r.hi = r.hi.saturating_add(offset as u32);
}

/// Build a `SlotText` from a delimited node (paren_slot / bracket_slot)
/// stripping the leading and trailing delimiter byte. Span is the inner
/// body range.
fn slot_text_from_delimited(n: Node<'_>, src: &str) -> SlotText {
    let r = n.byte_range();
    let lo = r.start.saturating_add(1);
    let hi = r.end.saturating_sub(1);
    let lo = lo.min(hi);
    let raw = &src[lo..hi];
    SlotText {
        raw: Arc::<str>::from(raw),
        span: ByteRange {
            lo: lo as u32,
            hi: hi as u32,
        },
    }
}

fn dsl_text(n: Node<'_>, src: &str) -> DslText {
    // dsl_body is a single token covering `\`...\``. Strip the fence
    // backticks so `raw` is just the body text.
    let r = n.byte_range();
    let lo = r.start.saturating_add(1);
    let hi = r.end.saturating_sub(1);
    let lo = lo.min(hi);
    DslText {
        raw: Arc::<str>::from(&src[lo..hi]),
        span: ByteRange {
            lo: lo as u32,
            hi: hi as u32,
        },
    }
}

/// Split a `paren_slot` body into top-level comma-separated args. Each
/// arg becomes a `SlotText` with a span covering its trimmed extent.
/// Nested parens / braces / brackets / backticks count as one token —
/// the named-child walk uses field-aware named_children but we go down
/// to byte-level here to handle the comma-split at top-level only.
fn split_paren_args(paren: Node<'_>, src: &str) -> Vec<SlotText> {
    let r = paren.byte_range();
    if r.end < r.start + 2 {
        return Vec::new();
    }
    let body_lo = r.start + 1;
    let body_hi = r.end - 1;
    let body = &src[body_lo..body_hi];

    let bytes = body.as_bytes();
    let mut depth_paren: i32 = 0;
    let mut depth_brace: i32 = 0;
    let mut depth_brack: i32 = 0;
    let mut in_backtick = false;
    let mut in_string = false;
    let mut splits: Vec<usize> = vec![0];
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            if c == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if in_backtick {
            if c == b'`' {
                in_backtick = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'`' => in_backtick = true,
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'{' => depth_brace += 1,
            b'}' => depth_brace -= 1,
            b'[' => depth_brack += 1,
            b']' => depth_brack -= 1,
            b',' if depth_paren == 0 && depth_brace == 0 && depth_brack == 0 => {
                splits.push(i + 1);
            }
            _ => {}
        }
        i += 1;
    }
    splits.push(bytes.len() + 1);

    let mut out = Vec::with_capacity(splits.len().saturating_sub(1));
    for w in splits.windows(2) {
        let raw_lo = w[0];
        let raw_hi = w[1].saturating_sub(1).min(bytes.len());
        // Trim ASCII whitespace from both ends.
        let mut a = raw_lo;
        let mut b = raw_hi;
        while a < b && (bytes[a] as char).is_ascii_whitespace() {
            a += 1;
        }
        while b > a && (bytes[b - 1] as char).is_ascii_whitespace() {
            b -= 1;
        }
        if a == b {
            continue;
        } // empty arg slot, skip
        let slice = &body[a..b];
        out.push(SlotText {
            raw: Arc::<str>::from(slice),
            span: ByteRange {
                lo: (body_lo + a) as u32,
                hi: (body_lo + b) as u32,
            },
        });
    }
    out
}

fn first_field<'a>(node: Node<'a>, name: &str) -> Option<Node<'a>> {
    let mut walker = node.walk();
    for child in node.children_by_field_name(name, &mut walker) {
        return Some(child);
    }
    None
}

fn node_range(n: Node<'_>) -> ByteRange {
    let r = n.byte_range();
    ByteRange {
        lo: r.start as u32,
        hi: r.end as u32,
    }
}

/// Walk the tree collecting ERROR and MISSING nodes as `Diag`s.
fn collect_diags(cursor: &mut TreeCursor<'_>, out: &mut Vec<Diag>) {
    let n = cursor.node();
    if n.is_error() {
        let r = n.byte_range();
        out.push(
            Diag::error("parse/syntax-error", "syntax error")
                .with_span(r.start as u32, r.end as u32),
        );
    } else if n.is_missing() {
        let r = n.byte_range();
        let expected = n.kind();
        out.push(
            Diag::error("parse/missing-token", format!("missing `{}`", expected))
                .with_span(r.start as u32, r.end as u32),
        );
    }
    if cursor.goto_first_child() {
        loop {
            collect_diags(cursor, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}
