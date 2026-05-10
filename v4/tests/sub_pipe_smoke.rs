//! #10 — ${pipe} sub-pipe carveouts in backtick DSL bodies.
//!
//! Carveout bodies have, until this lands, been term-shape only:
//!   ${X}    ${X?}    ${X.field}    ${&.field}
//! This lands op-shape: ${ <pipe> } drains a nested Pipe per outer
//! cursor and renders the drained cursor's value into the slot.

use std::sync::Arc;
use effect_runtime::v2::{
    expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend,
};
use v4::Cursor;
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};

fn run(src: &str) -> Arc<dyn FactStore<Cursor>> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse: {:?}", parse_diags);
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), std::env::temp_dir());
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk: {:?}",
        walk_diags.iter().map(|d| (d.code.as_ref(), d.message.as_str())).collect::<Vec<_>>());
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for pipe in pipes {
        let inst = pipe.into_instance();
        expand(&inst, queue.clone(), vec![Arc::new(Cursor::default())], ExpandOpts::default());
    }
    store
}

/// PRIMARY — op-shape carveout in a rule body backtick template.
/// Today: row value is literal `out=${ str`hi` }` text (regex didn't match)
///        OR walk diagnostic OR parse error.
/// After: nested str pipe drains "hi" cursor, slot renders to "out=hi".
#[test]
fn rule_body_renders_nested_str_subpipe() {
    let src = "rule(:greet) { `out=${ str`hi` }` };";
    let store = run(src);
    assert_eq!(store.len("greet"), 1, "expected one row");
    let rows = store.rows_of("greet");
    assert_eq!(&*rows[0].value, "out=hi",
        "sub-pipe should drain to \"hi\" and splice into the slot");
}

/// Multiple carveouts in one template — each independently drained.
#[test]
fn rule_body_renders_multiple_subpipes() {
    let src = "rule(:pair) { `${ str`a` }-${ str`b` }` };";
    let store = run(src);
    assert_eq!(store.len("pair"), 1);
    let rows = store.rows_of("pair");
    assert_eq!(&*rows[0].value, "a-b");
}

#[test]
fn subpipe_can_start_with_term_sugar_step() {
    let src = r#"
        rule(:types, TYPE?);
        rule(:docs, BODY?);

        `UserService` > TYPE? > types(TYPE);
        types?(TYPE?)
          > render_markdown`${ TYPE > `**${&.value}**` }`
          > BODY?
          > docs(BODY);
    "#;
    let store = run(src);
    let rows = store.rows_of("docs");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("BODY"), Some("**UserService**\n"));
}

#[test]
fn render_holes_can_nest_multiple_levels() {
    let src = r#"
        rule(:docs, BODY?);

        `leaf`
          > NAME?
          > render_markdown`outer ${ NAME > render_markdown`mid ${ NAME > render_markdown`inner ${&.value}` }` }`
          > BODY?
          > docs(BODY);
    "#;
    let store = run(src);
    let rows = store.rows_of("docs");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("BODY"), Some("outer mid inner leaf\n"));
}

/// Term-shape carveouts must keep working — regression sentinel.
#[test]
fn rule_body_term_carveout_still_works() {
    // Existing v4_e2e_smoke shape: cursor.value carried through unchanged.
    let src = "rule(:greet) { `hello world` };";
    let store = run(src);
    assert_eq!(store.len("greet"), 1);
    assert_eq!(&*store.rows_of("greet")[0].value, "hello world");
}
