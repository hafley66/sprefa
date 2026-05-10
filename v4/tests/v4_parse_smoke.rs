//! v4 parse smoke — ONE end-to-end test covering the four-slot shape.
//!
//! Per author convention: one smoke per port, not per-op granular files.

use v4::compile::parse::host_parse;

#[test]
fn host_parse_four_slot_and_diag_shape() {
    // 1. Happy path: four-slot example.
    //    `rule(:greet) ` + "`hi`" + ` { str ` + "`world`" + ` }`
    //
    //    Outer op:   name=rule, paren=[":greet"], dsl=Some("hi"),
    //                brace=PipeAst with one OpCall name=str, dsl=Some("world").
    let src = "rule(:greet)`hi`{ str`world` };";
    let (pipes, diags) = host_parse(src);
    assert!(diags.is_empty(), "expected clean parse, got diags: {diags:?}");
    assert_eq!(pipes.len(), 1, "one top-level pipe");
    let pipe = &pipes[0];
    assert_eq!(pipe.steps.len(), 1, "one op step");

    let outer = &pipe.steps[0];
    assert_eq!(&*outer.name, "rule");
    assert!(!outer.force);
    assert!(!outer.predicate);
    assert!(!outer.apply);

    let args = &outer.args;
    assert_eq!(args.len(), 1, "one paren arg, got {args:?}");
    assert_eq!(&*args[0].raw, ":greet");

    let dsl = outer.dsl.as_ref().expect("rule op has dsl slot");
    assert_eq!(&*dsl.raw, "hi");

    let block = outer.block.as_ref().expect("rule op has brace block");
    assert_eq!(block.steps.len(), 1, "one inner op");
    let inner = &block.steps[0];
    assert_eq!(&*inner.name, "str");
    let inner_dsl = inner.dsl.as_ref().expect("str op has dsl slot");
    assert_eq!(&*inner_dsl.raw, "world");

    // 2. Error path: doubled `>` separator. tree-sitter parses what it can
    //    and emits ERROR/MISSING nodes; we surface them as Diags. (The
    //    grammar is permissive on bare-identifier soup — most "oops syntax
    //    error" word salads parse as a sequence of pipes — so we pick a
    //    structurally illegal shape.)
    let src_bad = "foo > > bar;";
    let (_pipes_bad, diags_bad) = host_parse(src_bad);
    assert!(
        diags_bad.iter().any(|d| &*d.code == "parse/syntax-error"),
        "expected at least one parse/syntax-error diag, got: {diags_bad:?}"
    );

    let src_apply = "frontend_hooks!(OP, FILE);";
    let (pipes_apply, diags_apply) = host_parse(src_apply);
    assert!(diags_apply.is_empty(), "expected clean force apply parse, got diags: {diags_apply:?}");
    let call = &pipes_apply[0].steps[0];
    assert_eq!(&*call.name, "frontend_hooks");
    assert!(call.force);
    assert!(!call.predicate);
    assert!(!call.apply);
    assert_eq!(call.args.len(), 2);

    let src_query = "frontend_hooks?(OP?, FILE);";
    let (pipes_query, diags_query) = host_parse(src_query);
    assert!(diags_query.is_empty(), "expected clean rule query parse, got diags: {diags_query:?}");
    let call = &pipes_query[0].steps[0];
    assert_eq!(&*call.name, "frontend_hooks");
    assert!(!call.force);
    assert!(call.predicate);
    assert!(!call.apply);
    assert_eq!(call.args.len(), 2);

    let src_force = "db_size_over!(PATH, LIMIT);";
    let (pipes_force, diags_force) = host_parse(src_force);
    assert!(diags_force.is_empty(), "expected clean force parse, got diags: {diags_force:?}");
    let call = &pipes_force[0].steps[0];
    assert_eq!(&*call.name, "db_size_over");
    assert!(call.force);
    assert!(!call.predicate);
    assert!(!call.apply);

    let src_comment = "comment(`<!-- start -->`, `<!-- end -->`);";
    let (pipes_comment, diags_comment) = host_parse(src_comment);
    assert!(diags_comment.is_empty(), "expected clean comment arg parse, got diags: {diags_comment:?}");
    let call = &pipes_comment[0].steps[0];
    assert_eq!(&*call.name, "comment");
    assert_eq!(call.args.len(), 2);
    assert_eq!(&*call.args[0].raw, "`<!-- start -->`");
    assert_eq!(&*call.args[1].raw, "`<!-- end -->`");

    let src_render = "render.markdown`## Title`;";
    let (pipes_render, diags_render) = host_parse(src_render);
    assert!(diags_render.is_empty(), "expected clean dotted render parse, got diags: {diags_render:?}");
    let call = &pipes_render[0].steps[0];
    assert_eq!(&*call.name, "render.markdown");
    assert!(!call.apply);
    assert_eq!(&*call.dsl.as_ref().unwrap().raw, "## Title");
}
