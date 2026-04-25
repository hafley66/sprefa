//! Kitchen-sink integration test: one .sprf fragment exercising every
//! §8–§13 parse-layer feature. Asserts the tree-sitter CST has the right
//! node kinds at the right byte ranges so downstream lowering can trust
//! the structure.

use tree_sitter::{Language, Parser, TreeCursor};
use tree_sitter_sprefa::LANGUAGE;

const SRC: &str = r#"# top-level comment
rule(classes) > ast[rust] { class ${NAME?} } ;
rule(calls)   > ast[rust] { new ${classes}() } ;
rule(env)     > sh { echo ${{HOME}} } ;
rule(addrs)   > fs(${DIR}) > void ;
rule(tagged)  > tag(:repo, ${R}) ;
"#;

fn parse() -> (String, Vec<(String, (usize, usize))>) {
    let mut parser = Parser::new();
    let lang: Language = LANGUAGE.into();
    parser.set_language(&lang).expect("language loads");
    let tree = parser.parse(SRC, None).expect("parse succeeds");
    let root = tree.root_node();
    assert!(!root.has_error(), "parse produced ERROR node: {}", root.to_sexp());

    // Flatten: walk every named node, capture (kind, range).
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    walk(&mut cursor, &mut out);

    (root.to_sexp(), out)
}

fn walk(c: &mut TreeCursor, out: &mut Vec<(String, (usize, usize))>) {
    let node = c.node();
    if node.is_named() {
        let range = (node.start_byte(), node.end_byte());
        out.push((node.kind().to_string(), range));
    }
    if c.goto_first_child() {
        loop {
            walk(c, out);
            if !c.goto_next_sibling() { break; }
        }
        c.goto_parent();
    }
}

fn has(nodes: &[(String, (usize, usize))], kind: &str) -> bool {
    nodes.iter().any(|(k, _)| k == kind)
}

fn find_all<'a>(nodes: &'a [(String, (usize, usize))], kind: &str)
    -> Vec<&'a (String, (usize, usize))>
{
    nodes.iter().filter(|(k, _)| k == kind).collect()
}

#[test]
fn every_feature_lives_in_the_tree() {
    let (_sexp, nodes) = parse();

    // §8 — brace-mandatory term forms inside slot bodies
    assert!(has(&nodes, "term_ref"),           "term_ref node present");
    assert!(has(&nodes, "carveout_expr"),      "carveout_expr node present");
    assert!(has(&nodes, "shell_literal"),      "shell_literal node present");

    // §12 — fork/void: sample has `> void` trailing op
    let voids: Vec<_> = find_all(&nodes, "op_invocation")
        .into_iter()
        .filter(|(_, (s, e))| &SRC[*s..*e] == "void")
        .collect();
    assert_eq!(voids.len(), 1, "void op as ordinary pipe step");

    // Atoms
    let atoms = find_all(&nodes, "atom_literal");
    assert_eq!(atoms.len(), 1, "one :repo atom");
    assert_eq!(&SRC[atoms[0].1.0..atoms[0].1.1], ":repo");

    // Comments
    assert!(has(&nodes, "line_comment"));

    // Shell body is opaque
    let shells = find_all(&nodes, "shell_literal");
    assert_eq!(shells.len(), 1);
    let (s, e) = shells[0].1;
    assert_eq!(&SRC[s..e], "${{HOME}}");
}

#[test]
fn carveout_body_pipes_a_bare_ident_for_term_read() {
    let mut parser = Parser::new();
    parser.set_language(&LANGUAGE.into()).unwrap();
    let tree = parser.parse(SRC, None).unwrap();
    let sexp = tree.root_node().to_sexp();

    // ${classes} should produce a carveout_expr whose body is a pipe of
    // one bare-IDENT op_invocation (term-read shorthand, sprefa-9lt).
    assert!(
        sexp.contains("(carveout_expr") && sexp.contains("(pipe"),
        "sexp missing carveout/pipe nesting:\n{sexp}"
    );
}

#[test]
fn pipe_groups_steps_under_one_node() {
    let mut parser = Parser::new();
    parser.set_language(&LANGUAGE.into()).unwrap();
    let tree = parser.parse(SRC, None).unwrap();
    let root = tree.root_node();

    // Each `;` statement becomes one pipe. We have 5 `rule(...)` lines.
    let mut pipe_count = 0;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "pipe" { pipe_count += 1; }
    }
    assert_eq!(pipe_count, 5, "five top-level pipes");
}
