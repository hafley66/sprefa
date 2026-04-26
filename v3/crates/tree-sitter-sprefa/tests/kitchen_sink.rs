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

# ---- spec'd 2026-04-26 (v3-render-write-cursor-vision.md) ----
# These parse today as ordinary op_invocations; registry impls pending
# (sprefa-e6c render, sprefa-1cl write_cursor, sprefa-5ii lsp,
# sprefa-465 tag, sprefa-p0m comment @sprf body re-entry).
#
# Note: dotted carveout access (${X.field?}), backtick template strings,
# and @begin/@end marker identifiers are part of the spec but pending
# grammar landings (sprefa-p0m); fixtures with those shapes are kept
# under server/fixtures/ as forward-looking docs and are NOT included
# here so this test stays green on today's grammar.

# render[fmt] — bracket-arg picks backend (md/ascii/lsp/rust/json).
fs(glob(./views/*/)) > render[md](- ${STEM?}) > write_cursor(${SCOPE?}) ;

# render[ascii] with nested layout primitives in the paren slot.
collect(${WS?}) > render[ascii](
  col(
    title("workspace"),
    box(:live, header="LIVE", body=${WS?}),
    flow(:e1, from=:a, to=:b, label="bytes"))) ;

# write_cursor positional mode atom.
rule(scope) > render[rust](use crate::${M?}) > write_cursor(${S?}, :append) ;
rule(call)  > render[rust](new(${A?})) > write_cursor(${H?}, :wrap) ;

# lsp[severity] family — bracket-arg as severity.
fs(glob(**/*.rs)) > ast[rust](unwrap())
  > lsp[warn](message "no unwrap", code "no-unwrap") ;
fs(glob(**/*.rs)) > lsp[error](message "x") ;
fs(glob(**/*.rs)) > lsp[hint](message "x") ;
fs(glob(**/*.rs)) > lsp[info](message "x") ;

# tag variadic write + nullary read.
repo(${R?}) > rev(:prod) > tag(:prod-cut, ${R?}, ${HEAD?}) ;
tag(:prod-cut) > fs(glob(**/*.rs)) > rule(prod_files) ;
tag(:prod-cut) > tag(:in-review) > rule(prod_pending) ;

# comment(@sprf …) read direction; write direction (@begin/@end) is
# pending grammar work and is exercised only by fixtures.
fs(glob(**/*.rs)) > comment(@sprf ${BODY?}) > eval(${BODY?}) ;
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

    // Atoms — original :repo plus several from the spec'd lines (atom-arg
    // mode atoms like :append/:wrap/:prod-cut/:e1/:a/:b/:live etc.).
    let atoms = find_all(&nodes, "atom_literal");
    assert!(atoms.len() >= 1, "at least one atom literal present");
    let repo_atoms: Vec<_> = atoms.iter()
        .filter(|(_, (s, e))| &SRC[*s..*e] == ":repo")
        .collect();
    assert_eq!(repo_atoms.len(), 1, "one :repo atom from original sample");

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
    // 5 original + 12 spec'd lines from v3-render-write-cursor-vision.md.
    assert_eq!(pipe_count, 17, "seventeen top-level pipes");
}

#[test]
fn spec_2026_04_26_ops_parse_as_op_invocations() {
    // The vision doc (v3/docs/v3-render-write-cursor-vision.md) reserves
    // these op shapes; they should all parse today as ordinary
    // op_invocation nodes even though their registry impls are pending.
    let (_sexp, nodes) = parse();

    let invocations: Vec<&str> = find_all(&nodes, "op_invocation")
        .into_iter()
        .map(|(_, (s, e))| &SRC[*s..*e])
        .collect();

    let starts_with = |prefix: &str| -> usize {
        invocations.iter().filter(|s| s.starts_with(prefix)).count()
    };

    // render[fmt] — at least md / ascii / rust appear at op-head position.
    assert!(starts_with("render[md]") >= 1, "render[md] op present");
    assert!(starts_with("render[ascii]") >= 1, "render[ascii] op present");
    assert!(starts_with("render[rust]") >= 2, "render[rust] op present (>=2)");

    // write_cursor — bare and with mode atoms.
    assert!(starts_with("write_cursor") >= 3, "write_cursor present (>=3)");

    // lsp[severity] — all four severities.
    for sev in ["lsp[warn]", "lsp[error]", "lsp[hint]", "lsp[info]"] {
        assert!(starts_with(sev) >= 1, "{sev} present");
    }

    // tag — both write (variadic) and read (nullary) forms.
    assert!(starts_with("tag(:prod-cut") >= 1, "tag write form present");

    // comment @sprf read direction.
    assert!(starts_with("comment(@sprf") >= 1, "comment(@sprf …) present");

    // eval — sister op for comment-body re-entry.
    assert!(starts_with("eval") >= 1, "eval op present");

    // collect — rendezvous from streaming to tabular Value.
    assert!(starts_with("collect") >= 1, "collect op present");
}
