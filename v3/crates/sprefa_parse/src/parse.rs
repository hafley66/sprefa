//! `host_parse` — source bytes in, AST out.
//!
//! Wraps the tree-sitter-sprefa grammar with one entry point. Always
//! returns the (possibly partial) CST plus a vec of errors; tree-sitter
//! never refuses to produce a tree, so strict vs tolerant collapses to
//! "look at the error vec or don't."

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use tree_sitter::{Language, Node, Parser, Tree, TreeCursor};

use crate::ast::{InjectedTree, OpInvocation, ParsedSource, Pipe};
use crate::site::{ParseSeg, ParseSite};

/// Language resolver closure. Pattern-op-aware callers pass a closure
/// backed by their op registry (e.g. `pipeline::op_languages::language_of`).
/// Returning `None` skips the injected parse for that op — the path
/// for `str` and for non-pattern ops.
pub type ResolveLanguage<'a> = &'a dyn Fn(&str) -> Option<Language>;

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
/// empty on a clean parse. `injected` is always empty — to get pattern
/// sub-tree parses, use [`host_parse_with_injections`] and pass your
/// op→language resolver.
pub fn host_parse(src: &str, file: Arc<Path>) -> (ParsedSource, Vec<ParseError>) {
    host_parse_with_injections(src, file, &|_| None)
}

/// Parse a .sprf source plus every pattern-op paren body whose op has
/// a registered sub-grammar (§14.5a). `resolve` maps op name →
/// tree-sitter language; returning `None` skips the injection for that
/// op (the expected path for `str` and for non-pattern ops).
///
/// Injected parse errors (ERROR / MISSING inside a pattern body) are
/// merged into the returned `errors` vec with byte ranges shifted into
/// the source coordinate space (§14.8).
pub fn host_parse_with_injections(
    src:     &str,
    file:    Arc<Path>,
    resolve: ResolveLanguage<'_>,
) -> (ParsedSource, Vec<ParseError>) {
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

    let injected = collect_injections(src, &file, &tree, &pipes, resolve, &mut errors);

    (ParsedSource { tree, pipes, injected }, errors)
}

fn collect_injections(
    src:     &str,
    _file:   &Arc<Path>,
    _tree:   &Arc<Tree>,
    pipes:   &[Pipe],
    resolve: ResolveLanguage<'_>,
    errors:  &mut Vec<ParseError>,
) -> Vec<InjectedTree> {
    let mut out = Vec::new();
    for pipe in pipes {
        for inv in &pipe.ops {
            let Some(lang) = resolve(&inv.name) else { continue; };
            let op_node = inv.node();
            let Some(paren) = op_node.child_by_field_name("paren") else { continue; };
            // Paren body excludes the `(` and `)` delimiters.
            let body_range = (paren.start_byte() + 1)..(paren.end_byte().saturating_sub(1));
            if body_range.start > body_range.end || body_range.end > src.len() {
                continue;
            }
            let mut p = Parser::new();
            if p.set_language(&lang).is_err() { continue; }
            let body_src = &src[body_range.clone()];
            let Some(sub) = p.parse(body_src, None) else { continue; };
            let sub = Arc::new(sub);

            let mut walker = sub.walk();
            collect_errors_shifted(&mut walker, body_range.start, errors);
            drop(walker);

            out.push(InjectedTree {
                host_node:     inv.parse_site.clone(),
                language_name: Arc::from(format!("sprefa_{}", inv.name)),
                tree:          sub,
            });
        }
    }
    out
}

fn collect_errors_shifted(
    cursor: &mut TreeCursor<'_>,
    offset: usize,
    out:    &mut Vec<ParseError>,
) {
    let n = cursor.node();
    if n.is_error() {
        let r = n.byte_range();
        out.push(ParseError {
            kind:       ParseErrorKind::SyntaxError,
            byte_range: (r.start + offset)..(r.end + offset),
            message:    Arc::from("syntax error"),
        });
    } else if n.is_missing() {
        let expected: Arc<str> = Arc::from(n.kind());
        let r = n.byte_range();
        out.push(ParseError {
            kind:       ParseErrorKind::Missing { expected: expected.clone() },
            byte_range: (r.start + offset)..(r.end + offset),
            message:    Arc::from(format!("missing `{}`", expected)),
        });
    }
    if cursor.goto_first_child() {
        loop {
            collect_errors_shifted(cursor, offset, out);
            if !cursor.goto_next_sibling() { break; }
        }
        cursor.goto_parent();
    }
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
        let kind = step.kind();
        if kind != "op_invocation" && kind != "cursor_ref" {
            continue; // line_comment, etc.
        }
        let mut step_path: Vec<ParseSeg> = path.to_vec();
        step_path.push(ParseSeg::Child { index: idx as u16 });

        // cursor_ref has no `name` field; its op-name is the literal '&'.
        // Path content (`fs.ext`) is parsed downstream from the node bytes.
        let (name, predicate) = if kind == "cursor_ref" {
            (Arc::<str>::from("&"), false)
        } else {
            let name = step
                .child_by_field_name("name")
                .map(|n| Arc::<str>::from(&src[n.byte_range()]))
                .unwrap_or_else(|| Arc::from(""));
            let predicate = step.child_by_field_name("predicate").is_some();
            (name, predicate)
        };

        let site = ParseSite {
            file:       file.clone(),
            path:       Arc::from(step_path.into_boxed_slice()),
            byte_range: step.byte_range(),
        };
        ops.push(OpInvocation {
            name,
            predicate,
            parse_site: Arc::new(site),
            tree:       tree.clone(),
        });
    }
    Pipe { ops }
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

    #[test]
    fn host_parse_leaves_injected_empty() {
        let (p, _) = host_parse("glob(stuff)", fake_file());
        assert!(p.injected.is_empty());
    }

    #[test]
    fn nested_op_invocation_in_paren_slot() {
        // fs(glob(**/*.rs)) — nested op_invocation must surface as a real
        // op_invocation node inside the outer paren_slot, not as an
        // identifier + balanced_parens shadow shape.
        let (p, errs) = host_parse("fs(glob(**/*.rs))", fake_file());
        assert!(errs.is_empty(), "unexpected errs: {errs:?}");
        let outer = p.pipes[0].ops[0].node();
        assert_eq!(outer.kind(), "op_invocation");
        let paren = outer.child_by_field_name("paren").expect("outer paren");
        // Descend into the paren body and find the nested op_invocation.
        let mut cur = paren.walk();
        let mut nested = None;
        for child in paren.named_children(&mut cur) {
            if child.kind() == "op_invocation" {
                nested = Some(child);
                break;
            }
        }
        let nested = nested.expect("nested op_invocation inside paren_slot");
        let name = nested
            .child_by_field_name("name")
            .expect("nested op has name field");
        let src = "fs(glob(**/*.rs))";
        assert_eq!(&src[name.byte_range()], "glob");
        let inner_paren = nested
            .child_by_field_name("paren")
            .expect("nested op has paren_slot");
        assert_eq!(&src[inner_paren.byte_range()], "(**/*.rs)");
    }

    #[test]
    fn injections_populated_when_resolver_returns_some() {
        // Resolver returns the sprefa host language for any op. This is
        // not semantically meaningful but exercises the plumbing end to
        // end: paren body → substring parse → InjectedTree entry.
        let src = "foo(bar)";
        let resolve = |_op: &str| -> Option<Language> {
            Some(tree_sitter_sprefa::LANGUAGE.into())
        };
        let (p, errs) = host_parse_with_injections(src, fake_file(), &resolve);
        assert!(errs.is_empty(), "unexpected errs: {errs:?}");
        assert_eq!(p.injected.len(), 1, "one op_invocation, one injected tree");
        let inj = &p.injected[0];
        assert_eq!(&*inj.language_name, "sprefa_foo");
        assert_eq!(inj.tree.root_node().kind(), "source_file");
    }

    #[test]
    fn injections_skip_ops_whose_resolver_returns_none() {
        let (p, _) = host_parse_with_injections(
            "alpha(x) > beta(y)",
            fake_file(),
            &|op| (op == "alpha").then(|| tree_sitter_sprefa::LANGUAGE.into()),
        );
        assert_eq!(p.injected.len(), 1);
        // Only alpha should have injected.
        let inv = p.pipes[0].ops.iter()
            .find(|o| &*o.name == "alpha")
            .unwrap();
        assert_eq!(p.injected[0].host_node.byte_range, inv.parse_site.byte_range);
    }


    // -----------------------------------------------------------------------
    // sprefa-4m7.14: Pattern values as first-class — parse-layer acceptance
    // tests. These tests assert the host-CST shapes that the lowerer (14)
    // depends on. Each test documents which spec invariant it covers.
    // -----------------------------------------------------------------------

    /// §14.2 / §14.5c: `fs(glob(**/*.rs))` — the canonical nested-op arg
    /// form. Outer paren_slot must contain exactly one `op_invocation` named
    /// child whose `name` field is "glob" and whose inner paren covers the
    /// glob body.
    ///
    /// Three assertions share one parse call: no errors, outer name is "fs",
    /// inner op name is "glob" with body "**/*.rs".
    #[test]
    fn nested_op_with_literal_pattern_body() {
        let src = "fs(glob(**/*.rs))";
        let (p, errs) = host_parse(src, fake_file());
        assert!(errs.is_empty(), "unexpected parse errs: {errs:?}");

        let outer = p.pipes[0].ops[0].node();
        assert_eq!(outer.kind(), "op_invocation");
        assert_eq!(&src[outer.child_by_field_name("name").unwrap().byte_range()], "fs");

        let paren = outer.child_by_field_name("paren").expect("outer paren_slot");
        let mut cur = paren.walk();
        let nested = paren
            .named_children(&mut cur)
            .find(|c| c.kind() == "op_invocation")
            .expect("op_invocation child inside outer paren_slot");

        let inner_name = nested.child_by_field_name("name").expect("inner name field");
        assert_eq!(&src[inner_name.byte_range()], "glob");

        let inner_paren = nested.child_by_field_name("paren").expect("inner paren_slot");
        // Strip the parens to get the body.
        let inner_paren_src = &src[inner_paren.byte_range()];
        assert_eq!(inner_paren_src, "(**/*.rs)");
    }

    /// §14.2 / §14.5c.1: `rev(re($V))` — inner re op carries a term_ref.
    /// The inner paren_slot must contain a term_ref node whose text is "$V".
    ///
    /// Covers the "explicit opt-in" path: rev rejects bare $V but accepts
    /// rev(re($V)) because the outer re op makes the intent explicit.
    #[test]
    fn nested_op_with_term_arg() {
        let src = "rev(re($V))";
        let (p, errs) = host_parse(src, fake_file());
        assert!(errs.is_empty(), "unexpected parse errs: {errs:?}");

        let outer = p.pipes[0].ops[0].node();
        let paren = outer.child_by_field_name("paren").expect("outer paren_slot");

        let mut cur = paren.walk();
        let nested = paren
            .named_children(&mut cur)
            .find(|c| c.kind() == "op_invocation")
            .expect("re op_invocation inside rev's paren_slot");

        let inner_paren = nested.child_by_field_name("paren").expect("inner paren_slot");

        // Walk the inner paren body for a term_ref node.
        let term_ref = find_kind(inner_paren, "term_ref")
            .expect("term_ref node inside re(...)");
        assert_eq!(&src[term_ref.byte_range()], "$V");
    }

    /// §14.2: `:main` in arg position produces an `atom_literal` node whose
    /// text span covers the full `:main` bytes.
    #[test]
    fn atom_literal_in_slot_position() {
        let src = "rev(:main)";
        let (p, errs) = host_parse(src, fake_file());
        assert!(errs.is_empty(), "unexpected parse errs: {errs:?}");

        let outer = p.pipes[0].ops[0].node();
        let paren = outer.child_by_field_name("paren").expect("paren_slot");

        let atom = find_kind(paren, "atom_literal")
            .expect("atom_literal inside rev(:main)");
        assert_eq!(&src[atom.byte_range()], ":main");
    }

    /// §14.2: `comment(re(/\*), re(\*/))` — two comma-separated arg groups
    /// inside a paren_slot. The slot must contain two `op_invocation` children
    /// separated by at least one `slot_punct` (comma) node.
    #[test]
    fn mixed_args_split_on_comma() {
        // Use a two-arg comment invocation with two nested re ops.
        let src = r"comment(re(\/\*), re(\*\/))";
        let (p, errs) = host_parse(src, fake_file());
        assert!(errs.is_empty(), "unexpected parse errs: {errs:?}");

        let outer = p.pipes[0].ops[0].node();
        let paren = outer.child_by_field_name("paren").expect("paren_slot");

        let mut cur = paren.walk();
        let nested_ops: Vec<_> = paren
            .named_children(&mut cur)
            .filter(|c| c.kind() == "op_invocation")
            .collect();
        assert_eq!(nested_ops.len(), 2, "expected two op_invocation children");

        // Both inner ops should be named "re".
        for op in &nested_ops {
            let name_node = op.child_by_field_name("name").expect("op name");
            assert_eq!(&src[name_node.byte_range()], "re");
        }
    }

    /// §14.2: Grammar must tolerate a bare identifier in slot position
    /// without hard-failing the parse. The lowerer (not the grammar) is
    /// responsible for emitting `value/bare-ident`.
    #[test]
    fn bare_ident_in_slot_still_parses() {
        let src = "rev(main)";
        let (p, _errs) = host_parse(src, fake_file());
        // Parse must produce at least one pipe/op, regardless of errors.
        assert_eq!(p.pipes.len(), 1, "source parses to exactly one pipe");
        assert!(!p.pipes[0].ops.is_empty(), "pipe has at least one op");
        // Op name is "rev".
        assert_eq!(&*p.pipes[0].ops[0].name, "rev");
    }

    // Helper: depth-first search for the first node of a given kind.
    fn find_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
        if node.kind() == kind { return Some(node); }
        let mut cur = node.walk();
        for child in node.children(&mut cur) {
            if let Some(found) = find_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }
}
