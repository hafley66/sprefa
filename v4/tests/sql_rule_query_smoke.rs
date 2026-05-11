use std::sync::Arc;

use effect_runtime::v2::{expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend};

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

fn walk_diags(src: &str) -> Vec<effect_runtime::v2::Diag> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store, std::env::temp_dir());
    let (_pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    walk_diags
}

fn run_pipes(src: &str) -> Arc<dyn FactStore<Cursor>> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), std::env::temp_dir());
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk diags: {walk_diags:?}");

    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for pipe in pipes {
        expand(
            &pipe.into_instance(),
            queue.clone(),
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }

    store
}

#[test]
fn declared_rule_force_apply_policy_is_reserved() {
    let diags = walk_diags(
        r#"
        rule(:frontend_hooks);
        frontend_hooks!();
    "#,
    );

    assert!(
        diags
            .iter()
            .any(|d| d.code.as_ref() == "lower/rule-force-reserved"),
        "expected lower/rule-force-reserved diag, got {diags:?}"
    );
}

#[test]
fn sql_anti_join_finds_missing_frontend_hook() {
    let src = r#"
        rule(:frontend_hooks, OP?);
        rule(:missing_frontend_hooks, OP?);

        `getUser` > split(OP?)`!` > rule(:frontend_hooks);

        `getUser`
          > split(OP?)`!`
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
              )
            `
          > rule(:missing_frontend_hooks);

        `listPets`
          > split(OP?)`!`
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
              )
            `
          > rule(:missing_frontend_hooks);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("missing_frontend_hooks");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("listPets"));
}

#[test]
fn sql_anti_join_handles_declared_empty_rule_table() {
    let src = r#"
        rule(:frontend_hooks, OP?);
        rule(:missing_frontend_hooks, OP?);

        `listPets`
          > split(OP?)`!`
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
              )
            `
          > rule(:missing_frontend_hooks);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("missing_frontend_hooks");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("listPets"));
}

#[test]
fn declared_rule_call_projects_matching_rows() {
    let src = r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks);

        `getUser`
          > term_bind(:OP)
          > frontend_hooks?(OP, FILE?)
          > rule(:hook_hits);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("hook_hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("getUser"));
    assert_eq!(rows[0].get("FILE"), Some("src/hooks.ts"));
}

#[test]
fn rule_call_allows_kwarg_then_same_name_shorthand() {
    let src = r#"
        rule(:rule_a, X?, Y?, OUT_A?, OUT_B?);
        rule(:hits, X?, Y?, OUT_A?, OUT_B?);

        `x-val`
          > term_bind(:X)
          > `y-val`
          > term_bind(:Y)
          > `a-val`
          > term_bind(:OUT_A)
          > `b-val`
          > term_bind(:OUT_B)
          > rule_a(X, Y, OUT_A, OUT_B);

        `y-val`
          > term_bind(:Y)
          > rule_a?(X?, Y, OUT_A: OUT_A?, OUT_B?)
          > hits(X, Y, OUT_A, OUT_B);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("X"), Some("x-val"));
    assert_eq!(rows[0].get("Y"), Some("y-val"));
    assert_eq!(rows[0].get("OUT_A"), Some("a-val"));
    assert_eq!(rows[0].get("OUT_B"), Some("b-val"));
}

#[test]
fn rule_call_rejects_non_shorthand_positional_after_kwarg() {
    let diags = walk_diags(
        r#"
        rule(:rule_a, X?, Y?, OUT_A?);

        `y-val`
          > term_bind(:Y)
          > rule_a?(Y: Y, `literal`);
    "#,
    );

    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("positional arg after keyword arg")),
        "expected positional arg after keyword arg diag, got {diags:?}"
    );
}

#[test]
fn declared_empty_rule_apply_rejects_holes() {
    let src = r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks(OP: OP, FILE: FILE?)
          > rule(:hook_hits, OP: OP, FILE: FILE);
    "#;

    let diags = walk_diags(src);
    assert!(
        diags.iter().any(|d| d.code.as_ref() == "lower/rule-apply"),
        "expected lower/rule-apply diag for TERM? in apply, got {diags:?}"
    );
}

#[test]
fn declared_rule_dotted_apply_is_rejected() {
    let diags = walk_diags(
        r#"
        rule(:frontend_hooks, OP?, FILE?);
        `getUser` > term_bind(:OP) > `src/hooks.ts` > term_bind(:FILE) > frontend_hooks.(OP, FILE);
    "#,
    );

    assert!(
        diags
            .iter()
            .any(|d| d.code.as_ref() == "lower/rule-dotted-apply"),
        "expected lower/rule-dotted-apply diag, got {diags:?}"
    );
}

#[test]
fn declared_empty_rule_apply_writes_and_passes_through_with_grounded_args() {
    let src = r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:hook_hits, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks(OP: OP, FILE: FILE)
          > rule(:hook_hits, OP: OP, FILE: FILE);
    "#;

    let store = run_pipes(src);
    let frontend_rows = store.rows_of("frontend_hooks");
    assert_eq!(frontend_rows.len(), 1);
    assert_eq!(frontend_rows[0].get("OP"), Some("getUser"));
    assert_eq!(frontend_rows[0].get("FILE"), Some("src/hooks.ts"));
    assert!(frontend_rows[0].get("_id").is_some());

    let rows = store.rows_of("hook_hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("getUser"));
    assert_eq!(rows[0].get("FILE"), Some("src/hooks.ts"));
}

#[test]
fn declared_empty_rule_apply_accepts_backtick_literal_kwargs() {
    let src = r#"
        rule(:runtime_specs, NAME?, FILE?);

        runtime_specs(NAME: `Cursor`, FILE: `v4/src/lib.rs`);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("runtime_specs");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("NAME"), Some("Cursor"));
    assert_eq!(rows[0].get("FILE"), Some("v4/src/lib.rs"));
}

#[test]
fn declared_rule_query_accepts_backtick_literal_kwargs() {
    let src = r#"
        rule(:runtime_specs, NAME?, FILE?);
        rule(:hits, NAME?, FILE?);

        runtime_specs(NAME: `Cursor`, FILE: `v4/src/lib.rs`);

        runtime_specs?(NAME: `Cursor`, FILE?)
          > hits(NAME, FILE);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("hits");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("NAME"), Some("Cursor"));
    assert_eq!(rows[0].get("FILE"), Some("v4/src/lib.rs"));
}

#[test]
fn declared_rule_query_does_not_write_empty_rule() {
    let src = r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:checked_hooks, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks?(OP, FILE)
          > rule(:checked_hooks);
    "#;

    let store = run_pipes(src);
    assert_eq!(store.rows_of("frontend_hooks").len(), 0);
    let rows = store.rows_of("checked_hooks");
    assert_eq!(rows.len(), 0);
}

#[test]
fn declared_rule_grounded_query_dedupes_same_output_cursor() {
    let src = r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:checked_hooks, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks(OP, FILE);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks(OP, FILE);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks?(OP, FILE)
          > rule(:checked_hooks);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("checked_hooks");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("getUser"));
    assert_eq!(rows[0].get("FILE"), Some("src/hooks.ts"));
}

#[test]
fn declared_rule_predicate_syntax_queries_relation() {
    let src = r#"
        rule(:frontend_hooks, OP?);
        rule(:checked_hooks, OP?);

        `getUser` > term_bind(:OP) > frontend_hooks(OP);
        `getUser` > term_bind(:OP) > frontend_hooks?(OP) > checked_hooks(OP);
    "#;

    let store = run_pipes(src);
    let rows = store.rows_of("checked_hooks");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("OP"), Some("getUser"));
}

#[test]
fn declared_dotted_rule_name_writes_and_queries() {
    let src = r#"
        rule(:docs.runtime_node, NAME?, FILE?);
        rule(:docs.hit, NAME?, FILE?);

        `Cursor`
          > term_bind(:NAME)
          > `v4/src/lib.rs`
          > term_bind(:FILE)
          > docs.runtime_node(NAME, FILE);

        `Cursor`
          > term_bind(:NAME)
          > docs.runtime_node?(NAME, FILE?)
          > docs.hit(NAME, FILE);
    "#;

    let store = run_pipes(src);
    let runtime_rows = store.rows_of("docs.runtime_node");
    assert_eq!(runtime_rows.len(), 1);
    assert_eq!(runtime_rows[0].get("NAME"), Some("Cursor"));
    assert_eq!(runtime_rows[0].get("FILE"), Some("v4/src/lib.rs"));

    let hit_rows = store.rows_of("docs.hit");
    assert_eq!(hit_rows.len(), 1);
    assert_eq!(hit_rows[0].get("NAME"), Some("Cursor"));
    assert_eq!(hit_rows[0].get("FILE"), Some("v4/src/lib.rs"));
}
