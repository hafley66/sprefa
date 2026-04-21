//! `host_parse` — source bytes in, AST out.
//!
//! Wraps the tree-sitter-sprefa grammar with one entry point. Always
//! returns the (possibly partial) CST plus a vec of errors; tree-sitter
//! never refuses to produce a tree, so strict vs tolerant collapses to
//! "look at the error vec or don't."

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use tree_sitter::{Node, Parser, Tree, TreeCursor};

use crate::ast::{OpInvocation, ParsedSource, Pipe, PipeStepKind};
use crate::site::{ParseSeg, ParseSite};

/// Parse-time diagnostic. Always carries a `byte_range` so the LSP can
/// underline the exact span; `kind` is structured so the diagnostic
/// renderer can pick severity, code, and message template without string
/// matching.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub kind:       ParseErrorKind,
    pub byte_range: Range<usize>,
    /// Pre-rendered message for callers that don't want to format their
    /// own. The structured `kind` is authoritative.
    pub message:    Arc<str>,
}

#[derive(Debug, Clone)]
pub enum ParseErrorKind {
    /// Tree-sitter ERROR node. Couldn't parse this region as any rule.
    SyntaxError,
    /// Tree-sitter MISSING node. The grammar required a token here that
    /// wasn't in the source. `expected` is the node kind tree-sitter
    /// would have emitted (e.g. `}`, `identifier`, `>`).
    Missing { expected: Arc<str> },
}

impl ParseError {
    pub fn offset(&self) -> usize { self.byte_range.start }
}

/// Parse a .sprf source. Always returns the partial CST; `errors` is
/// empty on a clean parse.
pub fn host_parse(src: &str, file: Arc<Path>) -> (ParsedSource, Vec<ParseError>) {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_sprefa::LANGUAGE.into())
        .expect("tree-sitter-sprefa language loads");
    let tree = parser
        .parse(src, None)
        .expect("tree-sitter parser always returns a tree on str input");
    let tree = Arc::new(tree);

    let mut errors = Vec::new();
    let mut pipes  = Vec::new();
    {
        let root = tree.root_node();
        let mut walker = root.walk();
        collect_errors(&mut walker, &mut errors);
        let mut walker = root.walk();
        for (idx, child) in root.named_children(&mut walker).enumerate() {
            if child.kind() != "pipe" { continue; }
            let path = vec![ParseSeg::Child { index: idx as u16 }];
            pipes.push(lower_pipe(child, src, &file, &path, &tree));
        }
    }

    (ParsedSource { tree, pipes }, errors)
}

fn lower_pipe(
    pipe_node: Node<'_>,
    src:       &str,
    file:      &Arc<Path>,
    path:      &[ParseSeg],
    tree:      &Arc<Tree>,
) -> Pipe {
    let mut ops = Vec::new();
    let mut walker = pipe_node.walk();
    for (idx, step) in pipe_node.named_children(&mut walker).enumerate() {
        let kind = match pipe_step_kind(step.kind()) {
            Some(k) => k,
            None    => continue, // line_comment, etc.
        };
        let mut step_path: Vec<ParseSeg> = path.to_vec();
        step_path.push(ParseSeg::Child { index: idx as u16 });

        let name = match kind {
            PipeStepKind::OpInvocation => step
                .child_by_field_name("name")
                .map(|n| Arc::<str>::from(&src[n.byte_range()]))
                .unwrap_or_else(|| Arc::from("")),
            _ => Arc::from(""),
        };

        let site = ParseSite {
            file:       file.clone(),
            path:       Arc::from(step_path.into_boxed_slice()),
            byte_range: step.byte_range(),
        };
        ops.push(OpInvocation {
            kind,
            name,
            parse_site: Arc::new(site),
            tree:       tree.clone(),
        });
    }
    Pipe { ops }
}

fn pipe_step_kind(s: &str) -> Option<PipeStepKind> {
    Some(match s {
        "op_invocation"         => PipeStepKind::OpInvocation,
        "cursor_ref"            => PipeStepKind::CursorRef,
        "xref"                  => PipeStepKind::Xref,
        "capture_write"         => PipeStepKind::CaptureWrite,
        _                       => return None,
    })
}

fn collect_errors(cursor: &mut TreeCursor<'_>, out: &mut Vec<ParseError>) {
    let n = cursor.node();
    if n.is_error() {
        out.push(ParseError {
            kind:       ParseErrorKind::SyntaxError,
            byte_range: n.byte_range(),
            message:    Arc::from("syntax error"),
        });
    } else if n.is_missing() {
        let expected: Arc<str> = Arc::from(n.kind());
        out.push(ParseError {
            kind:       ParseErrorKind::Missing { expected: expected.clone() },
            byte_range: n.byte_range(),
            message:    Arc::from(format!("missing `{}`", expected)),
        });
    }
    if cursor.goto_first_child() {
        loop {
            collect_errors(cursor, out);
            if !cursor.goto_next_sibling() { break; }
        }
        cursor.goto_parent();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fake_file() -> Arc<Path> {
        Arc::from(PathBuf::from("test.sprf").as_path())
    }

    #[test]
    fn empty_input_yields_zero_pipes() {
        let (p, errs) = host_parse("", fake_file());
        assert!(p.pipes.is_empty());
        assert!(errs.is_empty());
    }

    #[test]
    fn single_op_one_pipe() {
        let (p, errs) = host_parse("foo", fake_file());
        assert!(errs.is_empty(), "unexpected errs: {errs:?}");
        assert_eq!(p.pipes.len(), 1);
        assert_eq!(p.pipes[0].ops.len(), 1);
        assert_eq!(&*p.pipes[0].ops[0].name, "foo");
        assert_eq!(p.pipes[0].ops[0].kind, PipeStepKind::OpInvocation);
    }

    #[test]
    fn chained_ops_in_one_pipe() {
        let (p, _) = host_parse("foo > bar > baz", fake_file());
        assert_eq!(p.pipes.len(), 1);
        let names: Vec<&str> = p.pipes[0].ops.iter().map(|o| &*o.name).collect();
        assert_eq!(names, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn semicolon_separates_pipes() {
        let (p, _) = host_parse("foo; bar", fake_file());
        assert_eq!(p.pipes.len(), 2);
        assert_eq!(&*p.pipes[0].ops[0].name, "foo");
        assert_eq!(&*p.pipes[1].ops[0].name, "bar");
    }

    #[test]
    fn parse_site_byte_range_resolves_to_node() {
        let src = "foo > bar";
        let (p, _) = host_parse(src, fake_file());
        let bar = &p.pipes[0].ops[1];
        let node = bar.node();
        assert_eq!(node.kind(), "op_invocation");
        assert_eq!(&src[node.byte_range()], "bar");
    }

    #[test]
    fn cursor_ref_step() {
        let (p, _) = host_parse("&.$DIR > void", fake_file());
        assert_eq!(p.pipes[0].ops.len(), 2);
        assert_eq!(p.pipes[0].ops[0].kind, PipeStepKind::CursorRef);
        assert_eq!(&*p.pipes[0].ops[1].name, "void");
    }

    #[test]
    fn capture_write_step() {
        let (p, _) = host_parse("foo > $TARGET", fake_file());
        assert_eq!(p.pipes[0].ops.len(), 2);
        assert_eq!(p.pipes[0].ops[1].kind, PipeStepKind::CaptureWrite);
    }

    #[test]
    fn xref_step_kind() {
        let (p, _) = host_parse("rule_a.$VAR", fake_file());
        assert_eq!(p.pipes[0].ops.len(), 1);
        assert_eq!(p.pipes[0].ops[0].kind, PipeStepKind::Xref);
    }

    #[test]
    fn invalid_input_returns_partial_tree_with_errors() {
        let (p, errs) = host_parse("foo > > bar", fake_file());
        assert!(!errs.is_empty());
        assert!(!p.pipes.is_empty());
    }

    #[test]
    fn syntax_error_carries_byte_range() {
        let src = "foo > > bar";
        let (_p, errs) = host_parse(src, fake_file());
        let e = errs.iter().find(|e| matches!(e.kind, ParseErrorKind::SyntaxError))
            .expect("at least one syntax error");
        assert!(e.byte_range.end > e.byte_range.start, "non-empty span: {:?}", e.byte_range);
        assert!(e.byte_range.end <= src.len());
    }

    #[test]
    fn missing_node_carries_expected_kind_and_range() {
        // Unclosed brace forces a MISSING `}` somewhere in the tree.
        let src = "foo { bar";
        let (_p, errs) = host_parse(src, fake_file());
        let m = errs.iter().find_map(|e| match &e.kind {
            ParseErrorKind::Missing { expected } => Some((expected.clone(), e.byte_range.clone())),
            _ => None,
        });
        let (expected, range) = m.expect("at least one MISSING diagnostic");
        assert_eq!(&*expected, "}");
        assert!(range.start <= src.len());
    }

    #[test]
    fn slot_paren_reachable_via_node_field() {
        let src = "rev(:main)";
        let (p, _) = host_parse(src, fake_file());
        let inv = &p.pipes[0].ops[0];
        let node = inv.node();
        let paren = node.child_by_field_name("paren").expect("paren slot");
        assert_eq!(paren.kind(), "paren_slot");
        assert_eq!(&src[paren.byte_range()], "(:main)");
    }

    /// Ops walk their slot subtrees themselves. Demonstrates the v3
    /// handoff: collect xrefs from a paren body via tree-sitter, no
    /// byte scanning.
    #[test]
    fn xref_in_paren_collected_via_node_walk() {
        let src = "foo(${classes.$NAME})";
        let (p, _) = host_parse(src, fake_file());
        let inv = &p.pipes[0].ops[0];
        let paren = inv.node().child_by_field_name("paren").unwrap();

        let mut found = Vec::new();
        collect_xrefs(paren, src, &mut found);
        assert_eq!(found.len(), 1);
        let (rule, var, full) = &found[0];
        assert_eq!(rule, "classes");
        assert_eq!(var,  "NAME");
        assert_eq!(full, "classes.$NAME");
    }

    fn collect_xrefs(node: Node<'_>, src: &str, out: &mut Vec<(String, String, String)>) {
        let mut cursor = node.walk();
        walk(&mut cursor, src, out);
    }

    fn walk(cursor: &mut TreeCursor<'_>, src: &str, out: &mut Vec<(String, String, String)>) {
        let n = cursor.node();
        if n.kind() == "xref" {
            let rule = n.child_by_field_name("rule").unwrap();
            let var  = n.child_by_field_name("var").unwrap();
            out.push((
                src[rule.byte_range()].to_string(),
                src[var.byte_range()].trim_start_matches('$').to_string(),
                src[n.byte_range()].to_string(),
            ));
        }
        if cursor.goto_first_child() {
            loop {
                walk(cursor, src, out);
                if !cursor.goto_next_sibling() { break; }
            }
            cursor.goto_parent();
        }
    }
}
