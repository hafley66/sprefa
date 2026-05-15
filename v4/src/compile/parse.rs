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
    // Source preprocess passes.
    //   1) `"…"` → `` `…` `` so test fixtures that use double-quoted
    //      strings still parse (the v4 grammar only has backticks).
    //   2) parenless `where EXPR` → `where(EXPR)`, where EXPR runs to
    //      the next top-level `>`, `;`, or `}`.
    // Each pass returns Some only if it rewrote, so we keep the
    // original slice when no transformation applies.
    let q = rewrite_quote_strings(src);
    let q_src: &str = match &q {
        Some(s) => s.as_str(),
        None => src,
    };
    let rewritten = rewrite_where_sugar(q_src).or(q);
    let src_view: &str = match &rewritten {
        Some(s) => s.as_str(),
        None => src,
    };
    // Tolerate a trailing top-level statement without `;`. Tree-sitter
    // emits a `missing-token` diag for an absent terminator; for a
    // *trailing* one that's friction we don't want at every call site
    // (CLI fragments, examples, tests). Synthesize the `;` so the tree
    // parses cleanly. Cross-statement omissions still surface.
    let synthesized = needs_trailing_semi(src_view);
    let owned_src: String;
    let parse_src: &str = if synthesized {
        owned_src = format!("{src_view};");
        &owned_src
    } else {
        src_view
    };
    let tree = parser
        .parse(parse_src, None)
        .expect("tree-sitter parser always returns a tree on str input");
    // Source range for downstream consumers stays the original src; the
    // synthesized `;` lives past the original end. Spans only point to
    // bytes that exist in `src`, so this is safe.
    let src = parse_src;

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

/// Rewrite `"…"` literals into backtick literals so the v4 grammar
/// (which has no double-quoted strings) accepts test fixtures lifted
/// from older sprf surfaces. Escaped quotes (`\"`) inside the literal
/// are preserved verbatim — they become regular backslash sequences
/// the dsl_body lexer keeps in tact.
fn rewrite_quote_strings(src: &str) -> Option<String> {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut rewrote = false;
    // Track outer-string mode so a `"` inside `` `…` `` doesn't get
    // rewritten; also skip `#`-line comments so backticks inside docs
    // don't toggle the tick state.
    let mut tick_depth = 0i32;
    let mut in_comment = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_comment {
            out.push(b as char);
            if b == b'\n' {
                in_comment = false;
            }
            i += 1;
            continue;
        }
        if tick_depth == 0 && b == b'#' {
            in_comment = true;
            out.push(b as char);
            i += 1;
            continue;
        }
        if b == b'`' {
            tick_depth = if tick_depth == 0 { 1 } else { 0 };
            out.push(b as char);
            i += 1;
            continue;
        }
        if tick_depth == 0 && b == b'"' {
            // Find the matching `"`. Tolerate backslash escapes.
            let mut j = i + 1;
            while j < bytes.len() {
                if bytes[j] == b'\\' && j + 1 < bytes.len() {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'"' {
                    break;
                }
                j += 1;
            }
            if j < bytes.len() {
                out.push('`');
                out.push_str(&src[i + 1..j]);
                out.push('`');
                i = j + 1;
                rewrote = true;
                continue;
            }
        }
        out.push(b as char);
        i += 1;
    }
    if rewrote {
        Some(out)
    } else {
        None
    }
}

/// Pre-process a sprefa source to rewrite the parenless `where EXPR`
/// predicate sugar into `where(EXPR)`. Returns `Some(new_src)` only if
/// at least one occurrence was rewritten; otherwise the caller can
/// keep the original byte slice unchanged.
fn rewrite_where_sugar(src: &str) -> Option<String> {
    let bytes = src.as_bytes();
    let needle = b"where";
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut rewrote = false;
    // Track whether we're inside a string / backtick / line comment
    // so we don't rewrite `where` inside literals or `#` comments.
    let mut quote: Option<u8> = None;
    let mut in_comment = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_comment {
            out.push(b as char);
            if b == b'\n' {
                in_comment = false;
            }
            i += 1;
            continue;
        }
        if let Some(q) = quote {
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i] as char);
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if b == q {
                quote = None;
            }
            out.push(b as char);
            i += 1;
            continue;
        }
        if b == b'#' {
            in_comment = true;
            out.push(b as char);
            i += 1;
            continue;
        }
        if b == b'`' || b == b'"' || b == b'\'' {
            quote = Some(b);
            out.push(b as char);
            i += 1;
            continue;
        }
        // Match `where` as a standalone word.
        if b == b'w' && bytes.get(i..i + needle.len()) == Some(needle) {
            let prev = if i == 0 { b' ' } else { bytes[i - 1] };
            let next = bytes.get(i + needle.len()).copied().unwrap_or(b' ');
            let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
            if !is_word(prev) && !is_word(next) {
                // Look at the byte after `where` (after whitespace) to
                // see if a call-form delimiter is already present.
                // `(` = paren-arg form. `` ` `` = backtick DSL body form
                // (canonical). Both skip the sugar rewrite.
                let mut j = i + needle.len();
                while j < bytes.len() && bytes[j] != b'\n' && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] != b'(' && bytes[j] != b'`' {
                    // Locate predicate end at top-level `>`, `;`, `}`
                    // or end-of-source.
                    let start = j;
                    let end = find_predicate_end(bytes, start);
                    let expr = src[start..end].trim_end();
                    if !expr.is_empty() {
                        out.push_str("where(");
                        out.push_str(expr);
                        out.push(')');
                        rewrote = true;
                        i = start + (end - start);
                        // Preserve trailing whitespace we skipped past
                        // the predicate so column offsets downstream
                        // don't shift.
                        let consumed = i - (i - (end - expr.len()));
                        let _ = consumed;
                        continue;
                    }
                }
            }
        }
        out.push(b as char);
        i += 1;
    }
    if rewrote {
        Some(out)
    } else {
        None
    }
}

/// Find the byte offset where a parenless `where` predicate ends.
/// Stops at top-level `>`, `;`, `}`, or end-of-source. Backticks /
/// quotes are tracked so a `>` inside a literal doesn't terminate.
fn find_predicate_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    let mut depth: i32 = 0;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == b'\\' && i + 1 < bytes.len() {
                i += 2;
                continue;
            }
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'`' | b'"' | b'\'' => {
                quote = Some(b);
                i += 1;
            }
            b'(' | b'[' | b'{' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' => {
                depth -= 1;
                i += 1;
            }
            b'}' if depth == 0 => break,
            b'}' => {
                depth -= 1;
                i += 1;
            }
            b'>' | b';' if depth == 0 => break,
            _ => i += 1,
        }
    }
    i
}

/// Does the source already end with a `;` after stripping whitespace
/// and `//` line comments? Used at the top of `host_parse` to decide
/// whether to append a synthetic terminator.
fn needs_trailing_semi(src: &str) -> bool {
    // Strip from the right past whitespace and full-line `//` comments.
    let mut bytes = src.as_bytes();
    loop {
        // trim trailing whitespace
        while let Some(&b) = bytes.last() {
            if b.is_ascii_whitespace() {
                bytes = &bytes[..bytes.len() - 1];
            } else {
                break;
            }
        }
        // strip the final line if it's a `//` comment
        if let Some(nl) = bytes.iter().rposition(|&b| b == b'\n') {
            let line = &bytes[nl + 1..];
            if line.trim_ascii_start().starts_with(b"//") {
                bytes = &bytes[..nl];
                continue;
            }
        } else if bytes.trim_ascii_start().starts_with(b"//") {
            return false;
        }
        break;
    }
    !bytes.is_empty() && *bytes.last().unwrap() != b';'
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
